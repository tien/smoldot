// Smoldot
// Copyright (C) 2019-2022  Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

#![deny(rustdoc::broken_intra_doc_links)]
// TODO: #![deny(unused_crate_dependencies)] doesn't work because some deps are used only by the binary, figure if this can be fixed?

use futures_channel::oneshot;
use futures_util::{future, stream, FutureExt as _, StreamExt as _};
use smol::lock::Mutex;
use smoldot::{
    chain, chain_spec,
    database::full_sqlite,
    executor, header,
    identity::keystore,
    informant::HashDisplay,
    libp2p::{
        connection, multiaddr,
        peer_id::{self, PeerId},
    },
};
use std::{borrow::Cow, iter, net::SocketAddr, path::PathBuf, sync::Arc, thread, time::Duration};

mod consensus_service;
mod database_thread;
mod jaeger_service;
mod json_rpc_service;
mod network_service;
mod util;

pub struct Config<'a> {
    /// Chain to connect to.
    pub chain: ChainConfig<'a>,
    /// If [`Config::chain`] contains a parachain, this field contains the configuration of the
    /// relay chain.
    pub relay_chain: Option<ChainConfig<'a>>,
    /// Ed25519 private key of network identity.
    pub libp2p_key: [u8; 32],
    /// List of addresses to listen on.
    pub listen_addresses: Vec<multiaddr::Multiaddr>,
    /// Bind point of the JSON-RPC server. If `None`, no server is started.
    pub json_rpc_address: Option<SocketAddr>,
    /// Function that can be used to spawn background tasks.
    ///
    /// The tasks passed as parameter must be executed until they shut down.
    pub tasks_executor: Arc<dyn Fn(future::BoxFuture<'static, ()>) + Send + Sync>,
    /// Function called whenever a part of the node wants to notify of something.
    pub log_callback: Arc<dyn LogCallback + Send + Sync>,
    /// Address of a Jaeger agent to send traces to. If `None`, do not send Jaeger traces.
    pub jaeger_agent: Option<SocketAddr>,
    // TODO: option is a bit weird
    pub show_informant: bool,
}

/// Allow generating logs.
///
/// Implemented on closures.
///
/// > **Note**: The `log` crate isn't used because dependencies complete pollute the logs.
pub trait LogCallback {
    /// Add a log entry.
    fn log(&self, log_level: LogLevel, message: String);
}

impl<T: ?Sized + Fn(LogLevel, String)> LogCallback for T {
    fn log(&self, log_level: LogLevel, message: String) {
        (*self)(log_level, message)
    }
}

/// Log level of a log entry.
#[derive(Debug)]
pub enum LogLevel {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

#[derive(Debug)]
pub struct ChainConfig<'a> {
    /// Specification of the chain.
    pub chain_spec: Cow<'a, [u8]>,
    /// Identity and address of nodes to try to connect to on startup.
    pub additional_bootnodes: Vec<(peer_id::PeerId, multiaddr::Multiaddr)>,
    /// List of secret phrases to insert in the keystore of the node. Used to author blocks.
    // TODO: also automatically add the same keys through ed25519?
    pub keystore_memory: Vec<[u8; 64]>,
    /// Path to the SQLite database. If `None`, the database is opened in memory.
    pub sqlite_database_path: Option<PathBuf>,
    /// Maximum size, in bytes, of the cache SQLite uses.
    pub sqlite_cache_size: usize,
    /// Path to the directory where cryptographic keys are stored on disk.
    ///
    /// If `None`, no keys are stored in disk.
    pub keystore_path: Option<PathBuf>,
}

/// Running client. As long as this object is alive, the client reads/writes the database and has
/// a JSON-RPC server open.
pub struct Client {
    _json_rpc_service: Option<json_rpc_service::JsonRpcService>,
    consensus_service: Arc<consensus_service::ConsensusService>,
    relay_chain_consensus_service: Option<Arc<consensus_service::ConsensusService>>,
    network_service: Arc<network_service::NetworkService>,
    network_known_best: Arc<Mutex<Option<u64>>>,
}

impl Client {
    /// Returns the best block according to the networking.
    pub async fn network_known_best(&self) -> Option<u64> {
        *self.network_known_best.lock().await
    }

    /// Returns the current number of peers of the client.
    pub async fn num_peers(&self) -> u64 {
        u64::try_from(self.network_service.num_peers(0).await).unwrap_or(u64::max_value())
    }

    /// Returns the current number of network connections of the client.
    pub async fn num_network_connections(&self) -> u64 {
        u64::try_from(self.network_service.num_established_connections().await)
            .unwrap_or(u64::max_value())
    }

    // TODO: not the best API
    pub async fn sync_state(&self) -> consensus_service::SyncState {
        self.consensus_service.sync_state().await
    }

    // TODO: not the best API
    pub async fn relay_chain_sync_state(&self) -> Option<consensus_service::SyncState> {
        if let Some(s) = &self.relay_chain_consensus_service {
            Some(s.sync_state().await)
        } else {
            None
        }
    }
}

/// Runs the node using the given configuration. Catches `SIGINT` signals and stops if one is
/// detected.
// TODO: should return an error if something bad happens instead of panicking
pub async fn start(config: Config<'_>) -> Client {
    let chain_spec = {
        smoldot::chain_spec::ChainSpec::from_json_bytes(&config.chain.chain_spec)
            .expect("Failed to decode chain specs")
    };

    // TODO: don't unwrap?
    let genesis_chain_information = chain_spec.to_chain_information().unwrap().0;

    let relay_chain_spec = config.relay_chain.as_ref().map(|rc| {
        smoldot::chain_spec::ChainSpec::from_json_bytes(&rc.chain_spec)
            .expect("Failed to decode relay chain chain specs")
    });

    // TODO: don't unwrap?
    let relay_genesis_chain_information = relay_chain_spec
        .as_ref()
        .map(|relay_chain_spec| relay_chain_spec.to_chain_information().unwrap().0);

    // Printing the SQLite version number can be useful for debugging purposes for example in case
    // a query fails.
    config.log_callback.log(
        LogLevel::Debug,
        format!("sqlite-version; version={}", full_sqlite::sqlite_version()),
    );

    let (database, database_existed) = {
        let (db, existed) = open_database(
            &chain_spec,
            genesis_chain_information.as_ref(),
            config.chain.sqlite_database_path,
            config.chain.sqlite_cache_size,
            config.show_informant,
        )
        .await;

        (Arc::new(database_thread::DatabaseThread::from(db)), existed)
    };

    let relay_chain_database = if let Some(relay_chain) = &config.relay_chain {
        Some(Arc::new(database_thread::DatabaseThread::from(
            open_database(
                relay_chain_spec.as_ref().unwrap(),
                relay_genesis_chain_information.as_ref().unwrap().as_ref(),
                relay_chain.sqlite_database_path.clone(),
                relay_chain.sqlite_cache_size,
                config.show_informant,
            )
            .await
            .0,
        )))
    } else {
        None
    };

    let database_finalized_block_hash = database
        .with_database(|db| db.finalized_block_hash().unwrap())
        .await;
    let database_finalized_block_number = header::decode(
        &database
            .with_database(move |db| {
                db.block_scale_encoded_header(&database_finalized_block_hash)
                    .unwrap()
                    .unwrap()
            })
            .await,
        chain_spec.block_number_bytes().into(),
    )
    .unwrap()
    .number;

    let noise_key = connection::NoiseKey::new(&config.libp2p_key);
    let local_peer_id =
        peer_id::PublicKey::Ed25519(*noise_key.libp2p_public_ed25519_key()).into_peer_id();

    let genesis_block_hash = genesis_chain_information
        .as_ref()
        .finalized_block_header
        .hash(chain_spec.block_number_bytes().into());

    let jaeger_service = jaeger_service::JaegerService::new(jaeger_service::Config {
        tasks_executor: &mut |task| (config.tasks_executor)(task),
        service_name: local_peer_id.to_string(),
        jaeger_agent: config.jaeger_agent,
    })
    .await
    .unwrap();

    let (network_service, network_events_receivers) =
        network_service::NetworkService::new(network_service::Config {
            listen_addresses: config.listen_addresses,
            num_events_receivers: 2 + if relay_chain_database.is_some() { 1 } else { 0 },
            chains: iter::once(network_service::ChainConfig {
                fork_id: chain_spec.fork_id().map(|n| n.to_owned()),
                block_number_bytes: usize::from(chain_spec.block_number_bytes()),
                database: database.clone(),
                has_grandpa_protocol: matches!(
                    genesis_chain_information.as_ref().finality,
                    chain::chain_information::ChainInformationFinalityRef::Grandpa { .. }
                ),
                genesis_block_hash,
                best_block: {
                    let block_number_bytes = chain_spec.block_number_bytes();
                    database
                        .with_database(move |database| {
                            let hash = database.finalized_block_hash().unwrap();
                            let header = database.block_scale_encoded_header(&hash).unwrap().unwrap();
                            let number = header::decode(&header, block_number_bytes.into(),).unwrap().number;
                            (number, hash)
                        })
                        .await
                },
                bootstrap_nodes: {
                    let mut list = Vec::with_capacity(
                        chain_spec.boot_nodes().len() + config.chain.additional_bootnodes.len(),
                    );

                    for node in chain_spec.boot_nodes() {
                        match node {
                            chain_spec::Bootnode::UnrecognizedFormat(raw) => {
                                panic!("Failed to parse bootnode in chain specification: {raw}")
                            }
                            chain_spec::Bootnode::Parsed { multiaddr, peer_id } => {
                                let multiaddr: multiaddr::Multiaddr = match multiaddr.parse() {
                                    Ok(a) => a,
                                    Err(_) => panic!(
                                        "Failed to parse bootnode in chain specification: {multiaddr}"
                                    ),
                                };
                                let peer_id = PeerId::from_bytes(peer_id.to_vec()).unwrap();
                                list.push((peer_id, multiaddr));
                            }
                        }
                    }

                    list.extend(config.chain.additional_bootnodes);
                    list
                },
            })
            .chain(
                if let Some(relay_chains_specs) = &relay_chain_spec {
                    Some(network_service::ChainConfig {
                        fork_id: relay_chains_specs.fork_id().map(|n| n.to_owned()),
                        block_number_bytes: usize::from(relay_chains_specs.block_number_bytes()),
                        database: relay_chain_database.clone().unwrap(),
                        has_grandpa_protocol: matches!(
                            relay_genesis_chain_information.as_ref().unwrap().as_ref().finality,
                            chain::chain_information::ChainInformationFinalityRef::Grandpa { .. }
                        ),
                        genesis_block_hash: relay_genesis_chain_information
                            .as_ref()
                            .unwrap()
                            .as_ref().finalized_block_header
                            .hash(chain_spec.block_number_bytes().into(),),
                        best_block: relay_chain_database
                            .as_ref()
                            .unwrap()
                            .with_database({
                                let block_number_bytes = chain_spec.block_number_bytes();
                                move |db| {
                                    let hash = db.finalized_block_hash().unwrap();
                                    let header = db.block_scale_encoded_header(&hash).unwrap().unwrap();
                                    let number = header::decode(&header, block_number_bytes.into()).unwrap().number;
                                    (number, hash)
                                }
                            })
                            .await,
                        bootstrap_nodes: {
                            let mut list =
                                Vec::with_capacity(relay_chains_specs.boot_nodes().len());
                            for node in relay_chains_specs.boot_nodes() {
                                match node {
                                    chain_spec::Bootnode::UnrecognizedFormat(raw) => {
                                        panic!("Failed to parse bootnode in chain specification: {raw}")
                                    }
                                    chain_spec::Bootnode::Parsed { multiaddr, peer_id } => {
                                        let multiaddr: multiaddr::Multiaddr = match multiaddr.parse() {
                                            Ok(a) => a,
                                            Err(_) => panic!(
                                                "Failed to parse bootnode in chain specification: {multiaddr}"
                                            ),
                                        };
                                        let peer_id = PeerId::from_bytes(peer_id.to_vec()).unwrap();
                                        list.push((peer_id, multiaddr));
                                    }
                                }
                            }
                            list
                        },
                    })
                } else {
                    None
                }
                .into_iter(),
            )
            .collect(),
            identify_agent_version: concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION")).to_owned(),
            noise_key,
            tasks_executor: {
                let executor = config.tasks_executor.clone();
                Box::new(move |task| executor(task))
            },
            log_callback: config.log_callback.clone(),
            jaeger_service: jaeger_service.clone(),
        })
        .await
        .unwrap();

    let mut network_events_receivers = network_events_receivers.into_iter();

    let keystore = Arc::new({
        let mut keystore = keystore::Keystore::new(config.chain.keystore_path, rand::random())
            .await
            .unwrap();
        for private_key in config.chain.keystore_memory {
            keystore.insert_sr25519_memory(keystore::KeyNamespace::all(), &private_key);
        }
        keystore
    });

    let consensus_service = consensus_service::ConsensusService::new(consensus_service::Config {
        tasks_executor: {
            let executor = config.tasks_executor.clone();
            Box::new(move |task| executor(task))
        },
        log_callback: config.log_callback.clone(),
        genesis_block_hash,
        network_events_receiver: network_events_receivers.next().unwrap(),
        network_service: (network_service.clone(), 0),
        database,
        block_number_bytes: usize::from(chain_spec.block_number_bytes()),
        keystore,
        jaeger_service: jaeger_service.clone(),
        slot_duration_author_ratio: 43691_u16,
    })
    .await;

    let relay_chain_consensus_service = if let Some(relay_chain_database) = relay_chain_database {
        Some(
            consensus_service::ConsensusService::new(consensus_service::Config {
                tasks_executor: {
                    let executor = config.tasks_executor.clone();
                    Box::new(move |task| executor(task))
                },
                log_callback: config.log_callback.clone(),
                genesis_block_hash: relay_genesis_chain_information
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .finalized_block_header
                    .hash(usize::from(
                        relay_chain_spec.as_ref().unwrap().block_number_bytes(),
                    )),
                network_events_receiver: network_events_receivers.next().unwrap(),
                network_service: (network_service.clone(), 1),
                database: relay_chain_database,
                block_number_bytes: usize::from(
                    relay_chain_spec.as_ref().unwrap().block_number_bytes(),
                ),
                keystore: Arc::new({
                    let mut keystore = keystore::Keystore::new(
                        config.relay_chain.as_ref().unwrap().keystore_path.clone(),
                        rand::random(),
                    )
                    .await
                    .unwrap();
                    for private_key in &config.relay_chain.as_ref().unwrap().keystore_memory {
                        keystore.insert_sr25519_memory(keystore::KeyNamespace::all(), private_key);
                    }
                    keystore
                }),
                jaeger_service, // TODO: consider passing a different jaeger service with a different service name
                slot_duration_author_ratio: 43691_u16,
            })
            .await,
        )
    } else {
        None
    };

    // Start the JSON-RPC service.
    // It only needs to be kept alive in order to function.
    //
    // Note that initialization can panic if, for example, the port is already occupied. It is
    // preferable to fail to start the node altogether rather than make the user believe that they
    // are connected to the JSON-RPC endpoint of the node while they are in reality connected to
    // something else.
    let json_rpc_service = if let Some(bind_address) = config.json_rpc_address {
        let result = json_rpc_service::JsonRpcService::new(json_rpc_service::Config {
            tasks_executor: { &mut |task| (config.tasks_executor)(task) },
            log_callback: config.log_callback.clone(),
            bind_address,
        })
        .await;

        Some(match result {
            Ok(service) => service,
            Err(err) => panic!("failed to initialize JSON-RPC endpoint: {err}"),
        })
    } else {
        None
    };

    // Spawn the task printing the informant.
    // This is not just a dummy task that just prints on the output, but is actually the main
    // task that holds everything else alive. Without it, all the services that we have created
    // above would be cleanly dropped and nothing would happen.
    // For this reason, it must be spawned even if no informant is started, in which case we simply
    // inhibit the printing.
    let network_known_best = Arc::new(Mutex::new(None));
    (config.tasks_executor)(Box::pin({
        let mut main_network_events_receiver = network_events_receivers.next().unwrap();
        let network_known_best = network_known_best.clone();

        // TODO: shut down this task if the client stops?
        async move {
            loop {
                let network_event = main_network_events_receiver.next().await.unwrap();
                let mut network_known_best = network_known_best.lock().await;

                match network_event {
                    network_service::Event::BlockAnnounce {
                        chain_index: 0,
                        scale_encoded_header,
                        ..
                    } => match (
                        *network_known_best,
                        header::decode(
                            &scale_encoded_header,
                            usize::from(chain_spec.block_number_bytes()),
                        ),
                    ) {
                        (Some(n), Ok(header)) if n >= header.number => {}
                        (_, Ok(header)) => *network_known_best = Some(header.number),
                        (_, Err(_)) => {
                            // Do nothing if the block is invalid. This is just for the
                            // informant and not for consensus-related purposes.
                        }
                    },
                    network_service::Event::Connected {
                        chain_index: 0,
                        best_block_number,
                        ..
                    } => match *network_known_best {
                        Some(n) if n >= best_block_number => {}
                        _ => *network_known_best = Some(best_block_number),
                    },
                    _ => {}
                }
            }
        }
    }));

    config.log_callback.log(
        LogLevel::Info,
        format!(
            "successful-initialization; local_peer_id={}; database_is_new={:?}; \
                finalized_block_hash={}; finalized_block_number={}",
            local_peer_id,
            !database_existed,
            HashDisplay(&database_finalized_block_hash),
            database_finalized_block_number
        ),
    );

    debug_assert!(network_events_receivers.next().is_none());
    Client {
        consensus_service,
        relay_chain_consensus_service,
        _json_rpc_service: json_rpc_service,
        network_service,
        network_known_best,
    }
}

/// Opens the database from the file system, or create a new database if none is found.
///
/// If `db_path` is `None`, open the database in memory instead.
///
/// The returned boolean is `true` if the database existed before.
///
/// # Panic
///
/// Panics if the database can't be open. This function is expected to be called from the `main`
/// function.
///
// TODO: `show_progress` option should be moved to the CLI
async fn open_database(
    chain_spec: &chain_spec::ChainSpec,
    genesis_chain_information: chain::chain_information::ChainInformationRef<'_>,
    db_path: Option<PathBuf>,
    sqlite_cache_size: usize,
    show_progress: bool,
) -> (full_sqlite::SqliteFullDatabase, bool) {
    // The `unwrap()` here can panic for example in case of access denied.
    match background_open_database(
        db_path.clone(),
        chain_spec.block_number_bytes().into(),
        sqlite_cache_size,
        show_progress,
    )
    .await
    .unwrap()
    {
        // Database already exists and contains data.
        full_sqlite::DatabaseOpen::Open(database) => {
            if database.block_hash_by_number(0).unwrap().next().unwrap()
                != genesis_chain_information
                    .finalized_block_header
                    .hash(chain_spec.block_number_bytes().into())
            {
                panic!("Mismatch between database and chain specification. Shutting down node.");
            }

            (database, true)
        }

        // The database doesn't exist or is empty.
        full_sqlite::DatabaseOpen::Empty(empty) => {
            let genesis_storage = chain_spec.genesis_storage().into_genesis_items().unwrap(); // TODO: return error instead

            // In order to determine the state_version of the genesis block, we need to compile
            // the runtime.
            // TODO: return errors instead of panicking
            let state_version = executor::host::HostVmPrototype::new(executor::host::Config {
                module: genesis_storage.value(b":code").unwrap(),
                heap_pages: executor::storage_heap_pages_to_value(
                    genesis_storage.value(b":heappages"),
                )
                .unwrap(),
                exec_hint: executor::vm::ExecHint::Oneshot,
                allow_unresolved_imports: true,
            })
            .unwrap()
            .runtime_version()
            .decode()
            .state_version
            .map(u8::from)
            .unwrap_or(0);

            // The finalized block is the genesis block. As such, it has an empty body and
            // no justification.
            let database = empty
                .initialize(
                    genesis_chain_information,
                    iter::empty(),
                    None,
                    genesis_storage.iter(),
                    state_version,
                )
                .unwrap();
            (database, false)
        }
    }
}

/// Since opening the database can take a long time, this utility function performs this operation
/// in the background while showing a small progress bar to the user.
///
/// If `path` is `None`, the database is opened in memory.
async fn background_open_database(
    path: Option<PathBuf>,
    block_number_bytes: usize,
    sqlite_cache_size: usize,
    show_progress: bool,
) -> Result<full_sqlite::DatabaseOpen, full_sqlite::InternalError> {
    let (tx, rx) = oneshot::channel();
    let mut rx = rx.fuse();

    let thread_spawn_result = thread::Builder::new().name("database-open".into()).spawn({
        let path = path.clone();
        move || {
            let result = full_sqlite::open(full_sqlite::Config {
                block_number_bytes,
                cache_size: sqlite_cache_size,
                ty: if let Some(path) = &path {
                    full_sqlite::ConfigTy::Disk(path)
                } else {
                    full_sqlite::ConfigTy::Memory
                },
            });
            let _ = tx.send(result);
        }
    });

    // Fall back to opening the database on the same thread if the thread spawn failed.
    if thread_spawn_result.is_err() {
        return full_sqlite::open(full_sqlite::Config {
            block_number_bytes,
            cache_size: sqlite_cache_size,
            ty: if let Some(path) = &path {
                full_sqlite::ConfigTy::Disk(path)
            } else {
                full_sqlite::ConfigTy::Memory
            },
        });
    }

    let mut progress_timer =
        stream::StreamExt::fuse(smol::Timer::after(Duration::from_millis(200)));

    let mut next_progress_icon = ['-', '\\', '|', '/'].iter().copied().cycle();

    loop {
        futures_util::select! {
            res = rx => return res.unwrap(),
            _ = progress_timer.next() => {
                if show_progress {
                    eprint!("    Opening database... {}\r", next_progress_icon.next().unwrap());
                }
            }
        }
    }
}

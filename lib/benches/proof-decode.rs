// Smoldot
// Copyright (C) 2023  Pierre Krieger
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

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

fn benchmark_proof_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("proof-decode");

    let proofs: &[&[u8]] = &[&[
        24, 212, 125, 1, 84, 37, 150, 173, 176, 93, 97, 64, 193, 112, 172, 71, 158, 223, 124, 253,
        90, 163, 83, 87, 89, 10, 207, 229, 209, 26, 128, 77, 148, 78, 80, 13, 20, 86, 253, 218,
        123, 142, 199, 249, 229, 199, 148, 205, 131, 25, 79, 5, 147, 228, 234, 53, 5, 128, 63, 147,
        128, 78, 76, 108, 66, 34, 183, 71, 229, 7, 0, 142, 241, 222, 240, 99, 187, 13, 45, 238,
        173, 241, 126, 244, 177, 14, 113, 98, 77, 58, 12, 248, 28, 128, 36, 31, 44, 6, 242, 46,
        197, 137, 104, 251, 104, 212, 50, 49, 158, 37, 230, 200, 250, 163, 173, 44, 92, 169, 238,
        72, 242, 232, 237, 21, 142, 36, 128, 173, 138, 104, 35, 73, 50, 38, 152, 70, 188, 64, 36,
        10, 71, 207, 216, 216, 133, 123, 29, 129, 225, 103, 191, 178, 76, 148, 122, 76, 218, 217,
        230, 128, 200, 69, 144, 227, 159, 139, 121, 162, 105, 74, 210, 191, 126, 114, 88, 175, 104,
        107, 71, 47, 56, 176, 100, 187, 206, 125, 8, 64, 73, 49, 164, 48, 128, 92, 114, 242, 91,
        27, 99, 4, 209, 102, 103, 226, 118, 111, 161, 169, 6, 203, 8, 23, 136, 235, 69, 2, 120,
        125, 247, 195, 89, 116, 18, 177, 123, 128, 110, 33, 197, 241, 162, 74, 25, 102, 21, 180,
        229, 179, 109, 33, 40, 12, 220, 200, 0, 152, 193, 226, 188, 232, 238, 175, 48, 30, 153, 81,
        118, 116, 128, 66, 79, 26, 205, 128, 186, 7, 74, 44, 232, 209, 128, 191, 52, 136, 165, 202,
        145, 203, 129, 251, 169, 108, 140, 60, 29, 51, 234, 203, 177, 129, 96, 128, 94, 132, 157,
        92, 20, 140, 163, 97, 165, 90, 44, 155, 56, 78, 23, 206, 145, 158, 147, 108, 203, 128, 17,
        164, 247, 37, 4, 233, 249, 61, 184, 205, 128, 237, 208, 5, 161, 73, 92, 112, 37, 13, 119,
        248, 28, 36, 193, 90, 153, 25, 240, 52, 247, 152, 61, 248, 229, 5, 229, 58, 90, 247, 180,
        2, 19, 128, 18, 160, 221, 144, 73, 123, 101, 49, 43, 218, 103, 234, 21, 153, 101, 120, 238,
        179, 137, 27, 202, 134, 102, 149, 26, 50, 102, 18, 65, 142, 49, 67, 177, 4, 128, 85, 93,
        128, 67, 251, 73, 124, 27, 42, 123, 158, 79, 235, 89, 244, 16, 193, 162, 158, 40, 178, 166,
        40, 255, 156, 96, 3, 224, 128, 246, 185, 250, 221, 149, 249, 128, 110, 141, 145, 27, 104,
        24, 3, 142, 183, 200, 83, 74, 248, 231, 142, 153, 32, 161, 171, 141, 147, 156, 54, 211,
        230, 155, 10, 30, 89, 40, 17, 11, 128, 186, 77, 63, 84, 57, 87, 244, 34, 180, 12, 142, 116,
        175, 157, 224, 10, 203, 235, 168, 21, 74, 252, 165, 122, 127, 128, 251, 188, 254, 187, 30,
        74, 128, 61, 27, 143, 92, 241, 120, 139, 41, 69, 55, 184, 253, 45, 52, 172, 236, 70, 70,
        167, 98, 124, 108, 211, 210, 3, 154, 246, 79, 245, 209, 151, 109, 128, 231, 98, 15, 33,
        207, 19, 150, 79, 41, 211, 75, 167, 8, 195, 180, 78, 164, 94, 161, 28, 88, 251, 190, 221,
        162, 157, 19, 71, 11, 200, 12, 160, 128, 249, 138, 174, 79, 131, 216, 27, 241, 93, 136, 1,
        158, 92, 48, 61, 124, 25, 208, 82, 78, 132, 199, 20, 224, 95, 97, 81, 124, 222, 11, 19,
        130, 128, 213, 24, 250, 245, 102, 253, 196, 208, 69, 9, 74, 190, 55, 43, 179, 187, 236,
        212, 117, 63, 118, 219, 140, 65, 186, 159, 192, 21, 85, 139, 242, 58, 128, 144, 143, 153,
        17, 38, 209, 44, 231, 172, 213, 85, 8, 255, 30, 125, 255, 165, 111, 116, 36, 1, 225, 129,
        79, 193, 70, 150, 88, 167, 140, 122, 127, 128, 1, 176, 160, 141, 160, 200, 50, 83, 213,
        192, 203, 135, 114, 134, 192, 98, 218, 47, 83, 10, 228, 36, 254, 37, 69, 55, 121, 65, 253,
        1, 105, 19, 53, 5, 128, 179, 167, 128, 162, 159, 172, 127, 125, 250, 226, 29, 5, 217, 80,
        110, 125, 166, 81, 91, 127, 161, 173, 151, 15, 248, 118, 222, 53, 241, 190, 194, 89, 158,
        192, 2, 128, 91, 103, 114, 220, 106, 78, 118, 4, 200, 208, 101, 36, 121, 249, 91, 52, 54,
        7, 194, 217, 19, 140, 89, 238, 183, 153, 216, 91, 244, 59, 107, 191, 128, 61, 18, 190, 203,
        106, 75, 153, 25, 221, 199, 197, 151, 61, 4, 238, 215, 105, 108, 131, 79, 144, 199, 121,
        252, 31, 207, 115, 80, 204, 194, 141, 107, 128, 95, 51, 235, 207, 25, 31, 221, 207, 59, 63,
        52, 110, 195, 54, 193, 5, 199, 75, 64, 164, 211, 93, 253, 160, 197, 146, 242, 190, 160, 0,
        132, 233, 128, 247, 100, 199, 51, 214, 227, 87, 113, 169, 178, 106, 31, 168, 107, 155, 236,
        89, 116, 43, 4, 111, 105, 139, 230, 193, 64, 175, 16, 115, 137, 125, 61, 128, 205, 59, 200,
        195, 206, 60, 248, 53, 159, 115, 113, 161, 51, 22, 240, 47, 210, 43, 2, 163, 211, 39, 104,
        74, 43, 97, 244, 164, 126, 0, 34, 184, 128, 218, 117, 42, 250, 235, 146, 93, 83, 0, 228,
        91, 133, 16, 82, 197, 248, 169, 197, 170, 232, 132, 241, 93, 100, 118, 78, 223, 150, 27,
        139, 34, 200, 128, 191, 31, 169, 199, 228, 201, 67, 64, 219, 175, 215, 92, 190, 1, 108,
        152, 13, 14, 93, 91, 78, 118, 130, 63, 161, 30, 97, 98, 144, 20, 195, 75, 128, 79, 84, 161,
        94, 93, 81, 208, 43, 132, 232, 202, 233, 76, 152, 51, 174, 129, 229, 107, 143, 11, 104, 77,
        37, 127, 111, 114, 46, 230, 108, 173, 249, 128, 148, 131, 63, 178, 220, 232, 199, 141, 68,
        60, 214, 120, 110, 12, 1, 216, 151, 74, 75, 119, 156, 23, 142, 245, 230, 107, 73, 224, 33,
        221, 127, 26, 225, 2, 159, 12, 93, 121, 93, 2, 151, 190, 86, 2, 122, 75, 36, 100, 227, 51,
        151, 96, 146, 128, 243, 50, 255, 85, 106, 191, 93, 175, 13, 52, 82, 61, 247, 200, 205, 19,
        105, 188, 182, 173, 187, 35, 164, 128, 147, 191, 7, 10, 151, 17, 191, 52, 128, 56, 41, 52,
        19, 74, 169, 25, 181, 156, 22, 255, 141, 232, 217, 122, 127, 220, 194, 68, 142, 163, 39,
        178, 111, 68, 0, 93, 117, 109, 23, 133, 135, 128, 129, 214, 52, 20, 11, 54, 206, 3, 28, 75,
        108, 98, 102, 226, 167, 193, 157, 154, 136, 227, 143, 221, 138, 210, 58, 189, 61, 178, 14,
        113, 79, 105, 128, 253, 225, 112, 65, 242, 47, 9, 96, 157, 121, 219, 227, 141, 204, 206,
        252, 170, 193, 57, 199, 161, 15, 178, 59, 210, 132, 193, 196, 146, 176, 4, 253, 128, 210,
        135, 173, 29, 10, 222, 101, 230, 77, 57, 105, 244, 171, 133, 163, 112, 118, 129, 96, 49,
        67, 140, 234, 11, 248, 195, 59, 123, 43, 198, 195, 48, 141, 8, 159, 3, 230, 211, 193, 251,
        21, 128, 94, 223, 208, 36, 23, 46, 164, 129, 125, 255, 255, 128, 21, 40, 51, 227, 74, 133,
        46, 151, 81, 207, 192, 249, 84, 174, 184, 53, 225, 248, 67, 147, 107, 169, 151, 152, 83,
        164, 14, 67, 153, 55, 37, 95, 128, 106, 54, 224, 173, 35, 251, 50, 36, 255, 246, 230, 219,
        98, 4, 132, 99, 167, 242, 124, 203, 146, 246, 91, 78, 52, 138, 205, 90, 122, 163, 160, 104,
        128, 39, 182, 224, 153, 193, 21, 129, 251, 46, 138, 207, 59, 107, 148, 234, 237, 68, 34,
        119, 185, 167, 76, 231, 249, 34, 246, 227, 191, 41, 89, 134, 123, 128, 253, 12, 194, 200,
        70, 219, 106, 158, 209, 154, 113, 93, 108, 60, 212, 106, 72, 183, 244, 9, 136, 60, 112,
        178, 212, 201, 120, 179, 6, 222, 55, 158, 128, 171, 0, 138, 120, 195, 64, 245, 204, 117,
        217, 156, 219, 144, 89, 81, 147, 102, 134, 68, 92, 131, 71, 25, 190, 33, 247, 98, 11, 149,
        13, 205, 92, 128, 109, 134, 175, 84, 213, 223, 177, 192, 111, 63, 239, 221, 90, 67, 8, 97,
        192, 209, 158, 37, 250, 212, 186, 208, 124, 110, 112, 212, 166, 121, 240, 184, 128, 243,
        94, 220, 84, 0, 182, 102, 31, 177, 230, 251, 167, 197, 153, 200, 186, 137, 20, 88, 209, 68,
        0, 3, 15, 165, 6, 153, 154, 25, 114, 54, 159, 128, 116, 108, 218, 160, 183, 218, 46, 156,
        56, 100, 151, 31, 80, 241, 45, 155, 66, 129, 248, 4, 213, 162, 219, 166, 235, 224, 105, 89,
        178, 169, 251, 71, 128, 46, 207, 222, 17, 69, 100, 35, 200, 127, 237, 128, 104, 244, 20,
        165, 186, 68, 235, 227, 174, 145, 176, 109, 20, 204, 35, 26, 120, 212, 171, 166, 142, 128,
        246, 85, 41, 24, 51, 164, 156, 242, 61, 5, 123, 177, 92, 66, 211, 119, 197, 93, 80, 245,
        136, 83, 41, 6, 11, 10, 170, 178, 34, 131, 203, 177, 128, 140, 149, 251, 43, 98, 186, 243,
        7, 24, 184, 51, 14, 246, 138, 82, 124, 151, 193, 188, 153, 96, 48, 67, 83, 34, 77, 138,
        138, 232, 138, 121, 213, 128, 69, 193, 182, 217, 144, 74, 225, 113, 213, 115, 189, 206,
        186, 160, 81, 66, 216, 22, 72, 189, 190, 177, 108, 238, 221, 197, 74, 14, 209, 93, 62, 43,
        128, 168, 234, 25, 50, 130, 254, 133, 182, 72, 23, 7, 9, 28, 119, 201, 33, 142, 161, 157,
        233, 20, 231, 89, 80, 146, 95, 232, 100, 0, 251, 12, 176, 128, 194, 34, 206, 171, 83, 85,
        234, 164, 29, 168, 7, 20, 111, 46, 45, 247, 255, 100, 140, 62, 139, 187, 109, 142, 226, 50,
        116, 186, 114, 69, 81, 177, 128, 8, 241, 66, 220, 60, 89, 191, 17, 81, 200, 41, 236, 239,
        234, 53, 145, 158, 128, 69, 61, 181, 233, 102, 159, 90, 115, 137, 154, 170, 81, 102, 238,
        128, 79, 29, 33, 251, 220, 1, 128, 196, 222, 136, 107, 244, 15, 145, 223, 194, 32, 43, 62,
        182, 212, 37, 72, 212, 118, 144, 128, 65, 221, 97, 123, 184,
    ][..]];

    for proof in proofs {
        group.throughput(Throughput::Bytes(proof.len() as u64));
        group.bench_with_input(BenchmarkId::new("decode", proof.len()), proof, |b, i| {
            b.iter(|| {
                smoldot::trie::proof_decode::decode_and_verify_proof(
                    smoldot::trie::proof_decode::Config { proof: i },
                )
                .unwrap()
            })
        });
    }

    group.finish()
}

criterion_group!(benches, benchmark_proof_decode);
criterion_main!(benches);

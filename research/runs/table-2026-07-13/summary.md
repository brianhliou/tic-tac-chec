# Dense-table random-update baseline — 2026-07-13

## Provenance

- Source commit: `f208240b37666086d4f0539cc2400c3c0ca0df03`
- Command pattern: `cargo run --manifest-path solver/Cargo.toml --release --bin table_bench -- <MiB-per-table> <iterations>`
- Rust: `rustc 1.92.0 (ded5c06cf 2025-12-08)`
- OS/architecture: macOS 26.5.2, `arm64`
- Host: Apple M3 Max with 48 GiB RAM (operator-provided hardware description)
- Build profile: optimized, LTO enabled

The benchmark allocates two equal byte arrays representing value and remaining-
child tables, pre-faults them before timing, and performs a deterministic random
read-modify-write in both arrays for each update. Sizes below are per array, so
the total working set is twice the listed size.

## Results

| Per array | Total arrays | Updates | Time/update | Throughput |
|---:|---:|---:|---:|---:|
| 1 MiB | 2 MiB | 20,000,000 | 3.78 ns | 264.764 million/s |
| 64 MiB | 128 MiB | 20,000,000 | 12.74 ns | 78.511 million/s |
| 512 MiB | 1 GiB | 10,000,000 | 16.99 ns | 58.862 million/s |
| 2 GiB | 4 GiB | 10,000,000 | 41.49 ns | 24.105 million/s |

The full normalized post-opening domain requires about 2.29 GiB for each byte
array, close to the largest rung. Its single-thread random-update cost should
therefore be of the same order as 41.49 ns, subject to access distribution and
frontier effects.

## Decision

Start the first retrograde implementation with one byte per value and one byte
per remaining-child counter. The current predecessor kernel costs about 433
ns/edge, roughly ten times the 4 GiB paired-update cost, so CPU edge generation
is the clearer single-thread bottleneck. Do not add two-bit packing, atomic
packing, or a reachable-only hash solely for memory bandwidth yet.

The full solver still needs a realistic combined benchmark: generated reverse
edges, random value/counter accesses, and queue insertion in one loop. Parallel
scaling may move the bottleneck toward memory bandwidth, so this decision must
be revisited after single-thread and multi-thread retrograde rungs.

## Limits

The benchmark uses a uniform power-of-two mask, touches both tables on every
operation, and has almost no queue or branch logic. A real solve skips resolved
parents, decrements counters only for winning children, and has nonuniform
frontiers. It does not measure atomics, false sharing, NUMA, queue memory, page
compression, checkpoint I/O, or peak RSS.

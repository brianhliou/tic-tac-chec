# Atomic parallel retrograde rehearsal — 2026-07-13

## Provenance

- Source commit: `da628be2a2b25950fa2f112c538dbc813f6676bf`
- Command pattern: `cargo run --manifest-path solver/Cargo.toml --release --bin parallel_retrograde_rehearsal -- <resolved-children> <threads> 12`
- Rules: original Dream Green rules with travel-direction pawn captures
- Rust: `rustc 1.92.0 (ded5c06cf 2025-12-08)`
- OS/architecture: macOS 26.5.2, `arm64`
- Host: Apple M3 Max with 48 GiB RAM (operator-provided hardware description)
- Build profile: optimized, LTO enabled

Like the single-thread experiment, this is a full-address bounded propagation
rehearsal, not a partial solve. It uses the complete 2,462,360,745-state value
and remaining-counter arrays, real generated predecessor IDs, relaxed atomic
byte updates, and one `u32` output frontier per worker. Input child IDs are
deterministic but independently seeded by worker, so exact edge counts vary
slightly with thread count.

## One-million-child scaling

| Threads | Predecessors | Time/pred | Throughput | Relative to 1 thread |
|---:|---:|---:|---:|---:|
| 1 | 11,663,122 | 562.79 ns | 1.777 million/s | 1.00× |
| 2 | 11,675,182 | 266.17 ns | 3.757 million/s | 2.11× |
| 4 | 11,672,643 | 131.06 ns | 7.630 million/s | 4.29× |
| 8 | 11,663,667 | 64.98 ns | 15.389 million/s | 8.66× |
| 12 | 11,659,693 | 50.86 ns | 19.662 million/s | 11.06× |
| 16 | 11,657,721 | 51.85 ns | 19.287 million/s | 10.85× |

The sub-second 12- and 16-thread results were close enough to require longer
runs.

## Stable scaling rungs

At 5,000,000 resolved children:

| Threads | Predecessors | Time/pred | Throughput | Peak RSS |
|---:|---:|---:|---:|---:|
| 4 | 58,354,792 | 127.23 ns | 7.860 million/s | 4,820.4 MiB |
| 8 | 58,342,767 | 64.52 ns | 15.500 million/s | 4,831.6 MiB |
| 12 | 58,326,821 | 48.59 ns | 20.578 million/s | 4,843.3 MiB |
| 16 | 58,319,934 | 47.26 ns | 21.157 million/s | 4,853.0 MiB |

At 20,000,000 resolved children:

| Threads | Predecessors | Time/pred | Throughput | Peak RSS |
|---:|---:|---:|---:|---:|
| 12 | 233,369,225 | 50.42 ns | 19.835 million/s | 5,166.6 MiB |
| 16 | 233,375,872 | 47.73 ns | 20.952 million/s | 5,173.4 MiB |

The 16-thread long run completed in 11.138518 s. It produced 113,484,953
frontier IDs, performed 113,473,182 counter decrements, skipped 6,417,737
already-resolved visits, and reported checksum `255726403338470669`. No counter
underflows occurred in any recorded run.

## Decision

Use relaxed atomic byte arrays with 16 workers and thread-local frontier output
buffers as the first local production architecture. Sixteen workers were 5.6%
faster than twelve on the longest matched rung. Atomic scaling is sufficiently
close to linear through the performance cores that rank-sharded BSP does not
yet justify its batching, synchronization, and checkpoint complexity.

At 20.95 million predecessors/s, one traversal of the roughly 28.8-billion-edge
dense graph would take about 23 minutes in this synthetic propagation regime.
This is only an order-of-magnitude projection: initialization, real frontier
distribution, terminal density, audit, synchronization between waves, and
checkpointing still need measurement. It nevertheless supports keeping the
first full solve local.

## Concurrency checks and limits

Tests cover the atomic transition rules directly and under deliberate
same-parent contention. Exactly one worker may resolve the final winning-child
decrement as a loss. A loss-child witness resolves the parent as a win even
while other workers decrement winning children. Resolved states are monotonic,
and workers never share frontier vectors.

The rehearsal uses uniform random resolved children, synthetic initial counters,
and alternating child values. Real waves may exhibit much hotter parent IDs and
different win/loss ratios. It does not initialize exact degrees, execute a
closed fixpoint, checkpoint, recover from interruption, compute remoteness, or
audit a game solution. Those remain mandatory before the canonical run.

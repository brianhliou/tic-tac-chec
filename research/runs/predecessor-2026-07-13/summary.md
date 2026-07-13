# Predecessor-edge baseline — 2026-07-13

## Provenance

- Source commit: `80f9608e35655c4bf1267b7cfd8d9489183c7afb`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin predecessor_bench -- 5000000`
- Rules: original Dream Green rules with travel-direction pawn captures
- Rust: `rustc 1.92.0 (ded5c06cf 2025-12-08)`
- OS/architecture: macOS 26.5.2, `arm64`
- Host: Apple M3 Max with 48 GiB RAM (operator-provided hardware description)
- Build profile: optimized, LTO enabled

The benchmark cycles through 65,536 deterministic pseudo-random IDs from the
full normalized post-opening structural domain. Each call includes unranking
and undoing player normalization, inverse-geometry candidate construction,
candidate ranking, production move-legality validation, exact forward replay,
and streaming every normalized parent ID to a checksum callback. It does not
access a value table.

## Results

| Metric | Result |
|---|---:|
| States sampled | 5,000,000 |
| Predecessor edges | 58,399,000 |
| Mean predecessors/state | 11.680 |
| Zero-predecessor states | 19,468 |
| Wall time | 25.265347 s |
| Time/state | 5,053.07 ns |
| Time/edge | 432.63 ns |
| Throughput | 2.311 million edges/s |
| Checksum | `65418993353002827` |

A preceding 1,000,000-state run measured 425.01 ns/edge and 2.353 million
edges/s. The sampled mean indegree of 11.680 also closely matches the 11.683
mean outdegree in the independently sampled successor benchmark, as required
in aggregate for a finite directed graph over the same dense domain.

## Optimization comparison

The first validated implementation at source commit
`b1bcdebab192ac02c43e71dae8e51d4a04e87b5b` tried every empty square as a
possible reverse origin. Replacing that with piece-specific inverse geometry
kept forward validation unchanged.

On the same 10,000-state deterministic sample:

| Implementation | Time/edge | Throughput | Edge checksum |
|---|---:|---:|---:|
| Every empty origin | 1,690.68 ns | 0.591 million/s | `130487871056717` |
| Inverse geometry | 701.34 ns | 1.426 million/s | `130487871056717` |

The identical 116,570 edge count and checksum make this a 2.4× candidate-
generation improvement with no observed semantic change.

At commit `80f9608e35655c4bf1267b7cfd8d9489183c7afb`, forward validation was changed
to stop immediately after finding its reconstructed action. On the stable
5,000,000-state benchmark this reduced cost from 522.14 to 432.63 ns/edge,
another 17.1%, with the same edge count and checksum.

## Correctness checks

Every reverse candidate must pass all of these checks before emission:

1. the reconstructed parent has a valid collision-free dense rank;
2. the independent production forward generator emits the proposed action;
3. applying the action recreates the exact absolute child position, including
   pawn directions, capture-to-hand resets, opening history, and side to move.

Tests recover one sampled forward edge from each of 5,000 random parents under
all three returning-pawn capture variants. Separate tests sample 2,000 random
children under all variants, reject duplicate parents, and replay every emitted
parent through the vector-based checked reference engine. The full test suite
contains 25 tests across the library and binaries.

## Decision

Keep the inverse-geometry, forward-validated predecessor generator as the
correctness baseline. Reverse edges remain about 2.2× more expensive than
successor edges, but are fast enough to move the next experiment to dense-table
random updates and a small end-to-end retrograde rung. Optimize validation or
ranking only if those fuller profiles still identify reverse CPU work as the
bottleneck.

## Limits

The mean indegree/outdegree comparison uses deterministic uniform samples, not
an exhaustive graph scan. The tests strongly exercise soundness and sampled
completeness but are not yet the final full-domain edge audit. This benchmark
does not measure reachability, multi-gigabyte table access, frontier storage,
parallel synchronization, reflection canonicalization, retrograde fixpoint
logic, or peak RSS.

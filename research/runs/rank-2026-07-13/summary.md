# Rank/unrank baseline — 2026-07-13

## Provenance

- Source commit: `eaa52102eccf8ad76cc61a68819693e18f2b40c4`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin rank_bench -- 10000000`
- Rust: `rustc 1.92.0 (ded5c06cf 2025-12-08)`
- OS/architecture: macOS 26.5.2, `arm64`
- Host: Apple M3 Max with 48 GiB RAM (operator-provided hardware description)
- Build profile: optimized, LTO enabled

The benchmark cycles through 65,536 deterministic pseudo-random structural
positions. Each timed section performs 10,000,000 operations. `black_box` and
reported checksums prevent dead-code elimination.

## Results

| Kernel | Time | Throughput |
|---|---:|---:|
| unrank | 133.34 ns | 7.500 million/s |
| rank, already normalized | 165.50 ns | 6.042 million/s |
| rank, Black to move | 175.85 ns | 5.687 million/s |
| unrank + normalized rank | 300.86 ns | 3.324 million/s |

```text
rank_normalized_checksum = 12368277147786847
rank_black_to_move_checksum = 12368277147786847
round_trip_checksum = 12315048801954102
```

The equal rank checksums confirm that color-swap plus 180-degree rotation maps
the sampled Black-to-move positions to the same IDs as their normalized forms.
Normalization adds about 10 ns per rank in this benchmark.

## Decision

Full combinatorial reranking is fast enough for the first graph kernel. Do not
implement incremental child ranks yet. First measure complete successor and
predecessor edges, including move generation, forward validation, canonical
rank, and random table access. Revisit incremental ranking if profiling shows
rank consumes a material fraction of edge time.

## Limits

This is a CPU microbenchmark, not a solve-runtime projection. It does not
measure move generation, predecessor generation, random multi-gigabyte array
updates, contention, left-right reflection, frontier representation, or peak
solver memory.

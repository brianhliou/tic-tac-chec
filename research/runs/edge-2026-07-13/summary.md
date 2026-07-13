# Successor-edge baseline — 2026-07-13

## Provenance

- Source commit: `cdd31d5c31c0e74797ce15a6363341cc3ab53707`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin edge_bench -- 10000000`
- Rules: original Dream Green rules with travel-direction pawn captures
- Rust: `rustc 1.92.0 (ded5c06cf 2025-12-08)`
- OS/architecture: macOS 26.5.2, `arm64`
- Host: Apple M3 Max with 48 GiB RAM (operator-provided hardware description)
- Build profile: optimized, LTO enabled

The benchmark cycles through 65,536 deterministic pseudo-random IDs from the
full normalized post-opening structural domain. Each successor call includes
unranking the parent, allocation-free move generation, applying each action,
player-to-move normalization, and ranking every child. The callback only adds
child IDs to a checksum; it does not access a value table.

## Results

| Metric | Result |
|---|---:|
| States sampled | 10,000,000 |
| Successor edges | 116,832,667 |
| Mean edges/state | 11.683 |
| Zero-successor states | 73,559 |
| Wall time | 22.648065 s |
| Time/state | 2,264.81 ns |
| Time/edge | 193.85 ns |
| Throughput | 5.159 million edges/s |
| Checksum | `138465096208305687` |

A preceding 1,000,000-state run measured 193.98 ns/edge, within 0.1% of the
long run.

## Correctness checks

The production generator is structurally independent from the existing
vector-based reference move generator. Tests compare exact action sets on:

- 20,000 deterministic random post-opening IDs under each of the three named
  returning-pawn capture variants;
- every generated locked-opening position through ply two; and
- 20,000 additional random post-opening IDs whose production successor sets
  are compared with checked reference play followed by canonical ranking.

These tests also reject duplicate emitted actions or child IDs in the sampled
positions. The normal test suite contains 23 tests across the library and
binaries.

## Decision

Keep full child reranking as the baseline. At roughly 194 ns per generated
edge, successor generation is not yet the architecture bottleneck. Implement
candidate predecessor generation with mandatory forward validation next and
benchmark that complete reverse-edge path before considering incremental
ranks, reflection canonicalization, or lower-level bitboards.

## Limits

The sample covers the dense structural domain, not only reachable legal
states, so its 11.683 mean branching factor is not a reachable-game census.
Zero-successor states include terminal positions and may include structurally
valid but unreachable no-move positions. The benchmark does not measure
predecessor generation, reachability, random multi-gigabyte table updates,
frontier storage, synchronization, left-right reflection, or solver RSS.

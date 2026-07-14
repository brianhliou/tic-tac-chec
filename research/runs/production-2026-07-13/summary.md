# Canonical post-opening solve — 2026-07-13

Rules: original Dream Green edition with travel-direction pawn captures.

This record will be extended as propagation and audit phases complete. The
checkpoint itself is intentionally ignored by Git.

## Initialization

- Source commit: `b3994ef6bf64cbbbbe5e889273a9578f87a97b8e`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- init research/runs/production-2026-07-13/post-opening-travel.ctb 16`
- Rules tag: `0x54544303`
- Threads: 16
- Dense states: **2,462,360,745**
- Directed successor edges: **28,730,418,180**
- Mean outdegree: **11.667834714**
- Terminal-loss seeds: **18,516,912**
- Nonterminal dead ends: **0**
- Maximum outdegree: **64**
- Initialization time: **466.875508 seconds**
- Throughput: **5.274 million states/s**, **61.538 million edges/s**
- Checkpoint write time: **14.648613 seconds**

The zero dead-end count confirms exhaustively that every nonterminal structural
post-opening state has at least one legal action. The maximum degree proves the
`u8` remaining-child counter bound over the complete dense domain.

## Wave-zero checkpoint

- File: `post-opening-travel.ctb`
- Size: **4,998,789,198 bytes**
- Wave: 0
- Frontier IDs: **18,516,912**
- SHA-256: `c4dac2ebc3df24bc7e7c9f33b23f32b9c1f8cc154e48db3929d85fb1244ec56e`
- Full reload and CRC verification: **13.959299 seconds**

The checkpoint contains one value byte and one exact remaining-child byte for
every normalized state plus the sorted terminal frontier. Loading validates
the version, rules tag, dimensions, frontier bounds, and streaming CRC-64.

## Status

Initialization is complete and independently reloadable. No propagation has
yet been applied in this checkpoint, and no W/L/D result is claimed here.

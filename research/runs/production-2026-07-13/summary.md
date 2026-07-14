# Canonical post-opening solve — 2026-07-13

Rules: original Dream Green edition with travel-direction pawn captures.

The checkpoint itself is intentionally ignored by Git.

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

## Propagation

- Source commit: `c3185f83378fbbd1c6492533b1920946b725fc18` plus the checkpoint-compatible solver from
  `b3994ef6bf64cbbbbe5e889273a9578f87a97b8e`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- propagate research/runs/production-2026-07-13/post-opening-travel.ctb 16 5`
- Fixpoint wave: **42**
- Resolved wins: **184,895,598** (7.508875%)
- Resolved losses: **24,178,920** (0.981941%)
- Unresolved draws: **2,253,286,227** (91.509184%)
- Resolved W/L states: **209,074,518**
- Predecessor edges processed: **2,192,996,014**
- Wave-computation time: **83.116374 seconds**
- Checkpoint-write time during propagation: **118.529765 seconds**

The conservative five-wave checkpoint interval made I/O more expensive than
propagation after the early large waves. Future runs should use a wider interval
once restart behavior has already been demonstrated.

## Final checkpoint

- Wave: 42
- Frontier IDs: 0
- Size: **4,924,721,550 bytes**
- SHA-256: `16fa3785ffa78d71205361ab467b033f5fbdb78687b04d9483f218756646ab18`
- Full reload and CRC verification: **13.275828 seconds**

## Independent minimax audit

- Audit source commit: `59229d23476579ea397e9b1bd16a90813ac93b85`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- audit research/runs/production-2026-07-13/post-opening-travel.ctb 16`
- States audited: **2,462,360,745**
- Successor edges regenerated: **28,730,418,180**
- Wins reproduced: **184,895,598**
- Losses reproduced: **24,178,920**
- Draws reproduced: **2,253,286,227**
- Audit time: **493.825230 seconds**
- Audit throughput: **58.179 million edges/s**

The audit is a pull computation over regenerated successors. It does not use
the solver's generated predecessors or remaining-child counters. Every dense
state satisfied the direct terminal/win/loss/draw minimax equation.

## Status

The normalized post-opening W/L/D table is complete, checksummed, reloadable,
and fully audited. This is not yet the value of the initial empty board: the
six-ply forced-placement opening must still be evaluated backward against this
table.

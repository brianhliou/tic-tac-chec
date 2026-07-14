# Outbound-only pawn sensitivity solve — 2026-07-13

Rules: original Dream Green edition with returning pawns unable to capture.
This is a sensitivity variant, not the canonical adjudication.

The large checkpoint and tablebase files are intentionally ignored by Git.

## Result

The outbound-only interpretation is also a **draw under perfect play**. The
initial value is unchanged, but the complete table differs materially from the
canonical travel-direction solve.

## Initialization

- Source commit: `35dad84b5fc57c422b960f9e30157b68b3080b54`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- init research/runs/outbound-only-2026-07-13/post-opening-outbound.ctb 16 --pawn=outbound`
- Rules tag: `0x54544301`
- Threads: 16
- Dense states: **2,462,360,745**
- Directed successor edges: **28,304,244,300**
- Terminal-loss seeds: **18,516,912**
- Nonterminal dead ends: **0**
- Maximum outdegree: **64**
- Initialization time: **593.496074 seconds**
- Throughput: **4.149 million states/s**, **47.691 million edges/s**
- Wave-zero checkpoint write: **15.753537 seconds**
- Wave-zero checkpoint SHA-256: `faefcea70b061e3abca03b9e6938944ad19c261270de2be515cff00f4e116161`
- Wave-zero reload and CRC verification: **14.889132 seconds**

Outbound-only removes exactly **426,173,880** directed moves from the canonical
post-opening graph, a **1.483354%** reduction, while preserving the terminal
seed count, dead-end count, and maximum-degree bound.

## Retrograde propagation

- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- propagate research/runs/outbound-only-2026-07-13/post-opening-outbound.ctb 16 20 --pawn=outbound`
- Fixpoint wave: **42**
- Wins: **191,895,754** (7.793162%)
- Losses: **25,329,088** (1.028651%)
- Draws: **2,245,135,903** (91.178188%)
- Wave computation: **92.676570 seconds**
- Checkpoint writes at waves 20, 40, and 42: **40.860005 seconds**
- Final checkpoint size: **4,924,721,550 bytes**
- Final checkpoint SHA-256: `d674d08a02a7560cc9c2bf414db7dade848e211c0cb7e72d35b763d581d99f67`

The 20-wave checkpoint interval reduced I/O overhead while retaining a
restartable midpoint. The canonical five-wave run spent more time writing
checkpoints than propagating values.

## Independent W/L/D audit

- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- audit research/runs/outbound-only-2026-07-13/post-opening-outbound.ctb 16 --pawn=outbound`
- States audited: **2,462,360,745**
- Successor edges regenerated: **28,304,244,300**
- Wins reproduced: **191,895,754**
- Losses reproduced: **25,329,088**
- Draws reproduced: **2,245,135,903**
- Audit time: **541.056688 seconds**
- Throughput: **52.313 million edges/s**

The pull audit regenerated forward successors and checked the direct minimax
equation at every dense position. It did not use propagation's generated
predecessors or remaining-child counters.

## Forced-placement opening

- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- opening research/runs/outbound-only-2026-07-13/post-opening-outbound.ctb 16 --pawn=outbound`
- Backward evaluation: **3.758558 seconds**
- Independent checked-play audit: **11.437732 seconds**
- Initial empty-board value: **draw**

| Ply | States | Edges | Wins | Losses | Draws |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | 64 | 0 | 0 | 1 |
| 1 | 64 | 3,840 | 0 | 0 | 64 |
| 2 | 3,840 | 161,280 | 0 | 0 | 3,840 |
| 3 | 80,640 | 3,144,960 | 0 | 0 | 80,640 |
| 4 | 1,572,480 | 37,739,520 | 65,420 | 0 | 1,507,060 |
| 5 | 12,579,840 | 276,756,480 | 95,844 | 33,472 | 12,450,524 |

All 64 first moves remain drawing. Decisive opening positions first appear at
ply four under both variants, but the later-layer counts differ.

## Remoteness and final artifact

- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- enrich research/runs/outbound-only-2026-07-13/post-opening-outbound.ctb research/runs/outbound-only-2026-07-13/post-opening-outbound.tb 16 --pawn=outbound`
- Checkpoint load: **15.105781 seconds**
- Nonterminal-loss edges initialized: **90,414,846**
- Remoteness propagation: **94.210412 seconds**
- Maximum post-opening distance: **41 plies**
- Decisive positions audited: **217,224,842**
- Decisive edges audited: **2,263,959,402**
- Independent distance-equation audit: **50.317075 seconds**
- Opening W/L/D plus remoteness: **7.713945 seconds**
- Independent opening-distance audit: **10.504496 seconds**
- Maximum locked-opening distance: **37 plies**
- Tablebase write: **7.158408 seconds**
- Tablebase size: **2,476,597,658 bytes**
- Tablebase CRC-64/XZ: `0x9727127fadf34fca`
- Tablebase SHA-256: `a75d985022831c45811a49f2bc5a63c8af095c51da19aa212551d4d7a7545752`
- Full reload, code validation, census, and CRC verification: **9.506475 seconds**

The final artifact contains 191,895,754 post-opening wins, 25,329,088 losses,
and 2,245,135,903 draws. Its opening section contains 161,264 wins, 33,472
losses, and 14,042,129 draws.

## Exact comparison with the canonical table

- Comparator source commit: `5653f278d87e7e4f4b19c9a99524985792c9f752`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin variant_compare -- research/runs/production-2026-07-13/post-opening-travel.tb research/runs/outbound-only-2026-07-13/post-opening-outbound.tb research/runs/outbound-only-2026-07-13/variant-comparison.md 5653f278d87e7e4f4b19c9a99524985792c9f752`
- Post-opening W/L/D changes: **16,529,908** (0.671303%)
- Post-opening exact result-or-distance changes: **17,701,586** (0.718887%)
- Locked-opening W/L/D changes: **16,912** (0.118790%)
- Locked-opening exact result-or-distance changes: **19,524** (0.137137%)
- Comparison report SHA-256: `8c7174205f10b7698095a27f3e4d346e109d8eb9998b45bb231571ef2cc71751`

The comparison includes genuine reversals: 9,366 canonical post-opening wins
become outbound-only losses, while 810 canonical losses become outbound-only
wins. Another 11,146,988 canonical draws become wins and 1,188,040 become
losses; movement restrictions can help or hurt the player to move depending on
the position. See [`variant-comparison.md`](variant-comparison.md) for the full
transition matrices and representative dense IDs.

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

## Forced-placement opening

- Source commit: `aafdef480fbfe2f23cd6a24d1babbc432352313d`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- opening research/runs/production-2026-07-13/post-opening-travel.ctb 16`
- Locked-opening states: **14,236,865**
- Placement edges: **317,806,144**
- Mean outdegree: **22.322761647**
- Backward-evaluation time: **3.114531 seconds**
- Independent reference audit time: **8.571737 seconds**
- Initial empty-board value: **draw**

| Ply | States | Edges | Wins | Losses | Draws |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | 64 | 0 | 0 | 1 |
| 1 | 64 | 3,840 | 0 | 0 | 64 |
| 2 | 3,840 | 161,280 | 0 | 0 | 3,840 |
| 3 | 80,640 | 3,144,960 | 0 | 0 | 80,640 |
| 4 | 1,572,480 | 37,739,520 | 60,404 | 0 | 1,512,076 |
| 5 | 12,579,840 | 276,756,480 | 87,068 | 30,468 | 12,462,304 |

The solve evaluated the layers from ply 5 back to ply 0 using the
allocation-free production opening generator. The audit then regenerated all
opening actions with the separately implemented vector-based rules engine,
applied every action through checked play, and verified the direct minimax
equation for every opening state. Its layer statistics matched the production
pass exactly.

## W/L/D milestone

The canonical original-edition game is a draw under perfect play. The result
is a strong W/L/D solution: the normalized post-opening table is complete,
checksummed, reloadable, and fully audited, and the complete forced-placement
opening has independently replayed values back to the initial empty board.
The following enrichment phase adds decisive remoteness; a compact
human-readable drawing strategy remains future work.

## Remoteness-enriched tablebase

- Enrichment source commit: `3fa92443b5c625dca0f0406a941cdcef37cd376d`
- Verification source commit: `b6f162b4e3f5ee5f67ac61af7bd3a31e6ebf79cb`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- enrich research/runs/production-2026-07-13/post-opening-travel.ctb research/runs/production-2026-07-13/post-opening-travel.tb 16`
- Threads: 16
- Checkpoint load: **14.505301 seconds**
- Nonterminal-loss edges initialized: **76,499,658**
- Remoteness propagation: **84.448156 seconds**
- Maximum post-opening remoteness: **41 plies**
- Decisive states audited: **209,074,518**
- Decisive successor edges audited: **2,192,239,958**
- Independent distance-equation audit: **40.134301 seconds**
- Opening W/L/D plus remoteness pass: **6.402754 seconds**
- Independent checked-play opening audit: **8.596755 seconds**

The distance convention is terminal loss at zero, win at one plus the minimum
distance of a losing child, and nonterminal loss at one plus the maximum
distance of a winning child. Draws have no finite remoteness. The forward audit
regenerated decisive successors and checked this equation at every decisive
post-opening state; it does not use the generated predecessors or propagation
counters.

Opening maximum remoteness is **39 plies** overall: ply 5 reaches 39, ply 4
reaches 25, and plies 0–3 contain only draws. Across all locked-opening layers,
the artifact contains 147,472 wins, 30,468 losses, and 14,058,925 draws.

## Result-plus-distance artifact

- File: `post-opening-travel.tb` (intentionally ignored by Git)
- Format: `TTCTB001`, version 1
- Rules tag: `0x54544303`
- Post-opening bytes: **2,462,360,745**
- Opening bytes: **14,236,865**
- Total file size: **2,476,597,658 bytes**
- CRC-64/XZ: `0xeb952765179a695e`
- SHA-256: `f6644e7d35cd9653e1c4bb33b2e4221afd27567385c0ec1f7b71c84e65c8f045`
- Atomic artifact write: **7.165775 seconds**
- Full reload, CRC verification, code validation, and census: **9.340936 seconds**
- Verification command: `cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- verify-tablebase research/runs/production-2026-07-13/post-opening-travel.tb`

Each position uses one byte. Codes 0–253 are finite remoteness in plies, with
even parity denoting a loss and odd parity a win; 254 is reserved and rejected;
255 denotes a draw. Moves and edges are generated on demand and are not stored
in the artifact.

## Updated status

The canonical W/L/D result and decisive remoteness are now complete and
audited, and a checksummed tablebase artifact exists.

## Initial tablebase probe

- Source commit: `8051e54cbc62551f0995361c340231c6d2af7498`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin tablebase_probe -- research/runs/production-2026-07-13/post-opening-travel.tb opening 0`
- Initial value reproduced: **draw**
- Legal moves generated: **64**
- Result-preserving optimal moves: **64**

The reusable probe ranks an engine position, reads its code, generates every
legal move, ranks each child, and reports the outcome from the moving player's
perspective. It separately marks moves that preserve W/L/D and moves satisfying
the remoteness policy: shortest forced win, any drawing continuation, or
longest resistance in a forced loss. Tests cover all three policies.

The remaining product work is human-facing position input/editing, a compact
drawing-strategy presentation, a hosted explorer, and human-readable strategic
analysis. The current CLI accepts dense opening or post-opening IDs and serves
as the backend behavior oracle for those interfaces.

## Deterministic drawing witness

- Extractor source commit: `80e2425cf967bfd288bf227504875f5f38122eae`
- Command: `cargo run --manifest-path solver/Cargo.toml --release --bin drawing_witness -- research/runs/production-2026-07-13/post-opening-travel.tb research/runs/production-2026-07-13/drawing-witness.json 1000000`
- Policy: `least-drawing-action-v1`
- Prefix: **32 plies**
- Exact repeated-position cycle: **18 plies**
- Total recorded line: **50 plies**
- Repeated normalized key: `post:2148899478`
- Replay audit: **passed**
- JSON size: **9,590 bytes**
- JSON SHA-256: `c3d6dc94f2a3720068928ea1712d204f7a68e6891a5baeb297fe2a8799ad5d18`

The extractor starts again from the empty board after constructing the line
and independently checks every stored position key, absolute side to move,
legal-move index, chosen action, draw value, child key, alternative count, and
policy decision. Its final full engine position exactly equals the position at
ply 32, including side to move and pawn direction state; equality of normalized
tablebase IDs alone is not treated as a repetition.

The repeating cycle is:

```text
a1-b3 N@a1 b2-a1 N@c1 a1-b2 b1-a2 b2-a1 c1-b3 N@b1
a2-b1 N@c1 b1-a2 a1-b2 a2-b1 c1-a2 b1-a2 N@a1 a2-b1
```

Sixteen positions on the 50-ply lasso offer at least one mover-relative losing
alternative, with 60 such alternatives in total and as many as 15 at one
position. The selected action remains drawing at every step.

This lasso is a compact perfect-play illustration, not a standalone proof
against all deviations. The independently audited complete tablebase remains
the strong-solution proof; it supplies the full drawing policy by selecting a
drawing child at every drawn position. See
[`research/drawing-witness.md`](../../drawing-witness.md) for the precise
semantics and artifact contract.

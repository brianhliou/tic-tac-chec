# tic-tac-chec

A from-scratch effort to solve **Tic Tac Chec**, Don Green's 4×4 alignment
game with chess movement and reusable captured pieces.

The canonical original-edition game is **a draw under perfect play**. The full
normalized post-opening W/L/D table and all six placement-only opening plies
were solved and independently audited on 2026-07-13. Exact counts, timings,
checkpoint checksum, and reproduction commands are in
[`research/runs/production-2026-07-13/summary.md`](research/runs/production-2026-07-13/summary.md).
Decisive positions now also carry audited optimal remoteness, with a maximum of
41 plies, in a checksummed one-byte-per-state artifact.

## Solve target

The canonical target is the original Dream Green ruleset:

- one pawn, knight, bishop, and rook per player;
- start from an empty 4×4 board, White first;
- each player's first three turns place pieces; movement begins after the six
  opening placements;
- afterward, a turn either places an in-hand piece or moves an on-board piece;
- captures return the captured piece to its owner for later placement;
- pawns reverse direction at either end of the board;
- the first player to align all four pieces in a rank, file, or long diagonal
  wins.

The printed rules leave pawn captures while returning ambiguous. The current
canonical interpretation makes captures follow the pawn's current travel
direction; the engine preserves the no-return-capture reading as a sensitivity
variant. See [`research/rules.md`](research/rules.md).

The 2025 Bobby Fischer reissue changes pawn movement and the capture-opening
threshold. It is tracked as a separate variant, not mixed into the initial
solve.

## Solved state space

There are exactly **1,174,226,049** board/hand arrangements before pawn
direction, side to move, and opening history. Under the original bouncing-pawn
rules, the dense all-legal domain is **4,938,958,355** states before symmetry
folding. This corrects the earlier rough estimate of 9.4 billion; the exact
derivation is in [`research/state-space.md`](research/state-space.md).

Player-to-move normalization reduces the post-opening domain to exactly
**2,462,360,745** dense states. The audited table contains 184,895,598 wins,
24,178,920 losses, and 2,253,286,227 draws. The placement-only opening adds
14,236,865 states and evaluates backward to a draw at the empty board.

The solve ran locally with byte-per-state tables and generated predecessors;
the production summary records the exact resource and throughput measurements.
The underlying design is in
[`research/solver-architecture.md`](research/solver-architecture.md).
The probe-to-web boundary is specified in
[`research/hosted-explorer.md`](research/hosted-explorer.md).

A deterministic draw-preserving policy reaches an exact repeated position
after a 32-ply prefix and then follows an 18-ply cycle. The replay-audited,
checksummed witness is committed as
[`drawing-witness.json`](research/runs/production-2026-07-13/drawing-witness.json),
with its scope and proof limitations documented in
[`research/drawing-witness.md`](research/drawing-witness.md).

## Reproduce

```sh
cargo test --manifest-path solver/Cargo.toml
cargo run --manifest-path solver/Cargo.toml --bin state-space
cargo run --manifest-path solver/Cargo.toml --release --bin rank_bench -- 10000000
cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- verify research/runs/production-2026-07-13/post-opening-travel.ctb
cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- opening research/runs/production-2026-07-13/post-opening-travel.ctb 16
cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- verify-tablebase research/runs/production-2026-07-13/post-opening-travel.tb
cargo run --manifest-path solver/Cargo.toml --release --bin tablebase_probe -- research/runs/production-2026-07-13/post-opening-travel.tb opening 0
cargo run --manifest-path solver/Cargo.toml --release --bin tablebase_server -- research/runs/production-2026-07-13/post-opening-travel.tb 4173
```

## Roadmap

1. Host the completed visual explorer and production tablebase.
2. Extract representative decisive lines and strategic findings.
3. Package the methodology, audits, witness, and artifacts for publication.
4. Run the alternate returning-pawn interpretation as a sensitivity solve.

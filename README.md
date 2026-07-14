# tic-tac-chec

A from-scratch effort to solve **Tic Tac Chec**, Don Green's 4×4 alignment
game with chess movement and reusable captured pieces.

The canonical original-edition game is **a draw under perfect play**. The full
normalized post-opening W/L/D table and all six placement-only opening plies
were solved and checked exhaustively through separately implemented audit paths
on 2026-07-13. Exact counts, timings,
checkpoint checksum, and reproduction commands are in
[`research/runs/production-2026-07-13/summary.md`](research/runs/production-2026-07-13/summary.md).
Decisive positions also carry audited optimal remoteness, with a maximum of 41
plies. The solve artifact uses one byte per state; a draw-aware publication
artifact preserves identical probes in **485,862,535 bytes (463 MiB)**.

**[Explore the solved game](https://tic-tac-chec-production.up.railway.app/)**
or read the hosted [solve write-up](https://tic-tac-chec-production.up.railway.app/write-up/).

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
direction. The no-return-capture reading has now also been strongly solved as a
sensitivity variant: its empty board is still a draw, although 0.671303% of
post-opening positions change W/L/D value. See
[`research/rules.md`](research/rules.md) and the
[`variant comparison`](research/runs/outbound-only-2026-07-13/variant-comparison.md).

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
The server also embeds a publication-ready
[`solve write-up`](solver/web/write-up.html), linked from the explorer at
`/write-up/`.

A deterministic draw-preserving policy reaches an exact repeated position
after a 32-ply prefix and then follows an 18-ply cycle. The replay-audited,
checksummed witness is committed as
[`drawing-witness.json`](research/runs/production-2026-07-13/drawing-witness.json),
with its scope and proof limitations documented in
[`research/drawing-witness.md`](research/drawing-witness.md).

The exact decisive-remoteness distribution, replay-audited longest win/loss
examples, and critical choices on that drawing lasso are collected in the
generated
[`strategic report`](research/runs/production-2026-07-13/strategic-report.md).

## Artifacts

Large generated artifacts are intentionally excluded from Git. The audited
source table `post-opening-travel.tb` is 2,476,597,658 bytes and has SHA-256
`f6644e7d35cd9653e1c4bb33b2e4221afd27567385c0ec1f7b71c84e65c8f045`.

The explorer uses a lossless draw-aware derivative. A decisive-position bitmap
omits distance storage for draws; six-bit distances and a rank directory retain
constant-time lookup. Its exhaustive comparison covered all 2,476,597,610
source entries. The resulting `post-opening-travel.ttb` is 485,862,535 bytes,
with internal CRC-64/XZ `0xbe44f17a62ec33e1` and SHA-256
`f80c1899e57941a2251ffa554645ad06e66d4e5fbd349b6d2b949efd2c526c53`.

The compact artifact and checksum are available from the
[`tablebase-v1`](https://github.com/brianhliou/tic-tac-chec/releases/tag/tablebase-v1)
release; they are not committed to the repository.

## Reproduce

```sh
cargo test --manifest-path solver/Cargo.toml
cargo run --manifest-path solver/Cargo.toml --bin state-space
cargo run --manifest-path solver/Cargo.toml --release --bin rank_bench -- 10000000
cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- verify research/runs/production-2026-07-13/post-opening-travel.ctb
cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- opening research/runs/production-2026-07-13/post-opening-travel.ctb 16
cargo run --manifest-path solver/Cargo.toml --release --bin post_opening_solver -- verify-tablebase research/runs/production-2026-07-13/post-opening-travel.tb
cargo run --manifest-path solver/Cargo.toml --release --bin compact_tablebase -- pack research/runs/production-2026-07-13/post-opening-travel.tb research/runs/production-2026-07-13/post-opening-travel.ttb
cargo run --manifest-path solver/Cargo.toml --release --bin compact_tablebase -- verify research/runs/production-2026-07-13/post-opening-travel.ttb
cargo run --manifest-path solver/Cargo.toml --release --bin tablebase_probe -- research/runs/production-2026-07-13/post-opening-travel.tb opening 0
cargo run --manifest-path solver/Cargo.toml --release --bin strategic_report -- research/runs/production-2026-07-13/post-opening-travel.tb research/runs/production-2026-07-13/strategic-report.md <source-commit>
cargo run --manifest-path solver/Cargo.toml --release --bin tablebase_server -- research/runs/production-2026-07-13/post-opening-travel.ttb 4173
```

The production container downloads that immutable release asset during its
multi-stage build and verifies the SHA-256 before creating the runtime image:

```sh
docker build -t tic-tac-chec .
docker run --rm -p 4173:8080 tic-tac-chec
```

## License

The original solver and explorer source is available under the [MIT
License](LICENSE). The bundled Cburnett chess artwork is separately available
under GPL-3.0-or-later; see the [piece credits](solver/web/pieces/CREDITS.md) and
[license text](solver/web/pieces/LICENSE).

## Roadmap

1. Attach `tic-tac-chec.brianhliou.com` after its DNS record is configured.
2. Package the methodology, audits, witness, and strategic report for publication.
3. Obtain a designer/publisher pawn ruling and transcribe the complete 2025 rules.

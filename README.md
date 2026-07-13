# tic-tac-chec

A from-scratch effort to solve **Tic Tac Chec**, Don Green's 4×4 alignment
game with chess movement and reusable captured pieces.

The project is at the rules-engine stage. No game-theoretic result is claimed
yet.

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

## Current state-space bound

There are exactly **1,174,226,049** board/hand arrangements before pawn
direction, side to move, and opening history. Under the original bouncing-pawn
rules, the dense all-legal domain is **4,938,958,355** states before symmetry
folding. This corrects the earlier rough estimate of 9.4 billion; the exact
derivation is in [`research/state-space.md`](research/state-space.md).

This is plausibly an in-memory strong solve on a 48–128 GB machine if we use a
dense rank, generate predecessors rather than store all reverse edges, and
control the retrograde frontier. Reachable-state enumeration is the next
measurement; edge traffic, not the 2-bit value array, is likely to set the
runtime. The researched implementation strategy and benchmark gates are in
[`research/solver-architecture.md`](research/solver-architecture.md).

## Reproduce

```sh
cargo test --manifest-path solver/Cargo.toml
cargo run --manifest-path solver/Cargo.toml --bin state-space
```

## Roadmap

1. Freeze and test the rules variants.
2. Implement exact rank/unrank and enumerate canonical reachable states.
3. Validate move generation against an independent oracle.
4. Prove the retrograde implementation on reduced closed games.
5. Run and audit the full loopy solve; publish the result and tablebase probe.

# State-space derivation

Last reviewed: 2026-07-13.

Run:

```sh
cargo run --manifest-path solver/Cargo.toml --bin state-space
```

## Board and hand arrangements

Each of eight distinct pieces is either in hand or injected into a distinct one
of 16 squares. If `k` pieces are on the board, the count is

```text
sum(k=0..8) C(8,k) * P(16,k) = 1,174,226,049.
```

This part of the earlier estimate was correct.

## Pawn directions

Multiplying every arrangement by four overcounts. An off-board pawn has only
its reset direction, and an on-board pawn on either end rank must point inward
in a normalized post-move state.

For one pawn, the weighted on-board square count is:

```text
8 end-rank squares * 1 direction + 8 interior squares * 2 directions = 24.
```

For both distinct pawns on distinct squares, the weighted injection count is:

```text
24^2 - (8 * 1^2 + 8 * 2^2) = 536.
```

Choose `a` of the six non-pawns. The direction-aware arrangement count is

```text
sum(a=0..6) C(6,a) *
  [P(16,a) + 2*24*P(15,a) + 536*P(14,a)]
= 2,462,360,745.
```

With side to move, the post-opening dense domain is **4,924,721,490** states.

## Opening history

Whether movement has unlocked is history-dependent after captures, but it does
not double the full domain. The locked phase occurs only during plies 0 through
5. Its exact per-ply arrangement counts are:

| Ply | White on board | Black on board | States |
|---:|---:|---:|---:|
| 0 | 0 | 0 | 1 |
| 1 | 1 | 0 | 64 |
| 2 | 1 | 1 | 3,840 |
| 3 | 2 | 1 | 80,640 |
| 4 | 2 | 2 | 1,572,480 |
| 5 | 3 | 2 | 12,579,840 |

The side to move is fixed by ply. Adding these **14,236,865** locked states to
the post-opening domain gives a conservative dense all-legal total of
**4,938,958,355** states before symmetry or terminal/reachability pruning.

## Memory implications

For 4,938,958,355 states:

| Field | Encoding | Memory |
|---|---:|---:|
| W/L/D/unknown | 2 bits | 1.15 GiB |
| remaining-child count | `u8` | 4.60 GiB |
| distance | `u16` | 9.20 GiB |
| all three | 3.25 bytes/state | 14.95 GiB |

This excludes rank metadata, queues/frontiers, reachability structures, and OS
overhead. A naïve `u64` queue containing every state would add 36.8 GiB by
itself, so "the table fits" is not the same as "the solver fits."

The symmetry group that preserves pawn orientation has up to four elements:
left-right reflection and color-swap plus 180° rotation. The exact canonical
count needs Burnside/fixed-point accounting or enumeration; dividing by four is
only a rough projection (~1.23 billion).

The decisive next number is the canonical reachable non-terminal count from
the initial position.

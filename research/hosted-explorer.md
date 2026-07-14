# Hosted tablebase explorer boundary

Status: implemented and verified locally; artifact provisioning and public
deployment remain.

## Product behavior

The explorer starts at the empty board and allows legal play forward. For the
current position it shows:

- side to move and W/L/D value;
- finite remoteness for wins and losses, and no numeric distance for draws;
- every legal move with its result from the mover's perspective;
- whether the move preserves W/L/D;
- whether it is remoteness-optimal: fastest win, any drawing continuation, or
  longest resistance in a loss;
- the canonical dense key used for the table lookup.

The explorer also serves a sourced solve write-up at `/write-up/`. It keeps the
exhaustive tablebase proof distinct from the illustrative drawing lasso and
links the production record and generated strategic report.

Shareable links should initially encode move history from the empty board. This
is unambiguous and lets the rules engine validate every transition. A later
advanced position editor must explicitly represent side to move, whether the
six-ply opening is complete, and each on-board pawn's current travel direction;
board occupancy alone is not a complete state.

## Backend contract

The Rust service loads and validates `post-opening-travel.tb` once at startup.
The first implementation can retain its 2,476,597,610 data bytes in a `Vec` on
an 8 GB host. A memory-mapped loader can subsequently reduce startup allocation
and let the operating system page the same immutable format.

The probe endpoint accepts either validated move history or a fully specified
engine position and returns JSON equivalent to the library's `ProbeResult`:

- canonical phase and ID;
- current value and optional distance;
- legal action and child key;
- mover-relative value and optional distance after each action;
- `preserves_result` and `optimal` flags.

Moves are generated on demand. The service never stores or downloads the
28,730,418,180-edge graph. At deployment, startup must verify rules tag
`0x54544303`, internal CRC-64/XZ `0xeb952765179a695e`, dimensions, encoding,
and the published SHA-256 before becoming healthy.

## Frontend contract

The frontend owns presentation and move-history navigation; it does not
reimplement legality, ranking, W/L/D inversion, or optimality. The API response
is the authority. The board should visibly mark pawn travel direction after a
reversal, because that direction affects future captures under the canonical
rules.

## Delivery sequence

1. Completed: stable JSON responses and checked move-history replay.
2. Completed: long-lived HTTP service with one validated artifact load.
3. Completed: board, hands, dragging, ranked moves, history navigation, move
   previews, terminal presentation, and write-up.
4. Pending deployment: artifact provisioning and startup verification on the
   public host.
5. Completed locally: browser/API behavior checked against engine fixtures for
   opening replay, absolute orientation, decisive distance, and terminal play.

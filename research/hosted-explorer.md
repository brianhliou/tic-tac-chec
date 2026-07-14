# Hosted tablebase explorer boundary

Status: implementation target after the canonical remoteness artifact and
move-by-move Rust probe.

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

1. Add stable JSON types and move-history replay around the existing probe.
2. Add a long-lived HTTP service that loads the artifact once.
3. Build the board, move list, history navigation, and shareable links.
4. Add artifact provisioning and startup verification on the host.
5. Validate browser/API results against the CLI on fixed opening, decisive,
   returning-pawn, capture/re-entry, and terminal fixtures.

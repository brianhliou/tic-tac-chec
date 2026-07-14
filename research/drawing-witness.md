# Drawing-strategy witness

Status: implementation specification for the canonical 1998
travel-direction tablebase.

## What the witness establishes

The audited retrograde tablebase is the proof that the empty board is drawn.
The compact witness is a human-facing consequence of that proof: it follows a
deterministic draw-preserving policy from the empty board until an exact engine
position repeats. The prefix plus repeated cycle is an infinite legal line in
which neither side concedes a forced loss.

This lasso is not, by itself, a standalone strategy proof against every
possible deviation. The full draw policy is the tablebase oracle: at every
drawn position, choose a child whose mover-relative value is draw. If the
opponent deviates to a mover-relative loss, they have conceded a forced win;
otherwise play remains in the draw region.

## Deterministic policy

At every drawn position:

1. Probe every legal action against the checksummed tablebase.
2. Keep exactly the actions whose mover-relative result is draw.
3. Choose the least action under this stable tuple order:
   - placements before board moves;
   - placements by piece kind (`pawn`, `knight`, `bishop`, `rook`) and then
     destination square index;
   - board moves by source square index and then destination square index.

The policy depends only on the current position, not on path history. Once an
exact position repeats, the same choices repeat forever. Cycle detection uses
the full absolute engine position, including side to move, opening phase, and
pawn travel directions; a match of symmetry-normalized table IDs alone is not
accepted as an exact repetition.

## Artifact contract

The JSON witness records:

- tablebase rules tag and CRC;
- policy identifier and extraction limit;
- prefix length, cycle length, and repeated position key;
- for every ply, the absolute side to move, normalized tablebase key, selected
  legal-move index and notation, child key, and counts of winning, drawing, and
  losing alternatives from the mover's perspective.

Replaying the indexed actions from the empty board must reproduce every key,
every draw value, and the final exact repetition. The production run record
must include the command, output checksum, prefix/cycle lengths, and source
commit.

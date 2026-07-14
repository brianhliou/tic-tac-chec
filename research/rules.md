# Rules ledger

Last reviewed: 2026-07-13.

## Canonical target: Dream Green (1998 rules)

The strongest freely accessible rules artifact found is the two-page Dream
Green printable rules sheet mirrored by Tippechess. Its operative text says:

- empty 4×4 board; White moves first;
- players alternate placing a piece on any empty square;
- "After placing your first three pieces, you may move and capture as in
  regular chess";
- a captured piece returns to its owner and may be placed again;
- pawns reverse after reaching the opposite rank, reverse again after returning
  to the first rank, and reset toward the opponent after capture/redeployment;
- four same-color pieces in a rank, file, or long diagonal wins.

Source: [Dream Green printable rules
(PDF)](https://redcanoe.weebly.com/uploads/7/4/5/0/7450428/ttcwb2.pdf).
The GamesCrafters rules page cites the original printed instructions as Don
Green, *Tic Tac Chec*, Dream Green Instructions (1998): [GamesCrafters rules
page](https://gamescrafters.berkeley.edu/site-legacy-archive-sp20/games.php?game=tictacchec).

### Formalization used by the solver

1. Each player owns exactly one pawn, knight, bishop, and rook.
2. White moves first.
3. During the opening, turns are placements only. The opening becomes complete
   once both players have placed three pieces (after six plies in every
   non-terminal game). Captures never re-lock movement.
4. After the opening, a legal turn is exactly one placement or one move.
5. Placement puts any in-hand piece on any empty square. It is not a capture.
6. Rook, bishop, and knight movement/capture is standard chess movement, with
   no check or king rules.
7. A pawn moves one square straight into an empty square. There is no initial
   two-square move in the canonical original variant; contemporary summaries
   of the original consistently describe a one-square pawn.
8. An on-board pawn carries a direction. Redeployment resets it toward the
   opponent. A pawn placed on or arriving at an end rank points inward on the
   resulting position.
9. A capture returns the captured piece to its owner's hand. "Immediately"
   means it is available on that owner's next turn, not an interrupting move.
10. A move that forms the mover's four-piece line ends the game before another
    turn is taken.
11. The printed rules give no repetition limit. We use the standard reachability
    value for an unbounded loopy game: unresolved cycles after win/loss
    retrograde propagation are draws.

## Named ambiguity: returning-pawn capture

The printable rules say both that a pawn "reverses direction" and that pawns
"only capture when moving forward." The two principal readings are:

- `travel-direction` (current canonical interpretation): the pawn captures
  diagonally in whichever direction it is currently traveling.
- `outbound-only` (conservative alternate): a pawn on its return trip cannot
  capture.

No located primary source explicitly illustrates a capture during the return
trip, so this is not a definitive designer ruling. The balance of evidence
favors `travel-direction`: the original says the pawn reverses *direction*; a
detailed French rules transcription defines captures as diagonally "forward"
and advises physically orienting or marking the pawn to remember its current
direction; and an independent playable implementation applies the current
direction to both movement and capture. Sources: [French rules
transcription](https://www.zpag.net/Jeux/Echec/tic_tac_chec.html) and
[cutalion/tic-tac-chec](https://github.com/cutalion/tic-tac-chec).

The engine also supports the less natural literal geometry reading
`toward-opponent`, in which a returning pawn moves straight toward home but may
capture diagonally toward the opponent. We should still obtain a
designer/publisher ruling.

The `outbound-only` sensitivity table was strongly solved and independently
audited on 2026-07-13. Its initial value is also a draw, but it removes
426,173,880 post-opening moves and changes W/L/D value at 16,529,908 dense
post-opening positions. See the [run summary](runs/outbound-only-2026-07-13/summary.md)
and [exact transition matrix](runs/outbound-only-2026-07-13/variant-comparison.md).

## Other missing adjudications

- **No legal move:** the rules do not define stalemate. Before inventing a
  result, enumeration should determine whether a reachable non-terminal state
  with no legal move exists.
- **Repetition for human/tournament play:** absent from the original printable
  rules. A threefold rule would make values history-dependent (the Graph
  History Interaction problem), so it must be modeled separately if adopted.
- **Pawn double move:** the original rules say "as in regular chess" but
  original-era rules summaries specify a one-square pawn. The 2025 board
  explicitly adds the two-square first move, evidence that it is a new-edition
  rule rather than the original default.

## Separate variant: Bobby Fischer edition (2025)

Wood Expressions' current product materials describe a materially changed
game:

- the board says a pawn may move two squares on its first move;
- the published board/product imagery does not state the original bouncing-pawn
  rule;
- the box says captures begin once five pieces have entered play;
- marketing instructions say pieces may be placed on any turn and an on-board
  piece moves/captures as in chess.

Sources: [publisher product
page](https://woodexpressions.com/products/bobby-fischer-tic-tac-chec) and its
official product imagery. This variant needs a complete rules transcription
before implementation.

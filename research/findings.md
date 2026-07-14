# Research findings

Last reviewed: 2026-07-13.

## Prior public solved status

No previously published strong or weak solution, proof of the initial value,
or public tablebase was found. This is a negative search result, not proof that
no private or obscure solve exists.

This project subsequently produced an independently audited strong W/L/D
solution of the canonical original-edition rules: the initial position is a
draw. See the
[production summary](runs/production-2026-07-13/summary.md) for the exact
result and reproduction record.

The game is listed by GamesCrafters, but the current `GamesmanClassic` source is
not a solution of the commercial 4×4 game. At commit
`ca21c5fdcf1e509027450f54be65451d15354df2`, `src/mttc.c` defaults to a 3×3
bishop/knight/rook game, generates movement immediately, has no opening-history
or pawn-direction state, and its pawn move generator is empty. It is useful as
historical prior art, not as an oracle or solved-status claim.

Source: [GamesmanClassic `mttc.c`](https://github.com/GamesCrafters/GamesmanClassic/blob/master/src/mttc.c).

## Existing playable implementation

Alexander Glushkov's current Go project implements a polished playable game and
an AlphaZero-style bot. At commit
`5c274bfc75aeba5641fa8353d64cbd8c4becbbbf`, its engine:

- permits movement without the original three-placement opening restriction;
- resets captured pawns and reverses them at board edges;
- lets a returning pawn capture along its travel direction;
- contains no exhaustive solution or tablebase.

It can become a useful independent oracle for the shared post-opening movement
subset after we account for the variant differences.

Source: [cutalion/tic-tac-chec](https://github.com/cutalion/tic-tac-chec).

## Returning-pawn adjudication

No primary-source diagram, example game, or designer statement found in the
public search explicitly shows whether a returning pawn can capture. The
current canonical interpretation is nevertheless `travel-direction`:

- the Dream Green sheet says the pawn "reverses direction" and captures only
  when moving "forward";
- a detailed French rules transcription likewise says captures are diagonally
  forward and recommends orienting or marking the pawn to remember the
  direction it has taken;
- the independent Go implementation uses the pawn's current direction for its
  capture diagonals.

The second item is the clearest interpretive clue: "forward" naturally becomes
the pawn's current facing after it reverses. `outbound-only` remains implemented
as a sensitivity variant. A direct question to Don Green or the current
publisher would supersede this provisional adjudication.

Sources: [Dream Green printable rules
(PDF)](https://redcanoe.weebly.com/uploads/7/4/5/0/7450428/ttcwb2.pdf), [French
rules transcription](https://www.zpag.net/Jeux/Echec/tic_tac_chec.html), and
[cutalion/tic-tac-chec](https://github.com/cutalion/tic-tac-chec).

## Academic/technical prior art

A 2023 University of Patras thesis, *Developing an application for the game
Tic-Tac-Chec* by Ioannis Panethymitakis, reports Python AI algorithms and a GUI.
The available metadata does not claim an exhaustive solution. Obtaining and
reviewing the full thesis remains useful for bibliography and possible test
positions.

Source: [OpenArchives metadata](https://www.openarchives.gr/aggregator-openarchives/edm/nemertes/000009-10889_24691).

## Closest local solver precedent

The archived `gobblet-gobblers` project is the closest architectural sibling:
both games are small alignment games with captures/re-entry and cycles. Its
successful pipeline was canonical reachable-state enumeration followed by
loopy retrograde analysis; unresolved residue became draws. The local
`dobutsu-shogi`, `shogi4`, and `micro-shogi` projects provide the dense-rank,
generated-predecessor, and audit patterns needed at larger scale.

## Current recommendation

Preserve the audited dense retrograde table as the source of truth. Build a
probe and strategy witness on top of it, then run the alternate returning-pawn
interpretation through the same pipeline as a sensitivity result. Forward
search remains useful for extracting compact lines, but should be checked
against table values because cycles make history-unsound transposition caching
easy to get wrong.

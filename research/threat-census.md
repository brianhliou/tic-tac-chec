# Immediate-threat and defensive-choice census

Exhaustive census over both indexed structural domains for the original-edition travel-direction rules.

- Census source commit: `e8f3ae79462fabce815ef95836b8806418b166a4`
- Rules tag: `0x54544303`
- Tablebase CRC-64/XZ: `0xeb952765179a695e`
- Threads: 16; elapsed: 579.495 seconds
- Independent detector mismatches: 0
- Tablebase invariant failures: 0

## Definitions

A live threat is a distinct legal action the opponent could play immediately on the same board to complete four in a row. This is stricter than a geometric three-piece line: it accounts for the missing piece's location, slider blocking, pawn direction and capture rules, target occupancy, and the opening movement lock.

A safe response wins immediately or leaves the opponent with no immediate winning action. A drawing response is a safe response that preserves the tablebase draw. An immediate-loss move from a drawn position gives the opponent a win in one ply, so the original mover loses in two plies.

## post-opening

| Result | All positions | Live-threat positions | Threat rate |
| --- | ---: | ---: | ---: |
| Win | 184,895,598 | 7,456,340 | 4.032730% |
| Loss | 24,178,920 | 5,500,978 | 22.751132% |
| Draw | 2,253,286,227 | 108,060,290 | 4.795675% |

- Positions: 2,462,360,745
- Terminal positions: 18,516,912
- Live-threat positions: 121,017,608 (4.914699%)
- Answerable live threats: 117,255,072 (96.890919%)
- Each live-threat position has exactly one immediate winning action: 121,017,608 total (drop 33,973,224, move-to-empty 58,217,008, capture 28,827,376)
- Unanswerable threats: 3,762,536 (first at `post:105106681`)
- Loss-in-2 positions: 3,762,536

### Drawn-position choice width

- Drawn positions: 2,253,286,227
- Legal moves: 26,538,178,222; drawing moves: 24,734,672,866 (93.204110%)
- Positions where every move draws: 1,745,115,483 (77.447572%)
- Positions where a majority of moves draw: 2,132,233,767 (94.627737%)
- Positions with exactly one drawing move: 17,340,704 (0.769574%)
- Immediate-loss moves: 1,175,402,772 across 267,172,406 drawn positions (11.857011% of drawn positions)

### Live threats inside draws

- Drawn positions with a live threat: 108,060,290 (4.795675% of draws)
- Safe responses in drawn live-threat positions: 328,038,896 (25.734342% of their legal moves)
- Drawing responses in drawn live-threat positions: 320,246,498 (25.123036% of their legal moves)
- Safe responses across all live threats: 358,428,448 of 1,434,077,632 legal moves (24.993657%)
- Drawn live-threat positions with exactly one safe response: 13,851,610
- Drawn live-threat positions with exactly one drawing response: 16,462,872

The machine-readable artifact contains the complete legal-moves × drawing-moves distribution plus safe-response and drawing-response histograms.

## locked opening

| Result | All positions | Live-threat positions | Threat rate |
| --- | ---: | ---: | ---: |
| Win | 147,472 | 2,222 | 1.506727% |
| Loss | 30,468 | 30,292 | 99.422345% |
| Draw | 14,058,925 | 727,806 | 5.176825% |

- Positions: 14,236,865
- Terminal positions: 0
- Live-threat positions: 760,320 (5.340502%)
- Answerable live threats: 760,320 (100.000000%)
- Each live-threat position has exactly one immediate winning action: 760,320 total (drop 760,320, move-to-empty 0, capture 0)
- Unanswerable threats: 0
- Loss-in-2 positions: 0

### Drawn-position choice width

- Drawn positions: 14,058,925
- Legal moves: 313,770,656; drawing moves: 289,488,104 (92.261051%)
- Positions where every move draws: 11,614,025 (82.609623%)
- Positions where a majority of moves draw: 13,095,483 (93.147115%)
- Positions with exactly one drawing move: 54,524 (0.387825%)
- Immediate-loss moves: 14,556,120 across 727,806 drawn positions (5.176825% of drawn positions)

### Live threats inside draws

- Drawn positions with a live threat: 727,806 (5.176825% of draws)
- Safe responses in drawn live-threat positions: 1,455,612 (9.090909% of their legal moves)
- Drawing responses in drawn live-threat positions: 1,401,162 (8.750846% of their legal moves)
- Safe responses across all live threats: 1,520,640 of 16,727,040 legal moves (9.090909%)
- Drawn live-threat positions with exactly one safe response: 0
- Drawn live-threat positions with exactly one drawing response: 54,450

The machine-readable artifact contains the complete legal-moves × drawing-moves distribution plus safe-response and drawing-response histograms.

## Interpretation

The indexed-domain results support a precise version of the attention-game hypothesis. Away from immediate threats, drawn positions are broadly forgiving: 93.204110% of all legal moves from drawn post-opening positions preserve the draw, and 77.447572% of those positions draw after every legal move. A live threat sharply narrows the choice: only 25.123036% of moves from drawn live-threat positions preserve the draw, and 15.234895% of those positions have exactly one drawing response.

The stronger claim that every threat can be stopped is false. Still, 96.890919% of post-opening live threats have at least one safe response, and every live threat in a drawn position is answerable. The 3,762,536 unanswerable cases are exactly the tablebase's loss-in-2 positions, independently confirming the tactical definition.

## Scope and interpretation guardrails

These are exact counts over the solver's indexed structural domains, not frequencies under human play and not a claim that every indexed position is reachable from the empty board. The post-opening domain normalizes the player to move to White by color-swap plus 180-degree rotation. Threats are tactical one-move facts; tablebase values supply the game-theoretic classification.

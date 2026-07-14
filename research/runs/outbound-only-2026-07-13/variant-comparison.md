# Returning-pawn variant comparison

Exact position-by-position comparison of the original-edition travel-direction and outbound-only result-plus-remoteness tablebases.

- Comparator source commit: `5653f278d87e7e4f4b19c9a99524985792c9f752`
- Canonical travel-direction tag: `0x54544303`; CRC-64/XZ: `0xeb952765179a695e`
- Outbound-only tag: `0x54544301`; CRC-64/XZ: `0x9727127fadf34fca`
- Initial value: Draw under travel-direction, Draw under outbound-only

## Post-opening

| Travel ↓ / outbound → | Win | Loss | Draw |
| --- | ---: | ---: | ---: |
| Win | 180,747,956 | 9,366 | 4,138,276 |
| Loss | 810 | 24,131,682 | 46,428 |
| Draw | 11,146,988 | 1,188,040 | 2,240,951,199 |

- Positions: 2,462,360,745
- W/L/D value changes: 16,529,908 (0.671303%)
- Exact result-or-distance code changes: 17,701,586 (0.718887%)
- Same-result decisive positions with a changed distance: 1,171,678
- Maximum same-result distance change: 36 plies (first at `post:1221749941`)
- First dense representatives:
  - Win → Loss: `post:126619775` (9,366 positions)
  - Win → Draw: `post:124228` (4,138,276 positions)
  - Loss → Win: `post:54711078` (810 positions)
  - Loss → Draw: `post:3590505` (46,428 positions)
  - Draw → Win: `post:86795` (11,146,988 positions)
  - Draw → Loss: `post:3680880` (1,188,040 positions)

## Locked opening

| Travel ↓ / outbound → | Win | Loss | Draw |
| --- | ---: | ---: | ---: |
| Win | 147,418 | 0 | 54 |
| Loss | 0 | 30,464 | 4 |
| Draw | 13,846 | 3,008 | 14,042,071 |

- Positions: 14,236,865
- W/L/D value changes: 16,912 (0.118790%)
- Exact result-or-distance code changes: 19,524 (0.137137%)
- Same-result decisive positions with a changed distance: 2,612
- Maximum same-result distance change: 16 plies (first at `opening:162657`)
- First dense representatives:
  - Win → Draw: `opening:1704816` (54 positions)
  - Loss → Draw: `opening:11799609` (4 positions)
  - Draw → Win: `opening:85095` (13,846 positions)
  - Draw → Loss: `opening:1664517` (3,008 positions)

## Interpretation

The rule ambiguity does not change the perfect-play value of the empty board, but it changes values and decisive distances throughout the table. Matrix rows are the canonical travel-direction result; columns are the outbound-only result. IDs are the lowest dense representative for each nonempty transition, not claims about reachability from the empty board or strategic frequency. Both artifacts were solved and audited independently before this bytewise comparison.

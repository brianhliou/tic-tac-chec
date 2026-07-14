# Canonical strategic report

Generated from the complete result-plus-remoteness tablebase.

- Extractor source commit: `ef5ee378e0653fb8eb8a0593147318bcfbc81e14`
- Rules: original Dream Green edition, travel-direction pawn captures
- Rules tag: `0x54544303`
- Tablebase CRC-64/XZ: `0xeb952765179a695e`
- Post-opening positions: 2,462,360,745
- Locked-opening positions: 14,236,865

## Exact census

| Section | Wins | Losses | Draws |
| --- | ---: | ---: | ---: |
| Post-opening | 184,895,598 | 24,178,920 | 2,253,286,227 |
| Locked opening | 147,472 | 30,468 | 14,058,925 |

### Decisive remoteness distribution

Distance is measured in plies to a terminal four-in-a-row under fastest-win/longest-loss play.

| Plies | Result | Post-opening | Locked opening |
| ---: | --- | ---: | ---: |
| 0 | Loss | 18,516,912 | 0 |
| 1 | Win | 121,017,608 | 0 |
| 2 | Loss | 3,763,248 | 0 |
| 3 | Win | 44,671,112 | 0 |
| 4 | Loss | 1,213,110 | 25,864 |
| 5 | Win | 13,475,374 | 128,370 |
| 6 | Loss | 444,228 | 3,556 |
| 7 | Win | 4,114,500 | 14,368 |
| 8 | Loss | 132,854 | 600 |
| 9 | Win | 941,496 | 2,586 |
| 10 | Loss | 55,888 | 240 |
| 11 | Win | 366,616 | 1,046 |
| 12 | Loss | 24,430 | 82 |
| 13 | Win | 147,160 | 512 |
| 14 | Loss | 12,220 | 36 |
| 15 | Win | 73,198 | 252 |
| 16 | Loss | 6,158 | 18 |
| 17 | Win | 34,412 | 140 |
| 18 | Loss | 2,926 | 12 |
| 19 | Win | 16,302 | 42 |
| 20 | Loss | 2,114 | 18 |
| 21 | Win | 11,550 | 48 |
| 22 | Loss | 1,124 | 12 |
| 23 | Win | 5,638 | 24 |
| 24 | Loss | 678 | 12 |
| 25 | Win | 3,642 | 20 |
| 26 | Loss | 806 | 12 |
| 27 | Win | 4,472 | 18 |
| 28 | Loss | 788 | 6 |
| 29 | Win | 4,184 | 22 |
| 30 | Loss | 528 | 0 |
| 31 | Win | 2,874 | 8 |
| 32 | Loss | 424 | 0 |
| 33 | Win | 2,680 | 8 |
| 34 | Loss | 252 | 0 |
| 35 | Win | 1,570 | 4 |
| 36 | Loss | 148 | 0 |
| 37 | Win | 842 | 0 |
| 38 | Loss | 74 | 0 |
| 39 | Win | 340 | 4 |
| 40 | Loss | 10 | 0 |
| 41 | Win | 28 | 0 |

## Representative longest decisive lines

For each section and result, this selects the lowest dense ID at that result's maximum remoteness. Tied optimal actions use the same deterministic action ordering as the drawing witness. Each line was replayed through checked move application and ends at a terminal loss code at exactly the advertised distance.

### post-opening Win: 41 plies

- Start: `post:1059850930`
- Side to move: White
- Terminal: `post:1121700630`; winner: White

```text
4 P b . . 
3 n . R . 
2 . . r p 
1 N . . . 
  a b c d
White hand: B
Black hand: —
Pawn directions: White ↓, Black ↓
```

1. White B@d1 (`post:1059850930`) · 2. Black c2-c1 (`post:1937912181`) · 3. White c3-c1 (`post:2285094829`) · 4. Black R@b2 (`post:870187499`) · 5. White a1-b3 (`post:2285088800`) · 6. Black b2-b3 (`post:1937912625`) · 7. White N@d4 (`post:1197311660`) · 8. Black b3-b2 (`post:1937910285`)  
9. White c1-c3 (`post:2287109490`) · 10. Black b4-c3 (`post:1937912261`) · 11. White R@b3 (`post:908987015`) · 12. Black b2-b3 (`post:1937936022`) · 13. White d4-b3 (`post:908987017`) · 14. Black a3-c2 (`post:61419356`) · 15. White R@c1 (`post:65290530`) · 16. Black d2-c1 (`post:870206518`)  
17. White d1-c2 (`post:65098195`) · 18. Black N@d1 (`post:53439743`) · 19. White R@d2 (`post:65098569`) · 20. Black c3-d2 (`post:912797626`) · 21. White c2-d1 (`post:65098567`) · 22. Black N@c2 (`post:53440226`) · 23. White R@b2 (`post:65098193`) · 24. Black c1-b2 (`post:912750237`)  
25. White d1-c2 (`post:65218445`) · 26. Black N@d1 (`post:53197799`) · 27. White R@c1 (`post:65218698`) · 28. Black d2-c1 (`post:886117898`) · 29. White c2-d1 (`post:65218696`) · 30. Black N@c2 (`post:53198426`) · 31. White R@c3 (`post:65218443`) · 32. Black b2-c3 (`post:886087555`)  
33. White d1-c2 (`post:65460410`) · 34. Black N@d1 (`post:52717931`) · 35. White R@d2 (`post:65460784`) · 36. Black c1-d2 (`post:833268946`) · 37. White c2-d1 (`post:65460787`) · 38. Black N@c2 (`post:52717153`) · 39. White R@b2 (`post:65460413`) · 40. Black R@a1 (`post:833212196`)  
41. White b2-c2 (`post:2301628701`)  

### post-opening Loss: 40 plies

- Start: `post:1937912181`
- Side to move: White
- Terminal: `post:1121684669`; winner: Black

```text
4 b . . n 
3 P R . . 
2 . r . N 
1 . . B p 
  a b c d
White hand: —
Black hand: —
Pawn directions: White ↑, Black ↑
```

1. White b3-b4 (`post:1937912181`) · 2. Black b2-b4 (`post:2285094829`) · 3. White R@b2 (`post:870187499`) · 4. Black d4-c2 (`post:2285088803`) · 5. White b2-c2 (`post:1937909565`) · 6. Black N@a1 (`post:1197311660`) · 7. White c2-c3 (`post:1937910285`) · 8. Black b4-b2 (`post:2287109490`)  
9. White c1-b2 (`post:1937912261`) · 10. Black R@c2 (`post:908987015`) · 11. White c3-c2 (`post:1937936022`) · 12. Black a1-c2 (`post:908987017`) · 13. White d2-b3 (`post:61419356`) · 14. Black R@b4 (`post:65290530`) · 15. White a3-b4 (`post:870206518`) · 16. Black a4-b3 (`post:65098195`)  
17. White N@a4 (`post:53439743`) · 18. Black R@a3 (`post:65098569`) · 19. White b2-a3 (`post:912797626`) · 20. Black b3-a4 (`post:65098567`) · 21. White N@b3 (`post:53440226`) · 22. Black R@c3 (`post:65098193`) · 23. White b4-c3 (`post:912750237`) · 24. Black a4-b3 (`post:65218445`)  
25. White N@a4 (`post:53197799`) · 26. Black R@b4 (`post:65218698`) · 27. White a3-b4 (`post:886117898`) · 28. Black b3-a4 (`post:65218696`) · 29. White N@b3 (`post:53198426`) · 30. Black R@b2 (`post:65218443`) · 31. White c3-b2 (`post:886087555`) · 32. Black a4-b3 (`post:65460410`)  
33. White N@a4 (`post:52717931`) · 34. Black R@a3 (`post:65460784`) · 35. White b4-a3 (`post:833268946`) · 36. Black b3-a4 (`post:65460787`) · 37. White N@b3 (`post:52717153`) · 38. Black R@c3 (`post:65460413`) · 39. White R@a1 (`post:833212196`) · 40. Black c3-b3 (`post:2301628709`)  

### opening Win: 39 plies

- Start: `opening:3507218`
- Side to move: Black
- Terminal: `post:1121684669`; winner: Black

```text
4 . r . . 
3 P . . . 
2 . . . N 
1 . . B p 
  a b c d
White hand: R
Black hand: NB
Pawn directions: White ↑, Black ↑
```

1. Black B@a4 (`opening:3507218`) · 2. White R@a2 (`post:468343281`) · 3. Black N@c2 (`post:1197311662`) · 4. White a2-c2 (`post:1937908575`) · 5. Black N@a1 (`post:1197311660`) · 6. White c2-c3 (`post:1937910285`) · 7. Black b4-b2 (`post:2287109490`) · 8. White c1-b2 (`post:1937912261`)  
9. Black R@c2 (`post:908987015`) · 10. White c3-c2 (`post:1937936022`) · 11. Black a1-c2 (`post:908987017`) · 12. White d2-b3 (`post:61419356`) · 13. Black R@b4 (`post:65290530`) · 14. White a3-b4 (`post:870206518`) · 15. Black a4-b3 (`post:65098195`) · 16. White N@a4 (`post:53439743`)  
17. Black R@a3 (`post:65098569`) · 18. White b2-a3 (`post:912797626`) · 19. Black b3-a4 (`post:65098567`) · 20. White N@b3 (`post:53440226`) · 21. Black R@c3 (`post:65098193`) · 22. White b4-c3 (`post:912750237`) · 23. Black a4-b3 (`post:65218445`) · 24. White N@a4 (`post:53197799`)  
25. Black R@b4 (`post:65218698`) · 26. White a3-b4 (`post:886117898`) · 27. Black b3-a4 (`post:65218696`) · 28. White N@b3 (`post:53198426`) · 29. Black R@b2 (`post:65218443`) · 30. White c3-b2 (`post:886087555`) · 31. Black a4-b3 (`post:65460410`) · 32. White N@a4 (`post:52717931`)  
33. Black R@a3 (`post:65460784`) · 34. White b4-a3 (`post:833268946`) · 35. Black b3-a4 (`post:65460787`) · 36. White N@b3 (`post:52717153`) · 37. Black R@c3 (`post:65460413`) · 38. White R@a1 (`post:833212196`) · 39. Black c3-b3 (`post:2301628709`)  

### opening Loss: 28 plies

- Start: `opening:2594484`
- Side to move: Black
- Terminal: `post:1121700630`; winner: White

```text
4 P . . . 
3 . N b . 
2 . p . . 
1 . . . B 
  a b c d
White hand: R
Black hand: NR
Pawn directions: White ↓, Black ↓
```

1. Black N@c2 (`opening:2594484`) · 2. White R@c1 (`post:65194423`) · 3. Black b2-c1 (`post:891364798`) · 4. White d1-c2 (`post:65098195`) · 5. Black N@d1 (`post:53439743`) · 6. White R@d2 (`post:65098569`) · 7. Black c3-d2 (`post:912797626`) · 8. White c2-d1 (`post:65098567`)  
9. Black N@c2 (`post:53440226`) · 10. White R@b2 (`post:65098193`) · 11. Black c1-b2 (`post:912750237`) · 12. White d1-c2 (`post:65218445`) · 13. Black N@d1 (`post:53197799`) · 14. White R@c1 (`post:65218698`) · 15. Black d2-c1 (`post:886117898`) · 16. White c2-d1 (`post:65218696`)  
17. Black N@c2 (`post:53198426`) · 18. White R@c3 (`post:65218443`) · 19. Black b2-c3 (`post:886087555`) · 20. White d1-c2 (`post:65460410`) · 21. Black N@d1 (`post:52717931`) · 22. White R@d2 (`post:65460784`) · 23. Black c1-d2 (`post:833268946`) · 24. White c2-d1 (`post:65460787`)  
25. Black N@c2 (`post:52717153`) · 26. White R@b2 (`post:65460413`) · 27. Black R@a1 (`post:833212196`) · 28. White b2-c2 (`post:2301628701`)  

## Critical choices on one drawing lasso

The `least-drawing-action-v1` line has a 32-ply prefix and a 18-ply exact cycle. These positions illustrate where that one deterministic drawing line becomes tactically unforgiving; they are not claims about frequency across all play or a standalone proof of the draw.

### Earliest losing deviation: ply 10

- Position: `post:1647406695`
- Side to move: White
- Policy move: `c1-b3`
- Drawing choices: 8 of 9 legal moves
- Losing choices: 1

```text
4 . . . . 
3 . . . . 
2 B P R r 
1 b p N n 
  a b c d
White hand: —
Black hand: —
Pawn directions: White ↑, Black ↑
```

Drawing: `c1-b3`, `c1-d3`, `a2-b1`, `a2-b3`, `b2-b3`, `c2-d2`, `c2-c3`, `c2-c4`  
Losing: `a2-c4` (loss in 4 plies)  

### Narrowest drawing defense: ply 19

- Position: `post:1276048331`
- Side to move: Black
- Policy move: `a1-b2`
- Drawing choices: 3 of 18 legal moves
- Losing choices: 15

```text
4 . . . . 
3 . N n . 
2 B P R r 
1 b . . . 
  a b c d
White hand: —
Black hand: P
Pawn directions: White ↑, Black ↓
```

Drawing: `a1-b2`, `d2-c2`, `c3-a2`  
Losing: `P@b1` (loss in 2 plies), `P@c1` (loss in 2 plies), `P@d1` (loss in 2 plies), `P@a3` (loss in 2 plies), `P@d3` (loss in 2 plies), `P@a4` (loss in 2 plies), `P@b4` (loss in 2 plies), `P@c4` (loss in 2 plies), `P@d4` (loss in 2 plies), `d2-d1` (loss in 2 plies), `d2-d3` (loss in 2 plies), `d2-d4` (loss in 2 plies), `c3-b1` (loss in 2 plies), `c3-d1` (loss in 2 plies), `c3-a4` (loss in 2 plies)  

## Interpretation limits

The census is exhaustive over the solver's dense structural domains. The principal variations and drawing choices are deterministic examples, not unique strategies. Dense post-opening IDs normalize the player to move to White; the full engine positions used for lasso repetition retain absolute color, side to move, opening phase, and both pawn directions. The independently audited tablebase—not these selected lines—is the strong-solution proof.

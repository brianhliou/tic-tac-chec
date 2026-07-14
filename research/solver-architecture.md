# Solver architecture research

Last reviewed: 2026-07-13.

## Recommendation

Use an in-memory, dense-rank, push-retrograde solver with generated
predecessors. Normalize every post-opening position into player-to-move
coordinates, so the side-to-move factor disappears and the dense domain is
**2,462,360,745** positions before left-right reflection. This fits in `u32`.

Keep the six-ply placement-only opening in its own small rank space. It has
only 14,236,865 dense states and is acyclic; mixing an opening-history bit into
every later state would waste space and complicate the main graph.

The initial implementation should use byte arrays with relaxed atomic updates,
sixteen local workers, and thread-local `u32` frontier buffers. Full-address
rehearsals sustained 20.95 million generated-and-updated predecessors per
second with 5.17 GiB peak RSS; see the [parallel scaling record](runs/parallel-retrograde-2026-07-13/summary.md).
Two-bit values, left-right symmetry, bit-vector frontiers, sharded BSP, and
external-memory batching should each be enabled only after a later benchmark
establishes its tradeoff.

## Why this baseline

The closest local experiments already answer the largest architectural
question. Micro Shogi's 869,287,068-position KPG game used about 60.5 GiB and
4:21 with reachable IDs plus stored predecessor CSR, versus about 6.86 GiB and
2:21 with dense rank plus generated predecessors. See the [Micro Shogi
results](../../_archive/micro-shogi/README.md). Storing reverse edges is both
the memory and runtime trap.

Published work points the same way:

- reversible perfect hashing permits positions and frontiers to be represented
  by compact ordinal IDs and bit vectors ([Edelkamp, Sulewski, and Yücel,
  2010](https://cdn.aaai.org/ojs/13414/13414-40-16932-1-2-20201228.pdf));
- entirely in-core retrograde avoids the severe cost of random out-of-core
  access, while symmetry and blocked layouts can reduce work and improve
  locality ([Irving, 2014](https://arxiv.org/abs/1404.0743));
- when a computation must exceed RAM, successful designs partition work and
  turn random accesses into asynchronous or sequential batches ([Romein and
  Bal, 2003](https://doi.org/10.1109/MC.2003.1236468), [Korf,
  2004](https://cdn.aaai.org/AAAI/2004/AAAI04-103.pdf));
- bitmap retrograde can reduce working memory to one bit per position while
  retaining distance information on disk, making it a credible fallback rather
  than the first implementation ([Wu and Beal,
  2002](https://library.slmath.org/books/Book42/files/wu.pdf)).

Tic Tac Chec is loopy and does not decompose monotonically by material: captures
remove pieces from the board but return them for later placement. Its natural
algorithm is therefore the standard attractor fixpoint:

1. Seed terminal losses for the player to move.
2. A predecessor of a loss is a win.
3. A position becomes a loss once every distinct child is a win.
4. Legal reachable positions left unknown at the fixpoint are draws.

A forward solver with path-based repetition detection can be a useful witness
finder, but it should not be the source of truth. The local Gobblet project
records a concrete graph-history interaction failure from caching path-derived
draws; see its [retrograde correction
report](../../_archive/gobblet-gobblers/docs/retrograde_correction.md).

## State representation and ranking

Represent a position internally as a compact value, with:

- sixteen-square occupancy bitboards;
- one location-or-hand value for each of the eight distinct pieces;
- a direction bit for each on-board pawn when the square does not force the
  inward direction;
- no absolute side-to-move bit in the post-opening domain.

After every move, rotate 180 degrees and exchange `us` with `them`. The next
position is again represented from the new player's perspective. This is an
exact bijection, not a hash collision or a heuristic symmetry reduction. It
halves the existing 4,924,721,490-state post-opening domain and puts every ID
below `u32::MAX`.

Rank the subset of pieces on the board, their injection into distinct squares,
and the legal pawn-direction cases with precomputed bucket offsets. `unrank`
must be the exact inverse. Avoid Zobrist hashing as the authoritative index:
even a tiny collision probability is inappropriate for a claimed solve.

The hot path should emit ranked child or predecessor IDs through callbacks or
fixed stack buffers. It should not allocate a `Vec` per position. Once the
baseline works, benchmark incremental rank updates against full reranking;
published planning work reports that avoiding repeated rank/unrank operations
can be material ([Pommerening and Helmert,
2015](https://cdn.aaai.org/ojs/13733/13733-40-17251-1-2-20201228.pdf)).

## Symmetry policy

Player-to-move normalization should be mandatory. Left-right reflection should
initially be optional.

Reflection nearly halves the domain again to about **1.231 billion** positions,
but quotient-graph predecessor generation is subtle. A predecessor may reach
either orientation of a canonical child. The correct implementation must:

1. generate predecessor candidates from both child orientations;
2. canonicalize the candidate parents;
3. deduplicate canonical parent-child arcs;
4. validate each reverse candidate by replaying a legal forward move.

Counting distinct canonical children, rather than move multiplicity, is enough
for W/L/D and distance minimax. The mirror-aware Micro Shogi run found and
removed exactly this class of duplicate-predecessor problem. Reflection will
save memory, but generating from two orientations may cancel much of its
propagation-time benefit. Benchmark it rather than assuming a 2x speedup.

## Move and predecessor kernels

Use a `u16` board occupancy and precompute:

- the ten four-in-line masks;
- knight attacks for each square;
- pawn steps and captures by direction;
- optionally rook and bishop attacks indexed by `(square, occupancy)`.

Two complete `16 * 65,536` sliding-attack tables of `u16` entries occupy about
4 MiB. Compare them with short ray scans; the tables are not automatically
faster if cache pressure dominates.

Generate predecessors as candidates and verify them by applying the proposed
forward move. This is especially important for captures, pawn reversal, and
redeployment resets. Hand-written reverse logic without forward validation is
too easy to make locally plausible but globally incomplete.

The legal outdegree fits in `u8`. A player can have at most 64 placements
(four in-hand pieces times sixteen empty squares); moving pieces only lowers
that combined bound. Assert the bound during exhaustive reduced runs and use a
wider debug counter until it has been verified.

## Working-memory targets

For the 2,462,360,745-state player-to-move-normalized domain:

| Structure | Encoding | Approximate size |
|---|---:|---:|
| value | `u8` | 2.29 GiB |
| remaining-child count | `u8` | 2.29 GiB |
| one bitmap | 1 bit/state | 0.29 GiB |
| value + count + three bitmaps | — | 5.45 GiB |

With left-right quotienting, those figures are approximately halved. A `u16`
distance array over the quotient domain adds about 2.29 GiB. Thus even the
simple representation should fit comfortably in 16–32 GiB; a 48–128 GiB box
provides ample room for frontiers, buffers, and verification.

Do not store every frontier ID in a worst-case `VecDeque<u32>` without first
measuring its peak. Compare:

- a `u32` queue for sparse frontiers;
- current/next dense bitmaps for bounded memory and ordered scans;
- a hybrid that switches representation by density.

During the solve, byte-per-state values may outperform two-bit packed values
because random updates avoid masks, shifts, and shared-word contention. Pack
the final tablebase regardless; pack the live array only if the benchmark says
the saved bandwidth exceeds the extra instructions.

## Parallelism and locality

Initialization is a contiguous rank scan and should parallelize almost
linearly. Propagation is a random-update workload and will saturate memory
latency or bandwidth well before it saturates many CPU cores.

The full-address atomic rehearsal scaled from 1.78 million predecessors/s on
one thread to 20.95 million/s on sixteen threads. That is sufficient to select
atomic byte arrays for the first local production run. Benchmark additional
propagation modes on a closed game only if real frontier contention materially
reduces that scaling:

1. single-threaded reference queue;
2. atomic byte arrays with per-thread frontier buffers;
3. owner-computes, rank-sharded BSP with updates batched by destination shard.

The third option costs more machinery but gives deterministic checkpoints,
NUMA ownership, and a clean external-memory path. It remains the fallback if a
real closed rung exposes contention absent from the uniform rehearsal. On
Linux, also measure prefaulting and huge pages; on multi-socket hosts, allocate
and process arrays by shard owner.

A GPU is a later experiment, not the baseline. Perfect-hash GPU game solving
has precedent ([Edelkamp, Sulewski, and Yücel,
2010](https://doi.org/10.1609/socs.v1i1.18167)), but Tic Tac Chec's variable
predecessor generation and random counter updates may erase the throughput
advantage. Profile CPU kernels first; a GPU is most promising for the
independent initialization or reachability passes.

## Reachability and alternative indexing

Enumerate reachability with the reversible rank and bitmaps before choosing a
final storage layout. Measure both pawn-capture interpretations.

A minimal perfect hash over reachable keys is attractive for the published
probe table, but not automatically for solving: typical minimal perfect hashes
are not reversible, so the solver would also have to retain a packed key for
every reachable ID. That usually loses to a slightly sparse mathematical rank.
Use reachable-only MPH during the solve only if enumeration reveals an
exceptionally low reachable/domain ratio and an end-to-end prototype wins.

Set-based retrograde and BDD-based symbolic ranking are also research leads,
not current recommendations. Their largest reported gains rely on strong
domain structure or acyclic layers not obviously present here ([Stone,
Sturtevant, and Schaeffer, 2024](https://arxiv.org/abs/2411.09089)).

## Benchmark gates before the full run

Build a ladder of exact closed subgames and record all results in machine-readable
run directories. At minimum measure:

- rank/unrank nanoseconds and verified round trips;
- successor and predecessor nanoseconds per edge;
- canonicalization and mirror-dedup overhead;
- random update throughput at cache-resident and multi-gigabyte sizes;
- peak frontier density and RSS;
- single-thread, atomic-parallel, and BSP scaling;
- raw versus reflected domain wall time;
- `u8` versus two-bit live values;
- W/L/D counts, maximum distance, and a full minimax audit.

Recommended gates:

1. exhaustive tiny game, checked against an independent reference solver;
2. million-state rung, checked by two retrograde implementations;
3. 50–200 million-state rung, used for architecture selection;
4. billion-state rehearsal with checkpoint/restart and corruption injection;
5. canonical full solve, independent audit, then the alternate-pawn sensitivity
   solve or targeted comparison.

The final audit should scan every included legal state and verify that its value
equals the minimax of its children. It is an independent computation path, not
merely a checksum of the generated file.

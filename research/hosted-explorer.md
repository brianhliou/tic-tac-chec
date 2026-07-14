# Hosted tablebase explorer boundary

Status: implemented, published, and deployed at
[`tic-tac-chec.brianhliou.com`](https://tic-tac-chec.brianhliou.com/).
The custom domain is ownership-verified and serves a valid Railway-managed TLS
certificate.

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

The Rust service loads and validates `post-opening-travel.ttb` once at startup.
This draw-aware derivative stores a decisive-position bitmap, six-bit distances
for decisive positions only, and a prefix-rank directory. It is 485,862,535
bytes (463 MiB) and remains fully memory-resident, avoiding random disk reads
without paying for the 2.48 GB source representation.

The probe endpoint accepts either validated move history or a fully specified
engine position and returns JSON equivalent to the library's `ProbeResult`:

- canonical phase and ID;
- current value and optional distance;
- legal action and child key;
- mover-relative value and optional distance after each action;
- `preserves_result` and `optimal` flags.

Moves are generated on demand. The service never stores or downloads the
28,730,418,180-edge graph. At deployment, startup must verify rules tag
`0x54544303`, internal CRC-64/XZ `0xbe44f17a62ec33e1`, dimensions, rank index,
encoding, and published SHA-256
`f80c1899e57941a2251ffa554645ad06e66d4e5fbd349b6d2b949efd2c526c53`
before becoming healthy. The compact artifact was compared entry-for-entry
against all 2,476,597,610 codes in the source table.

The origin supports persistent HTTP/1.1 connections for up to 100 requests or
10 seconds idle. This avoids repeating connection setup during a sequence of
probes while preserving a fixed worker pool and bounded connection lifetime.

## Deployment package

The root `Dockerfile` builds only `tablebase_server`, downloads the immutable
`tablebase-v1` release asset with Docker's SHA-256 check, and copies both into a
minimal Debian runtime. The service runs as a non-root user. `.dockerignore`
keeps all local solve artifacts out of the build context; the verified local
build sent 501.8 KB rather than the multiple gigabytes present in ignored run
directories.

The production image was built and exercised locally on 2026-07-14:

- Docker-reported image size: **178,030,680 bytes**;
- steady container memory after a probe: **465.4 MiB**;
- health response: HTTP 200 in **0.003057 seconds**;
- empty-board probe: HTTP 200 in **0.003676 seconds**, 8,072 response bytes;
- empty-board result: draw, 64 legal moves, all 64 drawing.

These are single-machine smoke measurements, not latency distributions. A 1 GB
Railway memory limit leaves startup and request headroom while keeping the
expected steady footprint near the paid Hobby minimum.

The same image was deployed to Railway on 2026-07-14. The service validated and
loaded the compact tablebase in about 0.65 seconds. Its first production
measurements reported **499.5 MB** resident memory, **27 ms** server-side HTTP
p50, and no HTTP errors. End-to-end requests from the development machine took
about 215 ms, including TLS and internet transit. Railway currently reports a
24 GB platform ceiling; a smaller account-level safety cap remains optional,
not a requirement for keeping the tablebase resident.

## Frontend contract

The frontend owns presentation and move-history navigation; it does not
reimplement legality, ranking, W/L/D inversion, or optimality. The API response
is the authority. The board should visibly mark pawn travel direction after a
reversal, because that direction affects future captures under the canonical
rules.

Probe responses use a 512-position in-memory browser cache with in-flight
request deduplication. Hovered moves and the first four ranked continuations
are prefetched with at most two concurrent requests and eight queued requests.
Uncached navigation retains the previous position and only shows `Probing...`
after 180 ms, so fast responses do not flash a loading state. A generation
guard prevents an older response from replacing a newer navigation result.

## Delivery sequence

1. Completed: stable JSON responses and checked move-history replay.
2. Completed: long-lived HTTP service with one validated compact artifact load.
3. Completed: board, hands, dragging, ranked moves, history navigation, move
   previews, terminal presentation, and write-up.
4. Completed locally: compact publication artifact generated and exhaustively
   compared with the audited source table.
5. Completed: the compact artifact and checksum are published in the public
   `tablebase-v1` GitHub release.
6. Completed locally: browser/API behavior checked against engine fixtures for
   opening replay, absolute orientation, decisive distance, and terminal play.
7. Completed: Railway build, startup validation, public health check, live
   empty-board probe, and production resource measurement.
8. Completed: custom-domain CNAME and ownership TXT propagation, Railway
   verification, TLS issuance, and public HTTPS health check.

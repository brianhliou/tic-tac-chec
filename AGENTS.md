# tic-tac-chec

This repository is a research-grade strong-solve project for Tic Tac Chec.
The canonical target is the 1998 Dream Green ruleset documented in
`research/rules.md`; do not silently substitute the materially different 2025
Bobby Fischer edition.

## Layout

- `research/` — sourced rules, prior art, state-space derivations, run records,
  and unresolved questions.
- `solver/` — Rust rules engine, enumeration tools, rank/unrank code, and
  retrograde solver.

## Correctness rules

- Treat rules ambiguities as named variants, not informal assumptions.
- Every published count must have a committed reproduction command or test.
- Keep terminal detection, move generation, rank/unrank, and predecessor
  generation independently testable.
- Validate the production move generator against an independent reference
  implementation or exhaustive property tests before a full solve.
- A loopy forward minimax result is not accepted as a solution. Use graph-safe
  retrograde analysis and audit the resulting fixpoint.
- Large tablebases and raw run artifacts stay out of Git; commit checksums and
  compact run summaries.

## Commands

```sh
cargo test --manifest-path solver/Cargo.toml
cargo run --manifest-path solver/Cargo.toml --bin state-space
```

## Handoff

Before edits, run `git status --short --branch`. At handoff, report changed
files, verification, current rules variant, and any unresolved correctness
questions.

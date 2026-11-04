# CLAUDE.md

Instructions for Claude Code sessions working in this repository.

## What this is

The ASI Engine is an open-source Rust engine for branching, plot-heavy terminal
games. It is a Cargo workspace with its crates under `crates/`. It is built first
for ASI, a closed-source game in a separate repository that depends on this one.

**`docs/spec.md` and `docs/decisions/` are authoritative.** Read the relevant
spec sections before changing code. If code and spec disagree, or the spec is
ambiguous about something you need, stop and say so in the PR description
instead of guessing. Don't silently "fix" either side.

## Hard rules

- **No game-specific code in the engine.** Nothing specific to ASI (its names,
  its content, mechanics only ASI uses) belongs here. The engine never imports
  game code.
- **No floats in game logic.** Use integers and the `time` and `duration`
  values. `clippy::float_arithmetic` enforces this.
- **Ordered containers only.** Use `BTreeMap`/`BTreeSet`/`IndexMap`, never
  `HashMap`/`HashSet`, so iteration order never varies between runs.
  `clippy.toml` enforces this.
- **No wall clock in game logic.** Game logic uses game time from state only.
  Presentation may use real time, because it never feeds back into state.
  `clippy.toml` disallows `SystemTime::now` and `Instant::now`. If presentation
  code genuinely needs them, add a local `#[allow]` with a comment saying why.
- **All randomness comes from the seeded RNG in state**, through named
  sub-streams. No `thread_rng` and no OS entropy in game logic.
- **Release builds never panic on bad content or state.** Report the problem,
  clamp to a valid value, and keep at least one action available. No `unwrap`,
  `expect` or `panic!` outside tests. Return errors instead.
- **Content errors carry file and line.**
- **No `unsafe`.** The workspace sets `unsafe_code = "forbid"`.

## Public repository

This repository is public. Never put anything from the local machine into
files, commit messages or PR descriptions: no absolute paths, usernames,
hostnames, environment variables, emails, tokens, tool output that contains any
of these, or logs. Refer to files by repo-relative paths only (e.g.
`crates/asi_core/src/state.rs`).

## Scope rules

- Implement only the slice you were asked for. Note follow-ups in the PR
  instead of doing them.
- Never edit `docs/spec.md` in a PR. It is a synced copy of an external live
  doc. If the spec needs to change, propose the change in the PR description.
- Keep public APIs minimal and document every public item.
- No new dependency without a one-line justification in the PR description.
  Add it to `[workspace.dependencies]` in the root `Cargo.toml`, and have crates
  use `dep.workspace = true`. Its license must pass `cargo deny check`.
- If you make a design decision the spec doesn't settle, write an ADR in
  `docs/decisions/` (copy `000-template.md`).
- Every crate's `Cargo.toml` has `[lints] workspace = true`.

## Before opening a PR

All of these must pass. CI runs them with warnings as errors.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

CI also runs `cargo deny check`. Fill in `.github/pull_request_template.md`.

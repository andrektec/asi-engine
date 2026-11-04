# Contributing

Thanks for helping build the ASI Engine. The project is pre-alpha, so expect
APIs to change.

- **One slice per PR.** Work is planned in slices (see the milestones in
  `docs/spec.md`). Keep each PR to the one slice it implements.
- **CI must pass.** Run the commands in the README's "Building and testing"
  section before pushing. CI treats warnings as errors.
- **Decisions go in ADRs.** If your PR makes a design decision the spec doesn't
  settle, add a record to [`docs/decisions/`](docs/decisions/README.md).
- **Dependencies need a reason.** Justify each new dependency in one line in the
  PR description. Its license must pass `cargo deny check`.
- **Follow the spec.** `docs/spec.md` and `docs/decisions/` are authoritative.
  If code and spec disagree, raise it in the PR rather than picking one.

By contributing, you agree that your contributions are dual licensed under
MIT OR Apache-2.0, as described in the README.

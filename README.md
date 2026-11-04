# ASI Engine

The ASI Engine is an open-source Rust engine for branching, plot-heavy terminal
games. It provides a deterministic core (typed state, a story graph, a game-time
clock and events, seeded randomness), a declarative expression language for
conditions and effects that tools can analyze, and loose content files that are
easy to mod. Optional modules add features such as logic grids, travel and skill
checks. The engine is built first for the game ASI and can be used for any other
game. See [`docs/spec.md`](docs/spec.md) for the technical spec.

**Status:** pre-alpha. The API is unstable and will change without notice.

## Building and testing

You need [rustup](https://rustup.rs). It installs the toolchain pinned in
`rust-toolchain.toml` automatically.

```sh
cargo build --workspace
cargo test --workspace
cargo run -p asi_player      # prints the engine version
```

Before opening a PR, also run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

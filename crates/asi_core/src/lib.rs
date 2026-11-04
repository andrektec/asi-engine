//! Core of the ASI Engine: everything every game needs.
//!
//! This crate will hold the single typed state store (namespaced per module),
//! the story graph (scenes, nodes, choices, endings), game-time clock and
//! scheduled events, the seeded RNG with named sub-streams, the module
//! registry, and actions. It has no terminal or OS dependency, so the
//! analyzer, tests and bots can run it headless.
//!
//! See `docs/spec.md`, section "Core". Pre-alpha: no API stability.

/// The engine version, as recorded in save headers and printed by tools.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_version_is_set() {
        assert!(!ENGINE_VERSION.is_empty());
    }
}

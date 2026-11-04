//! Saves and replays for the ASI Engine.
//!
//! This crate will hold the save format (human-readable JSON with a header of
//! engine version, game ID and version, save-format version and enabled
//! modules), the body checksum, versioned migrations, and replays (a starting
//! save plus every action with its game-time stamp).
//!
//! See `docs/spec.md`, section "Saves, determinism and replays". Pre-alpha: no
//! API stability.

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {
        assert_eq!(env!("CARGO_PKG_NAME"), "asi_save");
    }
}

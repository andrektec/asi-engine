//! Content loading for the ASI Engine.
//!
//! This crate will hold the virtual file system (base game, then mods in
//! declared order, later mounts adding or replacing content by ID), YAML
//! loading into the canonical data model, load-time validation (schema, IDs,
//! references, expression types; errors carry file and line), and JSON Schema
//! generation from the Rust types.
//!
//! See `docs/spec.md`, section "Content format". Pre-alpha: no API stability.

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {
        assert_eq!(env!("CARGO_PKG_NAME"), "asi_content");
    }
}

//! Expression and effect language of the ASI Engine.
//!
//! This crate will hold the parser, type checker and evaluator for one small,
//! typed, side-effect-free condition language, plus the separate effect syntax
//! for state changes. Both are parsed and type-checked at load, so a typo in
//! content is a load error, not a runtime bug. Expressions have no loops,
//! user-defined functions or variables: every expression terminates.
//!
//! See `docs/spec.md`, section "Expression language". Pre-alpha: no API stability.

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {
        assert_eq!(env!("CARGO_PKG_NAME"), "asi_expr");
    }
}

//! Core of the ASI Engine: everything every game needs.
//!
//! This crate will hold the single typed state store (namespaced per module),
//! the story graph (scenes, nodes, choices, endings), game-time clock and
//! scheduled events, the seeded RNG with named sub-streams, the module
//! registry, and actions. It has no terminal or OS dependency, so the
//! analyzer, tests and bots can run it headless.
//!
//! So far it provides the value model and state store (ADR-001):
//!
//! - [`Time`] and [`Duration`]: whole minutes, checked arithmetic, literal
//!   parsing ([`time`] module docs list the formats).
//! - [`Symbol`], [`IdKind`] and [`Id`]: validated identifiers ([`ident`]
//!   module docs list the allowed characters).
//! - [`Type`] and [`Value`]: `bool`, `int`, `enum`, `set<kind>`, `time`,
//!   `duration`. No strings, no floats.
//! - [`KindRegistry`], [`StateSchema`] and [`FrozenSchema`]: declared ID
//!   kinds, namespaces and keys.
//! - [`State`]: the typed store, created from a frozen schema.
//!
//! All fallible operations return [`CoreError`]; nothing here panics.
//!
//! ```
//! use asi_core::{IdKind, KindRegistry, Owner, State, StateSchema, Type, Value};
//!
//! # fn main() -> asi_core::Result<()> {
//! let mut kinds = KindRegistry::new();
//! kinds.register(IdKind::new("clue")?, Owner::game())?;
//!
//! let mut schema = StateSchema::new(kinds);
//! schema.add_namespace("travel", Owner::module("travel")?)?;
//! schema.add_namespace("case", Owner::game())?;
//! let modes = [":walk".parse()?, ":bicycle".parse()?].into();
//! schema.add_key("travel.mode".parse()?, Type::Enum(modes), Value::Enum(":walk".parse()?))?;
//! schema.add_key(
//!     "case.clues".parse()?,
//!     Type::Set(IdKind::new("clue")?),
//!     Value::Set(Default::default()),
//! )?;
//!
//! let mut state = State::new(&schema.freeze());
//! state.set(&"travel.mode".parse()?, Value::Enum(":bicycle".parse()?))?;
//! state.insert(&"case.clues".parse()?, "clue.receipt_photo".parse()?)?;
//! assert!(state.contains(&"case.clues".parse()?, &"clue.receipt_photo".parse()?)?);
//!
//! // Wrong kinds and symbols outside the enum are rejected.
//! assert!(state.insert(&"case.clues".parse()?, "npc.carla".parse()?).is_err());
//! assert!(state.set(&"travel.mode".parse()?, Value::Enum(":car".parse()?)).is_err());
//! # Ok(())
//! # }
//! ```
//!
//! See `docs/spec.md`, section "Core". Pre-alpha: no API stability.

#![warn(missing_docs)]

mod error;
pub mod ident;
mod schema;
mod state;
pub mod time;
mod value;

pub use error::{CoreError, Result};
pub use ident::{Id, IdKind, Symbol};
pub use schema::{FrozenSchema, Key, KindRegistry, Owner, StateSchema};
pub use state::State;
pub use time::{Duration, Time};
pub use value::{Type, Value};

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

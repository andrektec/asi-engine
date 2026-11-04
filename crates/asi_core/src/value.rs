//! State value types and values (spec "Core › State", ADR-001 §1).
//!
//! There are no string or float values.

use std::collections::BTreeSet;
use std::fmt;

use crate::ident::{Id, IdKind, Symbol};
use crate::time::{Duration, Time};

/// The type of a state key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    /// `true` or `false`.
    Bool,
    /// A signed 64-bit integer.
    Int,
    /// One symbol from a closed set, e.g. `{:walk, :bicycle}`.
    Enum(BTreeSet<Symbol>),
    /// A set of IDs, all of the given kind (`set<clue>`).
    Set(IdKind),
    /// A point in game time.
    Time,
    /// A span of game time.
    Duration,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool => f.write_str("bool"),
            Self::Int => f.write_str("int"),
            Self::Enum(symbols) => {
                f.write_str("enum{")?;
                write_list(f, symbols)?;
                f.write_str("}")
            }
            Self::Set(kind) => write!(f, "set<{kind}>"),
            Self::Time => f.write_str("time"),
            Self::Duration => f.write_str("duration"),
        }
    }
}

/// A state value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Value {
    /// A [`Type::Bool`] value.
    Bool(bool),
    /// A [`Type::Int`] value.
    Int(i64),
    /// A [`Type::Enum`] value.
    Enum(Symbol),
    /// A [`Type::Set`] value. Ordered, so iteration never varies between runs.
    Set(BTreeSet<Id>),
    /// A [`Type::Time`] value.
    Time(Time),
    /// A [`Type::Duration`] value.
    Duration(Duration),
}

impl Value {
    /// Whether this value is valid for `ty`: same variant, an enum symbol in
    /// the allowed set, and every set member of the set's kind. An empty set
    /// matches every set type.
    pub fn type_matches(&self, ty: &Type) -> bool {
        match (self, ty) {
            (Self::Bool(_), Type::Bool)
            | (Self::Int(_), Type::Int)
            | (Self::Time(_), Type::Time)
            | (Self::Duration(_), Type::Duration) => true,
            (Self::Enum(symbol), Type::Enum(allowed)) => allowed.contains(symbol),
            (Self::Set(members), Type::Set(kind)) => members.iter().all(|id| id.kind() == kind),
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Enum(symbol) => write!(f, "{symbol}"),
            Self::Set(members) => {
                f.write_str("{")?;
                write_list(f, members)?;
                f.write_str("}")
            }
            Self::Time(time) => write!(f, "{time}"),
            Self::Duration(duration) => write!(f, "{duration}"),
        }
    }
}

/// Writes items separated by `, `.
fn write_list<T: fmt::Display>(
    f: &mut fmt::Formatter<'_>,
    items: impl IntoIterator<Item = T>,
) -> fmt::Result {
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{item}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(s: &str) -> Symbol {
        Symbol::new(s).unwrap()
    }

    fn kind(s: &str) -> IdKind {
        IdKind::new(s).unwrap()
    }

    fn id(s: &str) -> Id {
        s.parse().unwrap()
    }

    fn mode() -> Type {
        Type::Enum([sym("walk"), sym("bicycle")].into())
    }

    #[test]
    fn scalars_match_their_own_type_only() {
        let values = [
            Value::Bool(true),
            Value::Int(-3),
            Value::Time(Time::START),
            Value::Duration(Duration::ZERO),
        ];
        let types = [Type::Bool, Type::Int, Type::Time, Type::Duration];
        for (vi, value) in values.iter().enumerate() {
            for (ti, ty) in types.iter().enumerate() {
                assert_eq!(value.type_matches(ty), vi == ti, "{value} vs {ty}");
            }
            assert!(!value.type_matches(&mode()));
            assert!(!value.type_matches(&Type::Set(kind("clue"))));
        }
    }

    #[test]
    fn enum_values_must_be_in_the_type() {
        assert!(Value::Enum(sym("bicycle")).type_matches(&mode()));
        assert!(!Value::Enum(sym("car")).type_matches(&mode()));
        assert!(!Value::Enum(sym("walk")).type_matches(&Type::Enum(BTreeSet::new())));
        assert!(!Value::Enum(sym("walk")).type_matches(&Type::Bool));
    }

    #[test]
    fn set_members_must_have_the_set_kind() {
        let clues = Type::Set(kind("clue"));
        assert!(Value::Set(BTreeSet::new()).type_matches(&clues));
        assert!(Value::Set([id("clue.a"), id("clue.b.c")].into()).type_matches(&clues));
        assert!(!Value::Set([id("clue.a"), id("npc.a")].into()).type_matches(&clues));
        assert!(!Value::Set([id("npc.a")].into()).type_matches(&clues));
        assert!(!Value::Set(BTreeSet::new()).type_matches(&Type::Int));
    }

    #[test]
    fn display() {
        assert_eq!(mode().to_string(), "enum{:bicycle, :walk}");
        assert_eq!(Type::Set(kind("clue")).to_string(), "set<clue>");
        assert_eq!(
            Value::Set([id("clue.b"), id("clue.a")].into()).to_string(),
            "{clue.a, clue.b}"
        );
        assert_eq!(Value::Enum(sym("walk")).to_string(), ":walk");
        assert_eq!(
            Value::Time("d3 22:00".parse().unwrap()).to_string(),
            "d3 22:00"
        );
        assert_eq!(Value::Duration("90m".parse().unwrap()).to_string(), "1h30m");
    }
}

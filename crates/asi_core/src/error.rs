//! The single error type of this crate.

use std::fmt;

use crate::ident::{Id, IdKind};
use crate::schema::{Key, Owner};
use crate::value::{Type, Value};

/// Result alias using [`CoreError`].
pub type Result<T, E = CoreError> = std::result::Result<T, E>;

/// Everything that can go wrong in `asi_core`.
///
/// Core errors don't know which content file they came from; the content
/// loader wraps them with file and line.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoreError {
    /// A symbol, ID, kind, key, namespace or module name broke the identifier
    /// rules (see [`crate::ident`]).
    InvalidName {
        /// What was being named, e.g. `"symbol"` or `"state key"`.
        what: &'static str,
        /// The rejected text.
        text: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// A time literal or day/hour/minute triple was invalid or out of range.
    InvalidTime {
        /// The rejected input.
        text: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// A duration literal was invalid or out of range.
    InvalidDuration {
        /// The rejected input.
        text: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// An ID kind was registered twice.
    DuplicateKind {
        /// The kind.
        kind: IdKind,
        /// Who registered it first.
        owner: Owner,
    },
    /// A state namespace was declared twice.
    DuplicateNamespace {
        /// The namespace.
        namespace: String,
        /// Who declared it first.
        owner: Owner,
    },
    /// A key was declared in a namespace that was never declared.
    UnknownNamespace {
        /// The key being declared.
        key: Key,
    },
    /// A state key was declared twice.
    DuplicateKey {
        /// The key.
        key: Key,
    },
    /// A `set<kind>` type names a kind that isn't registered.
    UnknownKind {
        /// The key being declared.
        key: Key,
        /// The unregistered kind.
        kind: IdKind,
    },
    /// A key's declared default doesn't match its declared type.
    BadDefault {
        /// The key being declared.
        key: Key,
        /// The declared type.
        expected: Type,
        /// The rejected default.
        found: Value,
    },
    /// A key that isn't in the schema was read or written.
    UnknownKey {
        /// The key.
        key: Key,
    },
    /// A value of the wrong type was written to a key.
    TypeMismatch {
        /// The key.
        key: Key,
        /// The key's type.
        expected: Type,
        /// The rejected value.
        found: Value,
    },
    /// A set operation was used on a key that isn't a set.
    NotASet {
        /// The key.
        key: Key,
        /// The key's actual type.
        found: Type,
    },
    /// A set operation used an ID of a different kind than the set holds.
    WrongIdKind {
        /// The set key.
        key: Key,
        /// The kind the set holds.
        expected: IdKind,
        /// The rejected ID.
        id: Id,
    },
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName { what, text, reason } => {
                write!(f, "invalid {what} `{text}`: {reason}")
            }
            Self::InvalidTime { text, reason } => write!(f, "invalid time `{text}`: {reason}"),
            Self::InvalidDuration { text, reason } => {
                write!(f, "invalid duration `{text}`: {reason}")
            }
            Self::DuplicateKind { kind, owner } => {
                write!(f, "ID kind `{kind}` is already registered by {owner}")
            }
            Self::DuplicateNamespace { namespace, owner } => {
                write!(f, "namespace `{namespace}` is already declared by {owner}")
            }
            Self::UnknownNamespace { key } => write!(
                f,
                "key `{key}` is in namespace `{}`, which is not declared",
                key.namespace()
            ),
            Self::DuplicateKey { key } => write!(f, "state key `{key}` is already declared"),
            Self::UnknownKind { key, kind } => write!(
                f,
                "key `{key}` has type `set<{kind}>`, but ID kind `{kind}` is not registered"
            ),
            Self::BadDefault {
                key,
                expected,
                found,
            } => write!(
                f,
                "default `{found}` for key `{key}` does not match its type `{expected}`"
            ),
            Self::UnknownKey { key } => write!(f, "unknown state key `{key}`"),
            Self::TypeMismatch {
                key,
                expected,
                found,
            } => write!(
                f,
                "cannot store `{found}` in key `{key}`: expected a value of type `{expected}`"
            ),
            Self::NotASet { key, found } => write!(
                f,
                "key `{key}` has type `{found}`, so set operations don't apply to it"
            ),
            Self::WrongIdKind { key, expected, id } => write!(
                f,
                "key `{key}` holds `set<{expected}>`, but `{id}` is of kind `{}`",
                id.kind()
            ),
        }
    }
}

impl std::error::Error for CoreError {}

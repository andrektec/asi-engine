//! Identifiers: enum symbols, ID kinds and IDs.
//!
//! # Identifier rules
//!
//! Every name in the engine is built from *segments*. A segment is non-empty,
//! starts with an ASCII lowercase letter (`a`–`z`) and continues with ASCII
//! lowercase letters, ASCII digits (`0`–`9`) or underscores (`_`). That is,
//! `[a-z][a-z0-9_]*`. Uppercase, hyphens and non-ASCII characters are rejected
//! so names are unambiguous in content, file names and saves.
//!
//! A *dotted path* is one or more segments joined by single dots
//! (`case1.diner.arrive`).
//!
//! - [`Symbol`]: one segment (`bicycle`, written `:bicycle` in content).
//! - [`IdKind`]: one segment (`clue`).
//! - [`Id`]: a kind plus a dotted path (`clue.receipt_photo`).

use std::fmt;
use std::str::FromStr;

use crate::error::{CoreError, Result};

/// Checks one segment; returns the reason it is invalid.
fn segment_error(text: &str) -> Option<&'static str> {
    let mut chars = text.chars();
    match chars.next() {
        None => Some("must not be empty"),
        Some(c) if !c.is_ascii_lowercase() => Some("must start with a lowercase ASCII letter"),
        Some(_) => {
            if chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                None
            } else {
                Some("may only contain lowercase ASCII letters, digits and `_`")
            }
        }
    }
}

/// Validates one segment, reporting errors as `what`.
pub(crate) fn check_segment(what: &'static str, text: &str) -> Result<()> {
    match segment_error(text) {
        None => Ok(()),
        Some(reason) => Err(CoreError::InvalidName {
            what,
            text: text.to_owned(),
            reason,
        }),
    }
}

/// Validates a dotted path of one or more segments, reporting errors as `what`.
pub(crate) fn check_path(what: &'static str, text: &str) -> Result<()> {
    for segment in text.split('.') {
        if let Some(reason) = segment_error(segment) {
            return Err(CoreError::InvalidName {
                what,
                text: text.to_owned(),
                reason: if segment.is_empty() {
                    "has an empty segment (check for stray dots)"
                } else {
                    reason
                },
            });
        }
    }
    Ok(())
}

/// A value of an enum type, such as `:bicycle`.
///
/// Stored without the colon. One segment (see the [module docs](self)).
/// [`Display`](fmt::Display) and [`FromStr`] use the content form with the
/// colon, so they round-trip.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(String);

impl Symbol {
    /// Creates a symbol from its bare name (no colon), e.g. `"bicycle"`.
    pub fn new(name: &str) -> Result<Self> {
        check_segment("symbol", name)?;
        Ok(Self(name.to_owned()))
    }

    /// The bare name, without the colon.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ":{}", self.0)
    }
}

impl FromStr for Symbol {
    type Err = CoreError;

    /// Parses the content form, `:name`.
    fn from_str(s: &str) -> Result<Self> {
        match s.strip_prefix(':') {
            Some(name) => Self::new(name),
            None => Err(CoreError::InvalidName {
                what: "symbol",
                text: s.to_owned(),
                reason: "must start with `:`",
            }),
        }
    }
}

/// The kind of an [`Id`], given by its first segment (`clue`, `flag`, `npc`).
///
/// One segment (see the [module docs](self)). Kinds are registered in a
/// [`KindRegistry`](crate::KindRegistry).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdKind(String);

impl IdKind {
    /// Creates a kind from its name, e.g. `"clue"`.
    pub fn new(name: &str) -> Result<Self> {
        check_segment("ID kind", name)?;
        Ok(Self(name.to_owned()))
    }

    /// The kind's name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for IdKind {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

/// A typed content ID, such as `clue.receipt_photo`: a kind plus a dotted path.
///
/// Validated at construction (see the [module docs](self)); whether any
/// content defines it is checked by the content loader, not here.
/// Ordering is by kind, then path, compared bytewise, so it never varies
/// between runs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id {
    kind: IdKind,
    path: String,
}

impl Id {
    /// Creates an ID from a kind and the dotted path after it.
    ///
    /// `Id::new(clue, "receipt_photo")` is `clue.receipt_photo`.
    pub fn new(kind: IdKind, path: &str) -> Result<Self> {
        check_path("ID", path)?;
        Ok(Self {
            kind,
            path: path.to_owned(),
        })
    }

    /// The ID's kind.
    pub fn kind(&self) -> &IdKind {
        &self.kind
    }

    /// The dotted path after the kind.
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.kind, self.path)
    }
}

impl FromStr for Id {
    type Err = CoreError;

    /// Parses `kind.path`, e.g. `clue.receipt_photo`.
    fn from_str(s: &str) -> Result<Self> {
        check_path("ID", s)?;
        match s.split_once('.') {
            Some((kind, path)) => Self::new(IdKind::new(kind)?, path),
            None => Err(CoreError::InvalidName {
                what: "ID",
                text: s.to_owned(),
                reason: "must be a kind followed by a dotted path, e.g. `clue.receipt_photo`",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_accept_valid_names() {
        for ok in ["a", "bicycle", "case1", "met_marcos", "a_1_b", "x__"] {
            assert!(segment_error(ok).is_none(), "{ok}");
        }
    }

    #[test]
    fn segments_reject_invalid_names() {
        for bad in [
            "", "1a", "_a", "Bicycle", "biCycle", "a-b", "a b", "a.b", "ção", ":a",
        ] {
            assert!(segment_error(bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn symbol_round_trips_with_colon() {
        let s: Symbol = ":bicycle".parse().unwrap();
        assert_eq!(s.as_str(), "bicycle");
        assert_eq!(s.to_string(), ":bicycle");
        assert_eq!(s, Symbol::new("bicycle").unwrap());
    }

    #[test]
    fn symbol_rejects_bad_input() {
        assert!("bicycle".parse::<Symbol>().is_err());
        assert!(":".parse::<Symbol>().is_err());
        assert!(":Bike".parse::<Symbol>().is_err());
        assert!(Symbol::new(":bicycle").is_err());
        assert!(Symbol::new("on.foot").is_err());
    }

    #[test]
    fn kind_validation() {
        assert_eq!(IdKind::new("clue").unwrap().as_str(), "clue");
        assert!(IdKind::new("clue.photo").is_err());
        assert!(IdKind::new("").is_err());
        assert!(IdKind::new("Clue").is_err());
    }

    #[test]
    fn id_round_trips() {
        for text in ["clue.receipt_photo", "scene.case1.diner.arrive", "npc.a1"] {
            let id: Id = text.parse().unwrap();
            assert_eq!(id.to_string(), text);
        }
        let id: Id = "scene.case1.diner".parse().unwrap();
        assert_eq!(id.kind().as_str(), "scene");
        assert_eq!(id.path(), "case1.diner");
        assert_eq!(
            id,
            Id::new(IdKind::new("scene").unwrap(), "case1.diner").unwrap()
        );
    }

    #[test]
    fn id_rejects_bad_input() {
        for bad in [
            "clue",
            "clue.",
            ".clue",
            "clue..photo",
            "clue.Photo",
            "clue.photo-1",
            "",
        ] {
            assert!(bad.parse::<Id>().is_err(), "{bad}");
        }
        let clue = IdKind::new("clue").unwrap();
        assert!(Id::new(clue.clone(), "").is_err());
        assert!(Id::new(clue, "a..b").is_err());
    }

    #[test]
    fn id_ordering_is_kind_then_path() {
        let mut ids: Vec<Id> = ["npc.a", "clue.b", "clue.a.z", "clue.a"]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect();
        ids.sort();
        let sorted: Vec<String> = ids.iter().map(ToString::to_string).collect();
        assert_eq!(sorted, ["clue.a", "clue.a.z", "clue.b", "npc.a"]);
    }
}

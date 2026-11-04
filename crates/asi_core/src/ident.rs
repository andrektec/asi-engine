//! Identifiers: enum symbols, ID kinds and IDs.
//!
//! # Identifier rules
//!
//! Every name in the engine is built from *segments* of ASCII lowercase
//! letters (`a`–`z`), ASCII digits (`0`–`9`) and underscores (`_`). There are
//! two shapes:
//!
//! - A *name segment* starts with a letter: `[a-z][a-z0-9_]*`. Symbols, ID
//!   kinds, namespaces and module names are one name segment.
//! - A *path segment* starts with a letter or digit: `[a-z0-9][a-z0-9_]*`.
//!
//! A *dotted path* is one or more path segments joined by single dots
//! (`case1.carla.042`). An [`Id`] is a kind followed by a dotted path, and a
//! state key is a namespace followed by a dotted path, so their first segment
//! is always letter-first and later ones may start with a digit
//! (`line.case1.carla.042`).
//!
//! Uppercase, hyphens and non-ASCII characters are rejected so names are
//! unambiguous in content, file names and saves.
//!
//! - [`Symbol`]: one name segment (`bicycle`, written `:bicycle` in content).
//! - [`IdKind`]: one name segment (`clue`).
//! - [`Id`]: a kind plus a dotted path (`clue.receipt_photo`).
//!
//! [`name_segment_error`] and [`path_segment_error`] check single segments,
//! for other crates (such as the expression parser) that read names out of
//! larger text and must apply the same rules.

use std::fmt;
use std::str::FromStr;

use crate::error::{CoreError, Result};

/// Whether `c` may appear after the first character of any segment.
fn is_tail_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
}

/// Checks a name segment (`[a-z][a-z0-9_]*`).
///
/// Returns `None` if `text` is a valid name segment, or a short reason (for
/// error messages) if it isn't.
pub fn name_segment_error(text: &str) -> Option<&'static str> {
    let mut chars = text.chars();
    match chars.next() {
        None => Some("must not be empty"),
        Some(c) if !c.is_ascii_lowercase() => Some("must start with a lowercase ASCII letter"),
        Some(_) if !chars.all(is_tail_char) => {
            Some("may only contain lowercase ASCII letters, digits and `_`")
        }
        Some(_) => None,
    }
}

/// Checks a path segment (`[a-z0-9][a-z0-9_]*`).
///
/// Returns `None` if `text` is a valid path segment, or a short reason (for
/// error messages) if it isn't. Dots are not allowed: pass one segment at a
/// time.
pub fn path_segment_error(text: &str) -> Option<&'static str> {
    let mut chars = text.chars();
    match chars.next() {
        None => Some("has an empty segment (check for stray dots)"),
        Some(c) if !(c.is_ascii_lowercase() || c.is_ascii_digit()) => {
            Some("each segment must start with a lowercase ASCII letter or digit")
        }
        Some(_) if !chars.all(is_tail_char) => {
            Some("may only contain lowercase ASCII letters, digits, `_` and `.`")
        }
        Some(_) => None,
    }
}

/// Validates one name segment, reporting errors as `what`.
pub(crate) fn check_segment(what: &'static str, text: &str) -> Result<()> {
    match name_segment_error(text) {
        None => Ok(()),
        Some(reason) => Err(CoreError::InvalidName {
            what,
            text: text.to_owned(),
            reason,
        }),
    }
}

/// Validates a dotted path of one or more path segments, reporting errors as
/// `what`.
pub(crate) fn check_path(what: &'static str, text: &str) -> Result<()> {
    match text.split('.').find_map(path_segment_error) {
        None => Ok(()),
        Some(reason) => Err(CoreError::InvalidName {
            what,
            text: text.to_owned(),
            reason,
        }),
    }
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
    fn name_segments() {
        for ok in ["a", "bicycle", "case1", "met_marcos", "a_1_b", "x__"] {
            assert!(name_segment_error(ok).is_none(), "{ok}");
        }
        for bad in [
            "", "1a", "042", "_a", "Bicycle", "biCycle", "a-b", "a b", "a.b", "ção", ":a",
        ] {
            assert!(name_segment_error(bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn path_segments_may_start_with_a_digit() {
        for ok in ["a", "case1", "042", "1a", "0", "2nd_visit"] {
            assert!(path_segment_error(ok).is_none(), "{ok}");
        }
        for bad in ["", "_a", "_1", "A1", "1A", "a-b", "a.b", "ção"] {
            assert!(path_segment_error(bad).is_some(), "{bad}");
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
        assert!(Symbol::new("1st").is_err());
        assert!(":042".parse::<Symbol>().is_err());
    }

    #[test]
    fn kind_validation() {
        assert_eq!(IdKind::new("clue").unwrap().as_str(), "clue");
        assert!(IdKind::new("clue.photo").is_err());
        assert!(IdKind::new("").is_err());
        assert!(IdKind::new("Clue").is_err());
        assert!(IdKind::new("1clue").is_err());
    }

    #[test]
    fn id_round_trips() {
        for text in [
            "clue.receipt_photo",
            "scene.case1.diner.arrive",
            "npc.a1",
            "line.case1.carla.042",
            "line.042",
        ] {
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
            "clue._photo",
            "1clue.photo",
            "042.carla",
            "",
        ] {
            assert!(bad.parse::<Id>().is_err(), "{bad}");
        }
        let clue = IdKind::new("clue").unwrap();
        assert!(Id::new(clue.clone(), "").is_err());
        assert!(Id::new(clue.clone(), "a..b").is_err());
        assert_eq!(Id::new(clue, "042").unwrap().to_string(), "clue.042");
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

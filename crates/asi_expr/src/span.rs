//! Byte spans into expression text, and turning offsets into lines and
//! columns.

/// A byte range `start..end` into the text an expression was parsed from.
///
/// `start <= end`, and both lie on UTF-8 character boundaries of that text.
/// An empty span (`start == end`) marks a position, such as the end of the
/// input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Span {
    /// Offset of the first byte.
    pub start: usize,
    /// Offset one past the last byte.
    pub end: usize,
}

impl Span {
    /// The span `start..end`.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The smallest span covering both `self` and `other`.
    pub fn to(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// A 1-based line and column, as shown to writers in error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineCol {
    /// Line number, starting at 1. Lines are separated by `\n`.
    pub line: usize,
    /// Column, starting at 1, counted in characters (Unicode scalar values),
    /// not bytes.
    pub column: usize,
}

/// The line and column of byte `offset` within `text`.
///
/// Positions are relative to the expression text itself; the content loader
/// adds the file and the expression's position in it. Never panics: an
/// offset past the end is treated as the end, and an offset inside a
/// multi-byte character as the start of that character.
pub fn line_col(text: &str, offset: usize) -> LineCol {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset = offset.saturating_sub(1);
    }
    let before = text.get(..offset).unwrap_or_default();
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    LineCol {
        line: before.matches('\n').count() + 1,
        column: before.get(line_start..).unwrap_or_default().chars().count() + 1,
    }
}

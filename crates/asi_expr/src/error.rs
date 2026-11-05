//! Parse errors.

use std::fmt;

use crate::span::Span;

/// A condition that could not be parsed.
///
/// Carries the span of the offending text, a message, and sometimes a hint
/// on how to fix it. The span is relative to the expression text; use
/// [`line_col`](crate::line_col) to turn it into a line and column. The
/// content loader adds the file and the expression's position in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    span: Span,
    message: String,
    hint: Option<String>,
}

impl ParseError {
    pub(crate) fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            hint: None,
        }
    }

    pub(crate) fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Where in the expression text the problem is.
    pub fn span(&self) -> Span {
        self.span
    }

    /// What is wrong.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// How to fix it, if there is a likely fix.
    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)?;
        if let Some(hint) = &self.hint {
            write!(f, " (hint: {hint})")?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseError {}

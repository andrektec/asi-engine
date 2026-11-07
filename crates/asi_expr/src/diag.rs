//! Load-time diagnostics from the type checker.

use std::fmt;

use crate::error::ParseError;
use crate::span::Span;

/// A problem found while parsing or type-checking a condition.
///
/// Like [`ParseError`], it carries the span of the offending text (relative
/// to the expression text), a message and maybe a hint. The content loader
/// adds the file and the expression's position in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    span: Span,
    message: String,
    hint: Option<String>,
}

impl Diagnostic {
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

    pub(crate) fn maybe_hint(mut self, hint: Option<String>) -> Self {
        self.hint = hint;
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

impl From<ParseError> for Diagnostic {
    fn from(e: ParseError) -> Self {
        Self {
            span: e.span(),
            message: e.message().to_owned(),
            hint: e.hint().map(str::to_owned),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)?;
        if let Some(hint) = &self.hint {
            write!(f, " (hint: {hint})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

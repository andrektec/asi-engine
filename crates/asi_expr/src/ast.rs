//! The syntax tree of a condition.
//!
//! Every node carries the [`Span`] of the text it came from. Parentheses are
//! not nodes: a parenthesized expression is its inner node, with the span
//! widened to cover the parentheses.
//!
//! Chains of the same left-associative operator are stored flat
//! ([`ExprKind::Or`], [`ExprKind::And`], [`ExprKind::Sum`]) rather than as
//! nested binary nodes, so the tree's depth is bounded by the nesting limit
//! no matter how long a chain is (ADR-003). Every later pass can recurse over
//! the tree without risking a stack overflow.

use std::fmt;
use std::str::FromStr;

use asi_core::{Duration, Symbol, Time};

use crate::error::ParseError;
use crate::span::Span;

/// An expression with its span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr {
    /// What the expression is.
    pub kind: ExprKind,
    /// Where it is in the source text.
    pub span: Span,
}

impl Expr {
    /// An expression of `kind` at `span`.
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// The shape of an [`Expr`].
///
/// The parser only builds `And` and `Or` with at least two items, and `Sum`
/// with at least one term in `rest`. It never builds a negative
/// [`Literal::Int`] or [`Literal::Duration`]: a leading `-` is always
/// [`ExprKind::Neg`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprKind {
    /// A literal value: `42`, `true`, `d3 22:00`, `1h30m`, `:bicycle`.
    Literal(Literal),
    /// A dotted path: `flag.met_marcos`, `travel.mode`. Whether it names a
    /// state key, an ID or something else is decided by the type checker.
    Path(Path),
    /// A function call: `has(clue.receipt)`.
    Call {
        /// The function's path.
        callee: Path,
        /// The span of `callee`.
        callee_span: Span,
        /// The arguments, in order. May be empty.
        args: Vec<Expr>,
    },
    /// Unary minus: `-x`.
    Neg(Box<Expr>),
    /// Logical not: `!x`.
    Not(Box<Expr>),
    /// `first` followed by `+`/`-` terms, evaluated left to right:
    /// `a - b + c` is `(a - b) + c`.
    Sum {
        /// The leftmost operand.
        first: Box<Expr>,
        /// The following terms, each with its operator.
        rest: Vec<SumTerm>,
    },
    /// A comparison. Comparisons don't chain, so neither side is a bare
    /// comparison.
    Compare {
        /// The operator.
        op: CmpOp,
        /// The span of the operator.
        op_span: Span,
        /// The left operand.
        lhs: Box<Expr>,
        /// The right operand.
        rhs: Box<Expr>,
    },
    /// Items joined by `&&`, evaluated left to right.
    And(Vec<Expr>),
    /// Items joined by `||`, evaluated left to right.
    Or(Vec<Expr>),
}

/// A literal value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    /// An integer.
    Int(i64),
    /// `true` or `false`.
    Bool(bool),
    /// A point in game time, `dN HH:MM`.
    Time(Time),
    /// A duration such as `15m` or `1h30m`.
    Duration(Duration),
    /// An enum symbol such as `:bicycle`.
    Symbol(Symbol),
}

/// One `+ x` or `- x` term of an [`ExprKind::Sum`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SumTerm {
    /// The operator before the operand.
    pub op: SumOp,
    /// The span of the operator.
    pub op_span: Span,
    /// The operand.
    pub operand: Expr,
}

/// An additive operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SumOp {
    /// `+`
    Add,
    /// `-`
    Sub,
}

impl SumOp {
    /// The operator as written.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
        }
    }
}

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CmpOp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
}

impl CmpOp {
    /// The operator as written.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }
}

/// A dotted path such as `flag.met_marcos` or `line.case1.carla.042`.
///
/// One or more segments joined by single dots: the first is a name segment,
/// later ones are path segments (ADR-002). A path of the single segment
/// `true` or `false` is not allowed, because that text is a boolean literal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path(String);

impl Path {
    /// The path as written, e.g. `"flag.met_marcos"`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The segments, in order.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('.')
    }

    /// Validates `text` as a path. Error spans are offsets into `text` plus
    /// `base`.
    pub(crate) fn validate(text: &str, base: usize) -> Result<Self, ParseError> {
        let mut offset = base;
        let single = !text.contains('.');
        for (i, segment) in text.split('.').enumerate() {
            let span = Span::new(offset, offset + segment.len());
            offset = span.end + 1;
            let problem = if i == 0 {
                asi_core::ident::name_segment_error(segment)
            } else {
                asi_core::ident::path_segment_error(segment)
            };
            if let Some(reason) = problem {
                let message = if single {
                    format!("invalid name `{segment}`: {reason}")
                } else {
                    format!("invalid segment `{segment}` in path `{text}`: {reason}")
                };
                return Err(ParseError::new(span, message));
            }
        }
        if text == "true" || text == "false" {
            return Err(ParseError::new(
                Span::new(base, base + text.len()),
                format!("`{text}` is a boolean literal, not a path"),
            ));
        }
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Path {
    type Err = ParseError;

    /// Parses a path with no surrounding whitespace. Error spans are offsets
    /// into `s`.
    fn from_str(s: &str) -> Result<Self, ParseError> {
        Self::validate(s, 0)
    }
}

//! Expression and effect language of the ASI Engine.
//!
//! This crate will hold the parser, type checker and evaluator for one small,
//! typed, side-effect-free condition language, plus the separate effect syntax
//! for state changes. Both are parsed and type-checked at load, so a typo in
//! content is a load error, not a runtime bug. Expressions have no loops,
//! user-defined functions or variables: every expression terminates.
//!
//! So far it provides the condition parser (ADR-003):
//!
//! - [`parse_condition`] turns text into an [`Expr`] tree, or a
//!   [`ParseError`] with a [`Span`], a message and maybe a hint.
//! - [`line_col`] turns a byte offset into a line and column.
//! - `Display` on [`Expr`] prints canonical text that parses back to the
//!   same tree.
//!
//! ```text
//! expr    := or
//! or      := and ( "||" and )*
//! and     := not ( "&&" not )*
//! not     := "!" not | cmp
//! cmp     := sum ( ( "==" | "!=" | "<" | "<=" | ">" | ">=" ) sum )?
//! sum     := unary ( ( "+" | "-" ) unary )*
//! unary   := "-" unary | primary
//! primary := literal | path | call | "(" expr ")"
//! call    := path "(" ( expr ( "," expr )* )? ")"
//! literal := int | "true" | "false" | time | duration | symbol
//! ```
//!
//! Literals are integers (`42`), `true`/`false`, times (`d3 22:00`),
//! durations (`15m`, `1h30m`) and symbols (`:bicycle`); there are no
//! strings. Paths are dotted names (`flag.met_marcos`). Comparisons don't
//! chain, and negative numbers are unary minus. Nesting of parentheses, `!`,
//! unary `-` and call arguments is limited to [`MAX_NESTING`] levels.
//!
//! The parser doesn't check types or whether paths exist; that's the type
//! checker's job.
//!
//! See `docs/spec.md`, section "Expression language". Pre-alpha: no API stability.

#![warn(missing_docs)]

mod ast;
mod error;
mod lexer;
mod parser;
mod print;
mod span;

pub use ast::{CmpOp, Expr, ExprKind, Literal, Path, SumOp, SumTerm};
pub use error::ParseError;
pub use parser::{MAX_NESTING, parse_condition};
pub use span::{LineCol, Span, line_col};

#[cfg(test)]
mod tests;

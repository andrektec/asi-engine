//! Expression and effect language of the ASI Engine.
//!
//! This crate will hold the parser, type checker and evaluator for one small,
//! typed, side-effect-free condition language, plus the separate effect syntax
//! for state changes. Both are parsed and type-checked at load, so a typo in
//! content is a load error, not a runtime bug. Expressions have no loops,
//! user-defined functions or variables: every expression terminates.
//!
//! So far it provides conditions: the parser (ADR-003), and the type
//! checker and evaluator (ADR-004).
//!
//! - [`parse_condition`] turns text into an [`Expr`] tree, or a
//!   [`ParseError`] with a [`Span`], a message and maybe a hint.
//! - [`line_col`] turns a byte offset into a line and column.
//! - `Display` on [`Expr`] prints canonical text that parses back to the
//!   same tree.
//! - [`check_condition`] (or [`compile_condition`], which parses first)
//!   resolves names against a [`FrozenSchema`](asi_core::FrozenSchema) and a
//!   [`FunctionRegistry`] and checks types, reporting every error as a
//!   [`Diagnostic`]. The result, a [`CheckedCondition`], lists the keys it
//!   reads and the IDs it mentions.
//! - [`CheckedCondition::eval`] evaluates it against a
//!   [`State`](asi_core::State), returning a bool plus [`EvalWarning`]s for
//!   saturated or clamped arithmetic. On an [`EvalError`], callers treat the
//!   condition as `false` and report the error.
//!
//! ```
//! use asi_core::{IdKind, KindRegistry, Owner, State, StateSchema, Type, Value};
//! use asi_expr::{FunctionRegistry, compile_condition};
//!
//! let mut kinds = KindRegistry::new();
//! kinds.register(IdKind::new("flag")?, Owner::game())?;
//! let mut schema = StateSchema::new(kinds);
//! schema.add_namespace("game", Owner::game())?;
//! schema.add_key(
//!     "game.flags".parse()?,
//!     Type::Set(IdKind::new("flag")?),
//!     Value::Set(Default::default()),
//! )?;
//! schema.set_holder(&"game.flags".parse()?)?;
//! let schema = schema.freeze();
//! let functions = FunctionRegistry::with_core();
//!
//! let late = compile_condition("flag.met_marcos && hour(time) >= 22", &schema, &functions)
//!     .map_err(|errors| errors[0].clone())?;
//! let mut state = State::new(&schema);
//! state.insert(&"game.flags".parse()?, "flag.met_marcos".parse()?)?;
//! state.set(&"core.time".parse()?, Value::Time("d2 23:00".parse()?))?;
//!
//! let mut warnings = Vec::new();
//! assert_eq!(late.eval(&state, &functions, &mut warnings), Ok(true));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
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
//! literal := int | "true" | "false" | "none" | time | duration | symbol
//! ```
//!
//! Literals are integers (`42`), `true`/`false`, times (`d3 22:00`),
//! durations (`15m`, `1h30m`) and symbols (`:bicycle`); there are no
//! strings. Paths are dotted names (`flag.met_marcos`). Comparisons don't
//! chain, and negative numbers are unary minus. Nesting of parentheses, `!`,
//! unary `-` and call arguments is limited to [`MAX_NESTING`] levels.
//!
//! The parser doesn't check types or whether paths exist; that's the type
//! checker's job. ADR-004 adds the literal `none`, the empty value of an
//! `id<kind>` key.
//!
//! See `docs/spec.md`, section "Expression language". Pre-alpha: no API stability.

#![warn(missing_docs)]

mod ast;
mod check;
mod diag;
mod error;
mod eval;
mod func;
mod lexer;
mod parser;
mod print;
mod span;
mod suggest;

pub use ast::{CmpOp, Expr, ExprKind, Literal, Path, SumOp, SumTerm};
pub use check::{CheckedCondition, check_condition, compile_condition};
pub use diag::Diagnostic;
pub use error::ParseError;
pub use eval::{EvalError, EvalWarning, WarningKind};
pub use func::{Function, FunctionCode, FunctionError, FunctionRegistry, RegistryError, StateView};
pub use parser::{MAX_NESTING, parse_condition};
pub use span::{LineCol, Span, line_col};

#[cfg(test)]
mod check_tests;
#[cfg(test)]
mod tests;

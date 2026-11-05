//! Canonical text for expressions.

use std::fmt;

use crate::ast::{Expr, ExprKind, Literal};

/// Binding strength of each node, matching the grammar levels: a child
/// needs parentheses when its level is below what its position requires.
fn level(kind: &ExprKind) -> u8 {
    match kind {
        ExprKind::Or(_) => 1,
        ExprKind::And(_) => 2,
        ExprKind::Not(_) => 3,
        ExprKind::Compare { .. } => 4,
        ExprKind::Sum { .. } => 5,
        ExprKind::Neg(_) => 6,
        ExprKind::Literal(_) | ExprKind::Path(_) | ExprKind::Call { .. } => 7,
    }
}

/// Writes `expr`, parenthesized if its level is below `min`.
fn child(f: &mut fmt::Formatter<'_>, expr: &Expr, min: u8) -> fmt::Result {
    if level(&expr.kind) < min {
        write!(f, "({expr})")
    } else {
        write!(f, "{expr}")
    }
}

/// Writes `items` joined by `sep`, each needing at least level `min`, or
/// `empty` if there are none.
fn join(
    f: &mut fmt::Formatter<'_>,
    items: &[Expr],
    sep: &str,
    min: u8,
    empty: &str,
) -> fmt::Result {
    if items.is_empty() {
        return f.write_str(empty);
    }
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            f.write_str(sep)?;
        }
        child(f, item, min)?;
    }
    Ok(())
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Time(v) => write!(f, "{v}"),
            Self::Duration(v) => write!(f, "{v}"),
            Self::Symbol(v) => write!(f, "{v}"),
        }
    }
}

/// Canonical text: the fewest parentheses that keep the same tree, single
/// spaces around binary operators, none after unary operators, and `, `
/// between call arguments. Durations are written in canonical form (`90m`
/// as `1h30m`).
///
/// For any tree the parser builds, parsing the printed text gives the same
/// tree (ignoring spans). Trees the parser never builds print as follows: an
/// empty `And` as `true`, an empty `Or` as `false`, and a negative `Int` or
/// `Duration` literal with a leading `-` (which parses back as
/// [`ExprKind::Neg`]).
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ExprKind::Literal(literal) => write!(f, "{literal}"),
            ExprKind::Path(path) => write!(f, "{path}"),
            ExprKind::Call { callee, args, .. } => {
                write!(f, "{callee}(")?;
                join(f, args, ", ", 1, "")?;
                f.write_str(")")
            }
            ExprKind::Neg(operand) => {
                f.write_str("-")?;
                child(f, operand, 6)
            }
            ExprKind::Not(operand) => {
                f.write_str("!")?;
                child(f, operand, 3)
            }
            ExprKind::Sum { first, rest } => {
                child(f, first, 6)?;
                for term in rest {
                    write!(f, " {} ", term.op.as_str())?;
                    child(f, &term.operand, 6)?;
                }
                Ok(())
            }
            ExprKind::Compare { op, lhs, rhs, .. } => {
                child(f, lhs, 5)?;
                write!(f, " {} ", op.as_str())?;
                child(f, rhs, 5)
            }
            ExprKind::And(items) => join(f, items, " && ", 3, "true"),
            ExprKind::Or(items) => join(f, items, " || ", 2, "false"),
        }
    }
}

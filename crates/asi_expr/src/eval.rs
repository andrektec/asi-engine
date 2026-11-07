//! Evaluating checked conditions (ADR-004 §4).

use std::fmt;

use asi_core::{CoreError, Duration, State, Time, Type, Value};

use crate::ast::{CmpOp, SumOp};
use crate::check::{CheckedCondition, Node};
use crate::func::{FunctionError, FunctionRegistry, StateView};
use crate::span::Span;

impl CheckedCondition {
    /// Evaluates the condition against `state`, calling functions from
    /// `functions`.
    ///
    /// `&&` and `||` evaluate left to right and short-circuit, so a call
    /// after `false &&` doesn't run. Int and duration overflow saturate at
    /// the limit, and a time before `d1 00:00` clamps to it; each such case
    /// is appended to `warnings`. Evaluation never panics.
    ///
    /// **Callers treat an error as `false` and report it.** Errors come from
    /// broken setups rather than content: a state of a different schema, a
    /// function missing from or changed in `functions`, or function code that
    /// failed, read an undeclared key or returned the wrong type.
    pub fn eval(
        &self,
        state: &State,
        functions: &FunctionRegistry,
        warnings: &mut Vec<EvalWarning>,
    ) -> Result<bool, EvalError> {
        if state.schema() != &self.schema {
            return Err(EvalError::WrongSchema);
        }
        let mut evaluator = Evaluator {
            state,
            functions,
            warnings,
        };
        match evaluator.eval(&self.node)? {
            Value::Bool(b) => Ok(b),
            _ => Err(internal(Span::default())),
        }
    }
}

/// Something evaluation adjusted to keep going (ADR-004 §4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalWarning {
    kind: WarningKind,
    span: Span,
}

impl EvalWarning {
    /// What happened.
    pub fn kind(&self) -> WarningKind {
        self.kind
    }

    /// The span of the subexpression whose result was adjusted.
    pub fn span(&self) -> Span {
        self.span
    }
}

impl fmt::Display for EvalWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.kind {
            WarningKind::IntOverflow => "integer overflow; saturated at the limit",
            WarningKind::DurationOverflow => "duration overflow; saturated at the limit",
            WarningKind::TimeOverflow => "time overflow; saturated at the latest time",
            WarningKind::TimeBeforeStart => "time before `d1 00:00`; clamped to `d1 00:00`",
        })
    }
}

/// The kinds of [`EvalWarning`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum WarningKind {
    /// An int result overflowed and was saturated.
    IntOverflow,
    /// A duration result overflowed and was saturated.
    DurationOverflow,
    /// A time result overflowed and was saturated at the latest time.
    TimeOverflow,
    /// A time result was before `d1 00:00` and was clamped to it.
    TimeBeforeStart,
}

/// Why a condition could not be evaluated. Callers treat it as `false` and
/// report it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EvalError {
    /// The state was created from a different schema than the condition was
    /// checked against.
    WrongSchema,
    /// A function the condition calls isn't in the registry.
    UnknownFunction {
        /// The function's name.
        name: String,
        /// The call.
        span: Span,
    },
    /// A function's signature or reads differ from when the condition was
    /// checked.
    FunctionChanged {
        /// The function's name.
        name: String,
        /// The call.
        span: Span,
    },
    /// A function's code returned an error.
    Function {
        /// The function's name.
        name: String,
        /// The call.
        span: Span,
        /// The error.
        error: FunctionError,
    },
    /// A function returned a value that doesn't match its return type.
    BadReturn {
        /// The function's name.
        name: String,
        /// The call.
        span: Span,
        /// The declared return type.
        expected: Type,
        /// The value returned. Boxed to keep the error small.
        found: Box<Value>,
    },
    /// Reading the state failed.
    State {
        /// The subexpression that read it.
        span: Span,
        /// The error. Boxed to keep the error small.
        error: Box<CoreError>,
    },
    /// A value had a type the checker should have ruled out. This is a bug
    /// in the engine.
    Internal {
        /// The subexpression.
        span: Span,
    },
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongSchema => f.write_str(
                "the state was created from a different schema than the condition was \
                 checked against",
            ),
            Self::UnknownFunction { name, .. } => {
                write!(f, "function `{name}` is not registered")
            }
            Self::FunctionChanged { name, .. } => write!(
                f,
                "function `{name}` has a different signature than when the condition was \
                 checked"
            ),
            Self::Function { name, error, .. } => write!(f, "function `{name}` failed: {error}"),
            Self::BadReturn {
                name,
                expected,
                found,
                ..
            } => write!(
                f,
                "function `{name}` returned `{found}`, which is not of its return type \
                 `{expected}`"
            ),
            Self::State { error, .. } => write!(f, "{error}"),
            Self::Internal { .. } => f.write_str("internal error: a value had an unexpected type"),
        }
    }
}

impl std::error::Error for EvalError {}

fn state_error(span: Span, error: CoreError) -> EvalError {
    EvalError::State {
        span,
        error: Box::new(error),
    }
}

fn internal(span: Span) -> EvalError {
    EvalError::Internal { span }
}

struct Evaluator<'a> {
    state: &'a State,
    functions: &'a FunctionRegistry,
    warnings: &'a mut Vec<EvalWarning>,
}

impl Evaluator<'_> {
    fn warn(&mut self, kind: WarningKind, span: Span) {
        self.warnings.push(EvalWarning { kind, span });
    }

    fn eval(&mut self, node: &Node) -> Result<Value, EvalError> {
        match node {
            Node::Const(value) => Ok(value.clone()),
            Node::Read { key, span } => self
                .state
                .get(key)
                .cloned()
                .map_err(|error| state_error(*span, error)),
            Node::Has(has) => self
                .state
                .contains(&has.key, &has.id)
                .map(Value::Bool)
                .map_err(|error| state_error(has.span, error)),
            Node::Call {
                function,
                span,
                args,
            } => self.call(function, *span, args),
            Node::Neg { operand, span } => {
                let value = self.eval(operand)?;
                self.neg(value, *span)
            }
            Node::Not(operand) => Ok(Value::Bool(!self.bool(operand)?)),
            Node::Sum { first, rest } => {
                let mut acc = self.eval(first)?;
                for term in rest {
                    let rhs = self.eval(&term.operand)?;
                    acc = self.sum(term.op, acc, rhs, term.span)?;
                }
                Ok(acc)
            }
            Node::Compare { op, span, lhs, rhs } => {
                let lhs = self.eval(lhs)?;
                let rhs = self.eval(rhs)?;
                compare(*op, &lhs, &rhs, *span).map(Value::Bool)
            }
            Node::And(items) => {
                for item in items {
                    if !self.bool(item)? {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }
            Node::Or(items) => {
                for item in items {
                    if self.bool(item)? {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }
        }
    }

    fn bool(&mut self, node: &Node) -> Result<bool, EvalError> {
        match self.eval(node)? {
            Value::Bool(b) => Ok(b),
            _ => Err(internal(Span::default())),
        }
    }

    fn call(
        &mut self,
        checked: &crate::func::Function,
        span: Span,
        args: &[Node],
    ) -> Result<Value, EvalError> {
        let name = &checked.name;
        let Some(function) = self.functions.get(name) else {
            return Err(EvalError::UnknownFunction {
                name: name.clone(),
                span,
            });
        };
        if !function.same_signature(checked) {
            return Err(EvalError::FunctionChanged {
                name: name.clone(),
                span,
            });
        }
        let values = args
            .iter()
            .map(|arg| self.eval(arg))
            .collect::<Result<Vec<_>, _>>()?;
        let view = StateView::new(self.state, &function.reads);
        let result = (function.code)(&values, &view).map_err(|error| EvalError::Function {
            name: name.clone(),
            span,
            error,
        })?;
        if !result.type_matches(&function.ret) {
            return Err(EvalError::BadReturn {
                name: name.clone(),
                span,
                expected: function.ret.clone(),
                found: Box::new(result),
            });
        }
        Ok(result)
    }

    fn neg(&mut self, value: Value, span: Span) -> Result<Value, EvalError> {
        match value {
            Value::Int(i) => Ok(Value::Int(i.checked_neg().unwrap_or_else(|| {
                self.warn(WarningKind::IntOverflow, span);
                i64::MAX
            }))),
            Value::Duration(d) => Ok(Value::Duration(d.checked_neg().unwrap_or_else(|| {
                self.warn(WarningKind::DurationOverflow, span);
                Duration::from_minutes(i64::MAX)
            }))),
            _ => Err(internal(span)),
        }
    }

    fn sum(&mut self, op: SumOp, lhs: Value, rhs: Value, span: Span) -> Result<Value, EvalError> {
        match (lhs, rhs) {
            (Value::Int(a), Value::Int(b)) => {
                let checked = match op {
                    SumOp::Add => a.checked_add(b),
                    SumOp::Sub => a.checked_sub(b),
                };
                Ok(Value::Int(checked.unwrap_or_else(|| {
                    self.warn(WarningKind::IntOverflow, span);
                    match op {
                        SumOp::Add => a.saturating_add(b),
                        SumOp::Sub => a.saturating_sub(b),
                    }
                })))
            }
            (Value::Duration(a), Value::Duration(b)) => {
                let checked = match op {
                    SumOp::Add => a.checked_add(b),
                    SumOp::Sub => a.checked_sub(b),
                };
                Ok(Value::Duration(checked.unwrap_or_else(|| {
                    self.warn(WarningKind::DurationOverflow, span);
                    Duration::from_minutes(match op {
                        SumOp::Add => a.minutes().saturating_add(b.minutes()),
                        SumOp::Sub => a.minutes().saturating_sub(b.minutes()),
                    })
                })))
            }
            (Value::Time(t), Value::Duration(d)) => Ok(Value::Time(self.shift(t, op, d, span))),
            (Value::Duration(d), Value::Time(t)) if op == SumOp::Add => {
                Ok(Value::Time(self.shift(t, op, d, span)))
            }
            (Value::Time(a), Value::Time(b)) if op == SumOp::Sub => {
                Ok(Value::Duration(a.duration_since(b)))
            }
            _ => Err(internal(span)),
        }
    }

    /// `time + duration` or `time - duration`, saturated or clamped.
    fn shift(&mut self, time: Time, op: SumOp, duration: Duration, span: Span) -> Time {
        let (t, d) = (time.minutes(), duration.minutes());
        let raw = match op {
            SumOp::Add => t.checked_add(d),
            SumOp::Sub => t.checked_sub(d),
        };
        match raw.map(Time::from_minutes) {
            Some(Some(result)) => result,
            // In range for i64 but negative: before game start.
            Some(None) => {
                self.warn(WarningKind::TimeBeforeStart, span);
                Time::START
            }
            // Overflowed i64. `t` is non-negative, so this went past the
            // latest time, unless the result was below `i64::MIN`, which is
            // impossible for a non-negative `t`.
            None => {
                self.warn(WarningKind::TimeOverflow, span);
                Time::from_minutes(i64::MAX).unwrap_or(Time::START)
            }
        }
    }
}

fn compare(op: CmpOp, lhs: &Value, rhs: &Value, span: Span) -> Result<bool, EvalError> {
    let ordering = match (lhs, rhs) {
        _ if matches!(op, CmpOp::Eq) => return Ok(lhs == rhs),
        _ if matches!(op, CmpOp::Ne) => return Ok(lhs != rhs),
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Time(a), Value::Time(b)) => a.cmp(b),
        (Value::Duration(a), Value::Duration(b)) => a.cmp(b),
        _ => return Err(internal(span)),
    };
    Ok(match op {
        CmpOp::Lt => ordering.is_lt(),
        CmpOp::Le => ordering.is_le(),
        CmpOp::Gt => ordering.is_gt(),
        CmpOp::Ge => ordering.is_ge(),
        CmpOp::Eq => ordering.is_eq(),
        CmpOp::Ne => ordering.is_ne(),
    })
}

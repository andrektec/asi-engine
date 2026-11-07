//! The condition type checker (ADR-004).
//!
//! [`check_condition`] resolves every name in a parsed [`Expr`] against a
//! schema and a function registry, checks every operation's types, and
//! produces a [`CheckedCondition`] that can be evaluated. It reports every
//! error it finds, not just the first.
//!
//! # Names
//!
//! - The single word `time` is the key `core.time`. No other single word is
//!   a valid path.
//! - Otherwise the first segment decides: a declared namespace means a state
//!   key (which must exist), a registered ID kind means an ID, and anything
//!   else is an error.
//! - A call's name is `has` (the built-in form) or a registered function.
//!
//! # IDs
//!
//! An ID written on its own where a `bool` is expected means `has(id)`.
//! Otherwise IDs may only be function arguments of type `id<kind>` or be
//! compared with `==`/`!=` against `id<kind>` values.

use std::collections::BTreeSet;

use asi_core::{FrozenSchema, Id, IdKind, Key, Symbol, Type, Value};

use crate::ast::{CmpOp, Expr, ExprKind, Literal, Path, SumOp, SumTerm};
use crate::diag::Diagnostic;
use crate::func::{Function, FunctionRegistry};
use crate::parser::{MAX_NESTING, parse_condition};
use crate::span::Span;
use crate::suggest::closest;

/// The deepest tree the checker accepts. Parsed trees are bounded by the
/// nesting limit (each nesting level adds at most five nodes: `||`, `&&`, a
/// comparison, a sum and the nesting node itself); this bound only stops
/// hand-built trees from exhausting the stack.
const MAX_DEPTH: usize = 6 * (MAX_NESTING + 1);

/// A condition that passed the type checker, ready to evaluate.
///
/// It remembers the schema it was checked against; evaluating it against a
/// state of another schema is an error.
#[derive(Debug, Clone)]
pub struct CheckedCondition {
    pub(crate) node: Node,
    reads: BTreeSet<Key>,
    ids: BTreeSet<Id>,
    pub(crate) schema: FrozenSchema,
}

impl CheckedCondition {
    /// Every state key the condition reads: directly, through `has` (or an
    /// ID meaning `has`), or through the declared reads of the functions it
    /// calls.
    pub fn reads(&self) -> &BTreeSet<Key> {
        &self.reads
    }

    /// Every ID the condition mentions. The content loader checks that each
    /// is defined.
    pub fn ids(&self) -> &BTreeSet<Id> {
        &self.ids
    }

    /// The schema the condition was checked against.
    pub fn schema(&self) -> &FrozenSchema {
        &self.schema
    }
}

/// The checked tree the evaluator runs.
#[derive(Debug, Clone)]
pub(crate) enum Node {
    Const(Value),
    Read {
        key: Key,
        span: Span,
    },
    /// Boxed: a key and an ID would double the size of every node.
    Has(Box<HasNode>),
    Call {
        function: Box<Function>,
        span: Span,
        args: Vec<Node>,
    },
    Neg {
        operand: Box<Node>,
        span: Span,
    },
    Not(Box<Node>),
    Sum {
        first: Box<Node>,
        rest: Vec<Term>,
    },
    Compare {
        op: CmpOp,
        span: Span,
        lhs: Box<Node>,
        rhs: Box<Node>,
    },
    And(Vec<Node>),
    Or(Vec<Node>),
}

/// `has(id)`: whether the holder set `key` contains `id`.
#[derive(Debug, Clone)]
pub(crate) struct HasNode {
    pub(crate) key: Key,
    pub(crate) id: Id,
    pub(crate) span: Span,
}

/// One `+ x` or `- x` of a [`Node::Sum`]. `span` covers the sum so far,
/// for warnings.
#[derive(Debug, Clone)]
pub(crate) struct Term {
    pub(crate) op: SumOp,
    pub(crate) span: Span,
    pub(crate) operand: Node,
}

/// Type-checks `expr` against `schema` and `functions`.
///
/// Returns every error found, each with a span, a message and maybe a hint,
/// sorted by position. On success, the [`CheckedCondition`] lists what the
/// condition reads and mentions.
pub fn check_condition(
    expr: &Expr,
    schema: &FrozenSchema,
    functions: &FunctionRegistry,
) -> Result<CheckedCondition, Vec<Diagnostic>> {
    let mut checker = Checker {
        schema,
        functions,
        diags: Vec::new(),
        reads: BTreeSet::new(),
        ids: BTreeSet::new(),
        too_deep: false,
    };
    let node = checker
        .expr(expr, 0)
        .and_then(|top| checker.bool_of(top, "a condition"));
    let Checker {
        mut diags,
        reads,
        ids,
        ..
    } = checker;
    match node {
        Some(node) if diags.is_empty() => Ok(CheckedCondition {
            node,
            reads,
            ids,
            schema: schema.clone(),
        }),
        _ => {
            if diags.is_empty() {
                diags.push(Diagnostic::new(expr.span, "the condition is invalid"));
            }
            diags.sort_by_key(|d| (d.span().start, d.span().end));
            Err(diags)
        }
    }
}

/// Parses `text` with [`parse_condition`], then checks it with
/// [`check_condition`]. A parse error is returned as a single diagnostic.
pub fn compile_condition(
    text: &str,
    schema: &FrozenSchema,
    functions: &FunctionRegistry,
) -> Result<CheckedCondition, Vec<Diagnostic>> {
    let expr = parse_condition(text).map_err(|e| vec![Diagnostic::from(e)])?;
    check_condition(&expr, schema, functions)
}

/// What the checker knows about a subexpression's type.
#[derive(Debug, Clone)]
enum Ty {
    /// A value of a known type.
    Val(Type),
    /// An ID written out, such as `clue.receipt`. Its meaning depends on
    /// where it is: `has(id)`, a function argument or a compared value.
    Id(Id),
    /// A symbol literal; its enum type comes from the other side.
    Symbol(Symbol),
    /// The `none` literal.
    None,
}

impl Ty {
    /// How the type is named in messages.
    fn describe(&self) -> String {
        match self {
            Self::Val(ty) => format!("`{ty}`"),
            Self::Id(id) => format!("the ID `{id}`"),
            Self::Symbol(symbol) => format!("the symbol `{symbol}`"),
            Self::None => "`none`".to_owned(),
        }
    }
}

/// A checked subexpression.
struct Typed {
    ty: Ty,
    node: Node,
    span: Span,
}

struct Checker<'a> {
    schema: &'a FrozenSchema,
    functions: &'a FunctionRegistry,
    diags: Vec<Diagnostic>,
    reads: BTreeSet<Key>,
    ids: BTreeSet<Id>,
    too_deep: bool,
}

/// The hint for an ID used where it can't be.
const ID_HINT: &str = "an ID on its own means `has(id)`; otherwise IDs can only be function \
                       arguments or be compared with `id<kind>` values";

impl Checker<'_> {
    fn error(&mut self, diag: Diagnostic) {
        self.diags.push(diag);
    }

    fn expr(&mut self, expr: &Expr, depth: usize) -> Option<Typed> {
        if depth > MAX_DEPTH {
            if !self.too_deep {
                self.too_deep = true;
                self.error(Diagnostic::new(
                    expr.span,
                    "expression is nested too deeply",
                ));
            }
            return None;
        }
        // Each kind is checked in its own method, so this recursive frame
        // stays small even in debug builds.
        let span = expr.span;
        let depth = depth + 1;
        match &expr.kind {
            ExprKind::Literal(literal) => Some(literal_typed(literal, span)),
            ExprKind::Path(path) => self.path(path, span),
            ExprKind::Call {
                callee,
                callee_span,
                args,
            } => self.call(callee, *callee_span, args, span, depth),
            ExprKind::Neg(operand) => self.neg(operand, span, depth),
            ExprKind::Not(operand) => self.not(operand, span, depth),
            ExprKind::And(items) => self.bool_items(items, false, span, depth),
            ExprKind::Or(items) => self.bool_items(items, true, span, depth),
            ExprKind::Sum { first, rest } => self.sum(first, rest, depth),
            ExprKind::Compare {
                op,
                op_span,
                lhs,
                rhs,
            } => self.compare(*op, *op_span, lhs, rhs, span, depth),
        }
    }

    fn neg(&mut self, operand: &Expr, span: Span, depth: usize) -> Option<Typed> {
        let operand = self.expr(operand, depth)?;
        match &operand.ty {
            Ty::Val(ty @ (Type::Int | Type::Duration)) => Some(Typed {
                ty: Ty::Val(ty.clone()),
                node: Node::Neg {
                    operand: Box::new(operand.node),
                    span,
                },
                span,
            }),
            other => {
                let diag = Diagnostic::new(
                    operand.span,
                    format!(
                        "unary `-` applies to `int` and `duration`, found {}",
                        other.describe()
                    ),
                );
                self.error(diag);
                None
            }
        }
    }

    fn not(&mut self, operand: &Expr, span: Span, depth: usize) -> Option<Typed> {
        let operand = self.expr(operand, depth)?;
        let node = self.bool_of(operand, "the operand of `!`")?;
        Some(Typed {
            ty: Ty::Val(Type::Bool),
            node: Node::Not(Box::new(node)),
            span,
        })
    }

    /// Checks each item of an `&&` (or, if `or`, `||`) chain as a bool,
    /// reporting errors in all of them.
    fn bool_items(&mut self, items: &[Expr], or: bool, span: Span, depth: usize) -> Option<Typed> {
        let what = if or {
            "an operand of `||`"
        } else {
            "an operand of `&&`"
        };
        let mut nodes = Vec::with_capacity(items.len());
        let mut ok = true;
        for item in items {
            let node = self.expr(item, depth).and_then(|t| self.bool_of(t, what));
            match node {
                Some(node) => nodes.push(node),
                None => ok = false,
            }
        }
        if !ok {
            return None;
        }
        let node = if or {
            Node::Or(nodes)
        } else {
            Node::And(nodes)
        };
        Some(Typed {
            ty: Ty::Val(Type::Bool),
            node,
            span,
        })
    }

    /// `typed` used where a bool is expected: a bool value, or an ID meaning
    /// `has(id)`. `what` names the position for the message.
    fn bool_of(&mut self, typed: Typed, what: &str) -> Option<Node> {
        match typed.ty {
            Ty::Val(Type::Bool) => Some(typed.node),
            Ty::Id(id) => self.has(id, typed.span),
            other => {
                let hint = match &other {
                    Ty::Val(Type::Set(kind)) => Some(format!(
                        "to test whether the set holds an ID, write the ID (`{kind}.name`) or \
                         `has({kind}.name)`"
                    )),
                    Ty::Val(Type::Ref(_)) => {
                        Some("compare it, e.g. `!= none` or `== kind.name`".to_owned())
                    }
                    Ty::Val(Type::Enum(_)) => {
                        Some("compare it with one of its symbols, e.g. `== :name`".to_owned())
                    }
                    _ => None,
                };
                let diag = Diagnostic::new(
                    typed.span,
                    format!("{what} must be `bool`, found {}", other.describe()),
                )
                .maybe_hint(hint);
                self.error(diag);
                None
            }
        }
    }

    /// `has(id)`: whether `id` is in its kind's holder set.
    fn has(&mut self, id: Id, span: Span) -> Option<Node> {
        match self.schema.holder_of(&id) {
            Some(key) => {
                self.reads.insert(key.clone());
                Some(Node::Has(Box::new(HasNode {
                    key: key.clone(),
                    id,
                    span,
                })))
            }
            None => {
                let kind = id.kind();
                let diag = Diagnostic::new(
                    span,
                    format!(
                        "ID kind `{kind}` has no holder set, so whether `{id}` is held can't be \
                         tested"
                    ),
                )
                .with_hint(format!(
                    "the schema must mark one `set<{kind}>` key as the holder of `{kind}`; to \
                     compare a single-ID value, write e.g. `ns.key == {id}`"
                ));
                self.error(diag);
                None
            }
        }
    }

    fn path(&mut self, path: &Path, span: Span) -> Option<Typed> {
        let text = path.as_str();
        let Some((first, _)) = text.split_once('.') else {
            return self.single_word(text, span);
        };
        let first_span = Span::new(span.start, span.start + first.len());
        if self.schema.namespace_owner(first).is_some() {
            let key = match text.parse::<Key>() {
                Ok(key) => key,
                Err(e) => {
                    self.error(Diagnostic::new(span, e.to_string()));
                    return None;
                }
            };
            let Some(ty) = self.schema.key_type(&key) else {
                let in_namespace = self
                    .schema
                    .keys()
                    .filter(|k| k.namespace() == first)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                let hint = closest(text, in_namespace.iter().map(String::as_str))
                    .map(|k| format!("did you mean `{k}`?"));
                let owner = self
                    .schema
                    .namespace_owner(first)
                    .map(ToString::to_string)
                    .unwrap_or_default();
                let diag = Diagnostic::new(
                    span,
                    format!(
                        "unknown state key `{text}`: namespace `{first}` ({owner}) has no such key"
                    ),
                )
                .maybe_hint(hint);
                self.error(diag);
                return None;
            };
            let ty = ty.clone();
            self.reads.insert(key.clone());
            return Some(Typed {
                ty: Ty::Val(ty),
                node: Node::Read { key, span },
                span,
            });
        }
        if IdKind::new(first).is_ok_and(|kind| self.schema.kinds().contains(&kind)) {
            let id = match text.parse::<Id>() {
                Ok(id) => id,
                Err(e) => {
                    self.error(Diagnostic::new(span, e.to_string()));
                    return None;
                }
            };
            self.ids.insert(id.clone());
            return Some(Typed {
                ty: Ty::Id(id.clone()),
                node: Node::Const(Value::Ref(Some(id))),
                span,
            });
        }
        let names: Vec<&str> = self
            .schema
            .namespaces()
            .map(|(name, _)| name)
            .chain(self.schema.kinds().iter().map(|(kind, _)| kind.as_str()))
            .collect();
        let hint = closest(first, names.iter().copied()).map(|name| {
            let rest = text.get(first.len()..).unwrap_or_default();
            format!("did you mean `{name}{rest}`?")
        });
        let diag = Diagnostic::new(
            first_span,
            format!("`{first}` in `{text}` is neither a state namespace nor an ID kind"),
        )
        .maybe_hint(hint);
        self.error(diag);
        None
    }

    /// A one-segment path: only `time` is valid.
    fn single_word(&mut self, text: &str, span: Span) -> Option<Typed> {
        if text == "time" {
            let key = Key::core_time();
            self.reads.insert(key.clone());
            return Some(Typed {
                ty: Ty::Val(Type::Time),
                node: Node::Read { key, span },
                span,
            });
        }
        let hint = if self.schema.namespace_owner(text).is_some() {
            Some(format!(
                "`{text}` is a namespace; name one of its keys, e.g. `{text}.name`"
            ))
        } else if IdKind::new(text).is_ok_and(|kind| self.schema.kinds().contains(&kind)) {
            Some(format!(
                "`{text}` is an ID kind; write a full ID, e.g. `{text}.name`"
            ))
        } else if text == "has" || self.functions.get(text).is_some() {
            Some(format!("`{text}` is a function; call it: `{text}(...)`"))
        } else {
            closest(text, ["time"]).map(|s| format!("did you mean `{s}`?"))
        };
        let diag = Diagnostic::new(
            span,
            format!(
                "unknown name `{text}`: the only single-word name is `time`; state keys and IDs \
                 have a dot"
            ),
        )
        .maybe_hint(hint);
        self.error(diag);
        None
    }

    fn call(
        &mut self,
        callee: &Path,
        callee_span: Span,
        args: &[Expr],
        span: Span,
        depth: usize,
    ) -> Option<Typed> {
        let name = callee.as_str();
        if name == "has" {
            return self.has_call(args, span, depth);
        }
        let Some(function) = self.functions.get(name) else {
            let names: Vec<&str> = self
                .functions
                .iter()
                .map(|f| f.name.as_str())
                .chain(["has"])
                .collect();
            let hint = closest(name, names.iter().copied()).map(|n| format!("did you mean `{n}`?"));
            let diag =
                Diagnostic::new(callee_span, format!("unknown function `{name}`")).maybe_hint(hint);
            self.error(diag);
            // Still check the arguments, to report their errors too.
            for arg in args {
                self.expr(arg, depth);
            }
            return None;
        };
        let mut ok = true;
        if args.len() != function.args.len() {
            let diag = Diagnostic::new(
                span,
                format!(
                    "`{name}` takes {}, found {}",
                    count(function.args.len(), "argument"),
                    args.len()
                ),
            )
            .with_hint(format!(
                "the signature is `{name}({}) -> {}`",
                signature(&function.args),
                function.ret
            ));
            self.error(diag);
            ok = false;
        }
        let mut nodes = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            let typed = self.expr(arg, depth);
            let expected = function.args.get(index);
            let node = match (typed, expected) {
                (Some(typed), Some(expected)) => self.accept(typed, expected, name, index),
                _ => None,
            };
            match node {
                Some(node) => nodes.push(node),
                None => ok = false,
            }
        }
        for key in &function.reads {
            if self.schema.key_type(key).is_none() {
                let diag = Diagnostic::new(
                    callee_span,
                    format!(
                        "function `{name}` declares that it reads `{key}`, which is not in the \
                         schema"
                    ),
                )
                .with_hint("is the module that owns the key enabled?");
                self.error(diag);
                ok = false;
            } else {
                self.reads.insert(key.clone());
            }
        }
        if !ok {
            return None;
        }
        Some(Typed {
            ty: Ty::Val(function.ret.clone()),
            node: Node::Call {
                function: Box::new(function.clone()),
                span,
                args: nodes,
            },
            span,
        })
    }

    /// An argument of declared type `expected`.
    fn accept(&mut self, typed: Typed, expected: &Type, name: &str, index: usize) -> Option<Node> {
        let what = format!("argument {} of `{name}`", index + 1);
        let fits = match (&typed.ty, expected) {
            (_, Type::Bool) => return self.bool_of(typed, &what),
            (Ty::Id(id), Type::Ref(kind)) => id.kind() == kind,
            (Ty::None, Type::Ref(_)) => true,
            (Ty::Symbol(symbol), Type::Enum(symbols)) => symbols.contains(symbol),
            (Ty::Val(ty), expected) => ty == expected,
            _ => false,
        };
        if fits {
            return Some(typed.node);
        }
        let hint = match (&typed.ty, expected) {
            (Ty::Id(_), Type::Ref(kind)) => Some(format!("pass an ID of kind `{kind}`")),
            (Ty::Symbol(_), Type::Enum(_)) => Some(format!("the symbols are {expected}")),
            _ => None,
        };
        let diag = Diagnostic::new(
            typed.span,
            format!("{what} must be `{expected}`, found {}", typed.ty.describe()),
        )
        .maybe_hint(hint);
        self.error(diag);
        None
    }

    /// `has(id)` written as a call.
    fn has_call(&mut self, args: &[Expr], span: Span, depth: usize) -> Option<Typed> {
        let [arg] = args else {
            let diag = Diagnostic::new(
                span,
                format!(
                    "`has` takes one ID, found {}",
                    count(args.len(), "argument")
                ),
            )
            .with_hint("write e.g. `has(clue.receipt)`");
            self.error(diag);
            for arg in args {
                self.expr(arg, depth);
            }
            return None;
        };
        let typed = self.expr(arg, depth)?;
        let Ty::Id(id) = typed.ty else {
            let hint = match &typed.ty {
                Ty::Val(Type::Ref(_)) => Some(
                    "`has` takes an ID written out; to test a single-ID value, compare it, \
                     e.g. `!= none`"
                        .to_owned(),
                ),
                _ => Some("write e.g. `has(clue.receipt)`".to_owned()),
            };
            let diag = Diagnostic::new(
                typed.span,
                format!(
                    "`has` takes an ID such as `clue.receipt`, found {}",
                    typed.ty.describe()
                ),
            )
            .maybe_hint(hint);
            self.error(diag);
            return None;
        };
        let node = self.has(id, typed.span)?;
        Some(Typed {
            ty: Ty::Val(Type::Bool),
            node,
            span,
        })
    }

    /// An operand of `+`, `-` or an ordering comparison: a value, not an ID,
    /// symbol or `none`.
    fn value_operand(&mut self, typed: Typed, op: &str) -> Option<(Type, Node)> {
        let message = match &typed.ty {
            Ty::Val(ty) => return Some((ty.clone(), typed.node)),
            Ty::Id(id) => format!("the ID `{id}` can't be used with `{op}`"),
            Ty::Symbol(symbol) => format!("the symbol `{symbol}` can't be used with `{op}`"),
            Ty::None => format!("`none` can't be used with `{op}`"),
        };
        let hint = match &typed.ty {
            Ty::Id(_) => ID_HINT,
            Ty::Symbol(_) => "symbols can only be compared with `==` or `!=`",
            _ => "`none` can only be compared with `id<kind>` values",
        };
        self.error(Diagnostic::new(typed.span, message).with_hint(hint));
        None
    }

    fn sum(&mut self, first: &Expr, rest: &[SumTerm], depth: usize) -> Option<Typed> {
        let first_op = rest.first().map_or("+", |t| t.op.as_str());
        let first_typed = self.expr(first, depth);
        let mut acc = first_typed.and_then(|t| self.value_operand(t, first_op));
        let mut span = first.span;
        let mut ok = acc.is_some();
        let mut terms = Vec::with_capacity(rest.len());
        for term in rest {
            let op = term.op.as_str();
            let rhs = self
                .expr(&term.operand, depth)
                .and_then(|t| self.value_operand(t, op));
            span = span.to(term.operand.span);
            let (Some((lt, _)), Some((rt, rnode))) = (&acc, rhs) else {
                ok = false;
                acc = None;
                continue;
            };
            match sum_type(term.op, lt, &rt) {
                Some(ty) => {
                    terms.push(Term {
                        op: term.op,
                        span,
                        operand: rnode,
                    });
                    if let Some((acc_ty, _)) = &mut acc {
                        *acc_ty = ty;
                    }
                }
                None => {
                    let diag = Diagnostic::new(
                        term.op_span,
                        format!("`{op}` can't be applied to `{lt}` and `{rt}`"),
                    )
                    .with_hint(sum_hint(term.op, lt, &rt));
                    self.error(diag);
                    ok = false;
                    acc = None;
                }
            }
        }
        let (ty, first_node) = acc.filter(|_| ok)?;
        Some(Typed {
            ty: Ty::Val(ty),
            node: Node::Sum {
                first: Box::new(first_node),
                rest: terms,
            },
            span,
        })
    }

    fn compare(
        &mut self,
        op: CmpOp,
        op_span: Span,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
        depth: usize,
    ) -> Option<Typed> {
        let lhs = self.expr(lhs, depth);
        let rhs = self.expr(rhs, depth);
        let (lhs, rhs) = (lhs?, rhs?);
        let (lhs, rhs) = match op {
            CmpOp::Eq | CmpOp::Ne => self.equality(op, op_span, lhs, rhs)?,
            CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge => {
                let o = op.as_str();
                let lhs = self.value_operand(lhs, o);
                let rhs = self.value_operand(rhs, o);
                let ((lt, lnode), (rt, rnode)) = (lhs?, rhs?);
                let ordered = matches!(lt, Type::Int | Type::Time | Type::Duration);
                if !(ordered && lt == rt) {
                    let hint = match (&lt, &rt) {
                        (Type::Int, Type::Time) | (Type::Time, Type::Int) => {
                            "to compare a time of day, use `hour(time)` or `minute(time)`"
                        }
                        (Type::Int, Type::Duration) | (Type::Duration, Type::Int) => {
                            "write durations with a unit, e.g. `15m`"
                        }
                        _ => "`<`, `<=`, `>` and `>=` compare two ints, two times or two durations",
                    };
                    let diag =
                        Diagnostic::new(op_span, format!("`{o}` can't compare `{lt}` with `{rt}`"))
                            .with_hint(hint);
                    self.error(diag);
                    return None;
                }
                (lnode, rnode)
            }
        };
        Some(Typed {
            ty: Ty::Val(Type::Bool),
            node: Node::Compare {
                op,
                span,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        })
    }

    /// The operands of `==` or `!=`, converted to nodes if they fit.
    fn equality(
        &mut self,
        op: CmpOp,
        op_span: Span,
        lhs: Typed,
        rhs: Typed,
    ) -> Option<(Node, Node)> {
        let o = op.as_str();
        let mismatch = |lhs: &Ty, rhs: &Ty| {
            format!(
                "`{o}` can't compare {} with {}",
                lhs.describe(),
                rhs.describe()
            )
        };
        match (&lhs.ty, &rhs.ty) {
            (Ty::Val(a), Ty::Val(b)) if a == b => Some((lhs.node, rhs.node)),
            (Ty::Val(Type::Enum(symbols)), Ty::Symbol(s)) => {
                let ok = self.symbol_in(s, symbols, rhs.span);
                ok.then_some((lhs.node, rhs.node))
            }
            (Ty::Symbol(s), Ty::Val(Type::Enum(symbols))) => {
                let ok = self.symbol_in(s, symbols, lhs.span);
                ok.then_some((lhs.node, rhs.node))
            }
            (Ty::Val(Type::Ref(kind)), Ty::Id(id)) | (Ty::Id(id), Ty::Val(Type::Ref(kind))) => {
                if id.kind() == kind {
                    Some((lhs.node, rhs.node))
                } else {
                    let diag =
                        Diagnostic::new(op_span, mismatch(&lhs.ty, &rhs.ty)).with_hint(format!(
                            "an `id<{kind}>` value can only equal IDs of kind `{kind}`, not \
                             `{}`",
                            id.kind()
                        ));
                    self.error(diag);
                    None
                }
            }
            (Ty::Val(Type::Ref(_)), Ty::None) | (Ty::None, Ty::Val(Type::Ref(_))) => {
                Some((lhs.node, rhs.node))
            }
            // An ID compared with a bool is used where a bool is expected.
            (Ty::Val(Type::Bool), Ty::Id(_)) => {
                let rhs = self.bool_of(rhs, "this operand")?;
                Some((lhs.node, rhs))
            }
            (Ty::Id(_), Ty::Val(Type::Bool)) => {
                let lhs = self.bool_of(lhs, "this operand")?;
                Some((lhs, rhs.node))
            }
            (Ty::Symbol(_), Ty::Symbol(_)) => {
                let diag =
                    Diagnostic::new(op_span, "two symbols can't be compared with each other")
                        .with_hint(
                            "compare a key with a symbol instead, e.g. `travel.mode == :walk`",
                        );
                self.error(diag);
                None
            }
            (Ty::None, Ty::None) => {
                let diag = Diagnostic::new(op_span, "`none` can't be compared with `none`")
                    .with_hint(
                        "compare an `id<kind>` key with `none`, e.g. `case.accused != none`",
                    );
                self.error(diag);
                None
            }
            (Ty::None, _) | (_, Ty::None) => {
                let diag = Diagnostic::new(op_span, mismatch(&lhs.ty, &rhs.ty))
                    .with_hint("`none` can only be compared with `id<kind>` values");
                self.error(diag);
                None
            }
            (Ty::Symbol(_), _) | (_, Ty::Symbol(_)) => {
                let diag = Diagnostic::new(op_span, mismatch(&lhs.ty, &rhs.ty))
                    .with_hint("symbols can only be compared with enum values");
                self.error(diag);
                None
            }
            (Ty::Id(_), Ty::Id(_)) => {
                let diag = Diagnostic::new(op_span, mismatch(&lhs.ty, &rhs.ty)).with_hint(
                    "to test whether IDs are held, use `has(...)`; to compare a single-ID \
                     value, compare its key",
                );
                self.error(diag);
                None
            }
            (Ty::Id(_), _) | (_, Ty::Id(_)) => {
                let diag = Diagnostic::new(op_span, mismatch(&lhs.ty, &rhs.ty)).with_hint(ID_HINT);
                self.error(diag);
                None
            }
            (Ty::Val(_), Ty::Val(_)) => {
                let diag = Diagnostic::new(op_span, mismatch(&lhs.ty, &rhs.ty))
                    .with_hint("both sides of `==` and `!=` must have the same type");
                self.error(diag);
                None
            }
        }
    }

    /// Checks that `symbol` (at `span`) is one of `symbols`, the enum type
    /// of the other side.
    fn symbol_in(&mut self, symbol: &Symbol, symbols: &BTreeSet<Symbol>, span: Span) -> bool {
        if symbols.contains(symbol) {
            return true;
        }
        let names: Vec<String> = symbols.iter().map(|s| s.as_str().to_owned()).collect();
        let suggestion = closest(symbol.as_str(), names.iter().map(String::as_str));
        let listed: Vec<String> = symbols.iter().map(ToString::to_string).collect();
        let hint = match suggestion {
            Some(s) => format!("did you mean `:{s}`?"),
            None if listed.is_empty() => "the enum has no symbols".to_owned(),
            None => format!("the symbols are {}", listed.join(", ")),
        };
        let diag = Diagnostic::new(
            span,
            format!("`{symbol}` is not a symbol of the enum it is compared with"),
        )
        .with_hint(hint);
        self.error(diag);
        false
    }
}

/// A literal's type and value.
fn literal_typed(literal: &Literal, span: Span) -> Typed {
    let (ty, value) = match literal {
        Literal::Int(v) => (Ty::Val(Type::Int), Value::Int(*v)),
        Literal::Bool(v) => (Ty::Val(Type::Bool), Value::Bool(*v)),
        Literal::Time(v) => (Ty::Val(Type::Time), Value::Time(*v)),
        Literal::Duration(v) => (Ty::Val(Type::Duration), Value::Duration(*v)),
        Literal::Symbol(s) => (Ty::Symbol(s.clone()), Value::Enum(s.clone())),
        Literal::None => (Ty::None, Value::Ref(None)),
    };
    Typed {
        ty,
        node: Node::Const(value),
        span,
    }
}

/// The result type of `lhs op rhs`, if the operation exists (ADR-004 §4).
fn sum_type(op: SumOp, lhs: &Type, rhs: &Type) -> Option<Type> {
    match (op, lhs, rhs) {
        (_, Type::Int, Type::Int) => Some(Type::Int),
        (_, Type::Duration, Type::Duration) => Some(Type::Duration),
        (_, Type::Time, Type::Duration) | (SumOp::Add, Type::Duration, Type::Time) => {
            Some(Type::Time)
        }
        (SumOp::Sub, Type::Time, Type::Time) => Some(Type::Duration),
        _ => None,
    }
}

/// The hint for an invalid `lhs op rhs`.
fn sum_hint(op: SumOp, lhs: &Type, rhs: &Type) -> &'static str {
    match (op, lhs, rhs) {
        (SumOp::Add, Type::Time, Type::Time) => {
            "two times can't be added; add a duration to a time, or subtract two times to get \
             a duration"
        }
        (SumOp::Sub, Type::Duration, Type::Time) => {
            "a time can't be subtracted from a duration; write `time - duration`"
        }
        (_, Type::Int, Type::Duration | Type::Time)
        | (_, Type::Duration | Type::Time, Type::Int) => "write durations with a unit, e.g. `15m`",
        _ => {
            "`+` and `-` work on two ints, two durations, or a time and a duration; `-` also \
             subtracts two times"
        }
    }
}

/// `n` followed by `noun`, pluralized: `1 argument`, `2 arguments`.
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// The argument types joined by `, `.
fn signature(args: &[Type]) -> String {
    args.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

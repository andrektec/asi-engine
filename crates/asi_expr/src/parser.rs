//! Recursive-descent parser for conditions.

use crate::ast::{CmpOp, Expr, ExprKind, Literal, SumOp, SumTerm};
use crate::error::ParseError;
use crate::lexer::{Lexer, Tok, Token};
use crate::span::{Span, line_col};

/// The deepest allowed nesting of parentheses, `!`, unary `-` and call
/// arguments, counted together.
pub const MAX_NESTING: usize = 64;

/// Parses a condition.
///
/// Returns the syntax tree, or the first error in the text. Never panics,
/// and nesting is limited to [`MAX_NESTING`] levels so hostile input can't
/// overflow the stack.
///
/// ```
/// use asi_expr::{ExprKind, parse_condition};
///
/// let expr = parse_condition("flag.met_marcos && travel.mode == :bicycle").unwrap();
/// assert!(matches!(expr.kind, ExprKind::And(_)));
/// assert_eq!(expr.to_string(), "flag.met_marcos && travel.mode == :bicycle");
///
/// let error = parse_condition("a < b < c").unwrap_err();
/// assert_eq!(error.message(), "comparisons can't be chained");
/// ```
pub fn parse_condition(text: &str) -> Result<Expr, ParseError> {
    let mut parser = Parser::new(text)?;
    let expr = parser.expr()?;
    if parser.tok.tok != Tok::Eof {
        return Err(parser.trailing());
    }
    Ok(expr)
}

struct Parser<'a> {
    text: &'a str,
    lexer: Lexer<'a>,
    /// The current (not yet consumed) token.
    tok: Token,
    depth: usize,
}

fn cmp_op(tok: &Tok) -> Option<CmpOp> {
    Some(match tok {
        Tok::EqEq => CmpOp::Eq,
        Tok::NotEq => CmpOp::Ne,
        Tok::Lt => CmpOp::Lt,
        Tok::Le => CmpOp::Le,
        Tok::Gt => CmpOp::Gt,
        Tok::Ge => CmpOp::Ge,
        _ => return None,
    })
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Result<Self, ParseError> {
        let mut lexer = Lexer::new(text);
        let tok = lexer.next_token()?;
        Ok(Self {
            text,
            lexer,
            tok,
            depth: 0,
        })
    }

    /// Consumes the current token and returns it.
    fn bump(&mut self) -> Result<Token, ParseError> {
        let next = self.lexer.next_token()?;
        Ok(std::mem::replace(&mut self.tok, next))
    }

    /// How the current token reads in a message.
    fn found(&self) -> String {
        match self.tok.tok {
            Tok::Eof => "the end of the condition".to_owned(),
            _ => {
                let span = self.tok.span;
                format!(
                    "`{}`",
                    self.text.get(span.start..span.end).unwrap_or_default()
                )
            }
        }
    }

    /// Enters one level of nesting at `span`.
    fn enter(&mut self, span: Span) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return Err(ParseError::new(
                span,
                format!("expression is nested more than {MAX_NESTING} levels deep"),
            )
            .with_hint("split the condition up, or remove redundant parentheses, `!` or `-`"));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn trailing(&self) -> ParseError {
        let error = ParseError::new(
            self.tok.span,
            format!(
                "expected an operator or the end of the condition, found {}",
                self.found()
            ),
        );
        match &self.tok.tok {
            Tok::Path(p) if p.as_str() == "and" => error.with_hint("use `&&` for \"and\""),
            Tok::Path(p) if p.as_str() == "or" => error.with_hint("use `||` for \"or\""),
            _ => error,
        }
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.or()
    }

    /// Parses `item (sep item)*` into a flat list, or the lone item.
    fn chain(
        &mut self,
        sep: &Tok,
        item: fn(&mut Self) -> Result<Expr, ParseError>,
        build: fn(Vec<Expr>) -> ExprKind,
    ) -> Result<Expr, ParseError> {
        let first = item(self)?;
        if self.tok.tok != *sep {
            return Ok(first);
        }
        let mut span = first.span;
        let mut items = vec![first];
        while self.tok.tok == *sep {
            self.bump()?;
            let next = item(self)?;
            span = span.to(next.span);
            items.push(next);
        }
        Ok(Expr::new(build(items), span))
    }

    fn or(&mut self) -> Result<Expr, ParseError> {
        self.chain(&Tok::OrOr, Self::and, ExprKind::Or)
    }

    fn and(&mut self) -> Result<Expr, ParseError> {
        self.chain(&Tok::AndAnd, Self::not, ExprKind::And)
    }

    fn not(&mut self) -> Result<Expr, ParseError> {
        if self.tok.tok != Tok::Bang {
            return self.cmp();
        }
        let bang = self.bump()?.span;
        self.enter(bang)?;
        let operand = self.not()?;
        self.leave();
        let span = bang.to(operand.span);
        Ok(Expr::new(ExprKind::Not(Box::new(operand)), span))
    }

    fn cmp(&mut self) -> Result<Expr, ParseError> {
        let lhs = self.sum()?;
        let Some(op) = cmp_op(&self.tok.tok) else {
            return Ok(lhs);
        };
        let op_span = self.bump()?.span;
        let rhs = self.sum()?;
        if cmp_op(&self.tok.tok).is_some() {
            return Err(
                ParseError::new(self.tok.span, "comparisons can't be chained")
                    .with_hint("combine comparisons with `&&`, e.g. `a < b && b < c`"),
            );
        }
        let span = lhs.span.to(rhs.span);
        Ok(Expr::new(
            ExprKind::Compare {
                op,
                op_span,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        ))
    }

    fn sum(&mut self) -> Result<Expr, ParseError> {
        let first = self.unary()?;
        let mut span = first.span;
        let mut rest = Vec::new();
        loop {
            let op = match self.tok.tok {
                Tok::Plus => SumOp::Add,
                Tok::Minus => SumOp::Sub,
                _ => break,
            };
            let op_span = self.bump()?.span;
            let operand = self.unary()?;
            span = span.to(operand.span);
            rest.push(SumTerm {
                op,
                op_span,
                operand,
            });
        }
        if rest.is_empty() {
            return Ok(first);
        }
        Ok(Expr::new(
            ExprKind::Sum {
                first: Box::new(first),
                rest,
            },
            span,
        ))
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if self.tok.tok != Tok::Minus {
            return self.primary();
        }
        let minus = self.bump()?.span;
        self.enter(minus)?;
        let operand = self.unary()?;
        self.leave();
        let span = minus.to(operand.span);
        Ok(Expr::new(ExprKind::Neg(Box::new(operand)), span))
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let literal = match &self.tok.tok {
            Tok::Int(v) => Some(Literal::Int(*v)),
            Tok::Bool(v) => Some(Literal::Bool(*v)),
            Tok::Time(v) => Some(Literal::Time(*v)),
            Tok::Duration(v) => Some(Literal::Duration(*v)),
            Tok::Symbol(v) => Some(Literal::Symbol(v.clone())),
            _ => None,
        };
        if let Some(literal) = literal {
            let span = self.bump()?.span;
            return Ok(Expr::new(ExprKind::Literal(literal), span));
        }
        match self.tok.tok {
            Tok::Path(_) => self.path_or_call(),
            Tok::LParen => {
                let open = self.bump()?.span;
                self.enter(open)?;
                let mut inner = self.expr()?;
                let close = self.close_paren(open)?;
                self.leave();
                inner.span = open.to(close);
                Ok(inner)
            }
            Tok::Bang => Err(ParseError::new(
                self.tok.span,
                "`!` can't be used here without parentheses",
            )
            .with_hint("`!` applies to a whole comparison; write e.g. `x == (!y)`")),
            _ => Err(ParseError::new(
                self.tok.span,
                format!("expected an expression, found {}", self.found()),
            )),
        }
    }

    fn path_or_call(&mut self) -> Result<Expr, ParseError> {
        let token = self.bump()?;
        let Tok::Path(path) = token.tok else {
            return Err(ParseError::new(token.span, "expected a path"));
        };
        if self.tok.tok != Tok::LParen {
            return Ok(Expr::new(ExprKind::Path(path), token.span));
        }
        let open = self.bump()?.span;
        self.enter(open)?;
        let mut args = Vec::new();
        if self.tok.tok != Tok::RParen {
            loop {
                args.push(self.expr()?);
                if self.tok.tok != Tok::Comma {
                    break;
                }
                self.bump()?;
                if self.tok.tok == Tok::RParen {
                    return Err(
                        ParseError::new(self.tok.span, "expected an argument after `,`")
                            .with_hint("remove the trailing comma"),
                    );
                }
            }
        }
        let close = self.close_paren(open)?;
        self.leave();
        Ok(Expr::new(
            ExprKind::Call {
                callee: path,
                callee_span: token.span,
                args,
            },
            token.span.to(close),
        ))
    }

    /// Consumes the `)` matching the `(` at `open`, returning its span.
    fn close_paren(&mut self, open: Span) -> Result<Span, ParseError> {
        if self.tok.tok == Tok::RParen {
            return Ok(self.bump()?.span);
        }
        let at = line_col(self.text, open.start);
        let error = ParseError::new(
            self.tok.span,
            format!("expected `)`, found {}", self.found()),
        );
        Err(match self.tok.tok {
            Tok::Eof => error.with_hint(format!(
                "the `(` at line {}, column {} is never closed",
                at.line, at.column
            )),
            _ => error.with_hint(format!(
                "close the `(` at line {}, column {}, or add a missing operator",
                at.line, at.column
            )),
        })
    }
}

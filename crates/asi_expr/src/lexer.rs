//! Turns condition text into tokens, one at a time.

use asi_core::{Duration, Symbol, Time};

use crate::ast::Path;
use crate::error::ParseError;
use crate::span::Span;

/// A token's kind and value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tok {
    Int(i64),
    Bool(bool),
    Time(Time),
    Duration(Duration),
    Symbol(Symbol),
    Path(Path),
    LParen,
    RParen,
    Comma,
    OrOr,
    AndAnd,
    Bang,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Eof,
}

/// A token and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) tok: Tok,
    pub(crate) span: Span,
}

/// Characters that may appear in a word (a path segment, number or
/// duration). Wider than the identifier rules so that, for example, `Flag`
/// or `ção` is read as one word and rejected with a useful message.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_space(c: char) -> bool {
    // `\r` is accepted so CRLF line endings behave like `\n`.
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// Whether `word` looks like the day of a time literal: `d` and digits.
fn is_day_word(word: &str) -> bool {
    word.strip_prefix('d')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// A lazy tokenizer, so the first error by position is the one reported.
pub(crate) struct Lexer<'a> {
    text: &'a str,
    pos: usize,
    /// Whether the last token was a path like `d3`, for a better hint when a
    /// bare `HH:MM` follows it.
    last_was_day_word: bool,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            text,
            pos: 0,
            last_was_day_word: false,
        }
    }

    fn rest(&self) -> &'a str {
        self.text.get(self.pos..).unwrap_or_default()
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_second(&self) -> Option<char> {
        self.rest().chars().nth(1)
    }

    fn slice(&self, start: usize, end: usize) -> &'a str {
        self.text.get(start..end).unwrap_or_default()
    }

    /// Advances past characters matching `pred` and returns the new position.
    fn eat_while(&mut self, pred: impl Fn(char) -> bool) -> usize {
        while let Some(c) = self.peek() {
            if !pred(c) {
                break;
            }
            self.pos += c.len_utf8();
        }
        self.pos
    }

    /// The next token, or [`Tok::Eof`] at the end of the text.
    pub(crate) fn next_token(&mut self) -> Result<Token, ParseError> {
        self.eat_while(is_space);
        let start = self.pos;
        let Some(c) = self.peek() else {
            return Ok(Token {
                tok: Tok::Eof,
                span: Span::new(start, start),
            });
        };
        let token = if c.is_ascii_digit() {
            self.number(start)?
        } else if is_word_char(c) {
            self.path(start)?
        } else {
            self.punct(start, c)?
        };
        self.last_was_day_word = matches!(&token.tok, Tok::Path(p) if is_day_word(p.as_str()));
        Ok(token)
    }

    fn punct(&mut self, start: usize, c: char) -> Result<Token, ParseError> {
        let next = self.peek_second();
        let (tok, len) = match (c, next) {
            ('(', _) => (Tok::LParen, 1),
            (')', _) => (Tok::RParen, 1),
            (',', _) => (Tok::Comma, 1),
            ('+', _) => (Tok::Plus, 1),
            ('-', _) => (Tok::Minus, 1),
            ('|', Some('|')) => (Tok::OrOr, 2),
            ('&', Some('&')) => (Tok::AndAnd, 2),
            ('!', Some('=')) => (Tok::NotEq, 2),
            ('!', _) => (Tok::Bang, 1),
            ('=', Some('=')) => (Tok::EqEq, 2),
            ('<', Some('=')) => (Tok::Le, 2),
            ('<', _) => (Tok::Lt, 1),
            ('>', Some('=')) => (Tok::Ge, 2),
            ('>', _) => (Tok::Gt, 1),
            (':', _) => return self.symbol(start),
            ('"' | '\'', _) => return Err(self.quoted_string(start, c)),
            _ => return Err(self.unexpected(start, c)),
        };
        self.pos += len;
        Ok(Token {
            tok,
            span: Span::new(start, self.pos),
        })
    }

    fn unexpected(&self, start: usize, c: char) -> ParseError {
        let span = Span::new(start, start + c.len_utf8());
        let error = ParseError::new(span, format!("unexpected character `{c}`"));
        match c {
            '|' => error.with_hint("use `||` for \"or\""),
            '&' => error.with_hint("use `&&` for \"and\""),
            '=' => error.with_hint("use `==` to compare; conditions can't assign"),
            '.' => error.with_hint("paths start with a letter and have no spaces around `.`"),
            '#' => error.with_hint("conditions have no comments"),
            _ => error,
        }
    }

    fn quoted_string(&self, start: usize, quote: char) -> ParseError {
        let after = start + quote.len_utf8();
        let end = self
            .text
            .get(after..)
            .and_then(|rest| rest.find(quote))
            .map_or(self.text.len(), |i| after + i + quote.len_utf8());
        ParseError::new(
            Span::new(start, end),
            "quoted strings are not values in conditions",
        )
        .with_hint(
            "there are no string values (ADR-001): use a `:symbol` for one of a fixed set of \
             choices, or a dotted ID such as `clue.receipt` for content",
        )
    }

    /// `:name`
    fn symbol(&mut self, start: usize) -> Result<Token, ParseError> {
        self.pos += 1;
        let end = self.eat_while(|c| is_word_char(c) || c == '.');
        let span = Span::new(start, end);
        let name = self.slice(start + 1, end);
        if name.is_empty() {
            return Err(ParseError::new(span, "expected a symbol name after `:`")
                .with_hint("symbols look like `:bicycle`"));
        }
        let symbol = Symbol::new(name).map_err(|e| ParseError::new(span, e.to_string()))?;
        Ok(Token {
            tok: Tok::Symbol(symbol),
            span,
        })
    }

    /// An integer or a duration; both start with a digit.
    fn number(&mut self, start: usize) -> Result<Token, ParseError> {
        let end = self.eat_while(is_word_char);
        let span = Span::new(start, end);
        let word = self.slice(start, end);
        if !word.bytes().all(|b| b.is_ascii_digit()) {
            let duration: Duration = word
                .parse()
                .map_err(|e: asi_core::CoreError| ParseError::new(span, e.to_string()))?;
            return Ok(Token {
                tok: Tok::Duration(duration),
                span,
            });
        }
        let next_is_digit = self.peek_second().is_some_and(|c| c.is_ascii_digit());
        match self.peek() {
            Some(':') => return Err(self.bare_time(start)),
            Some('.') if next_is_digit => {
                let end = self.eat_while(|c| is_word_char(c) || c == '.');
                return Err(ParseError::new(
                    Span::new(start, end),
                    "fractional numbers are not supported",
                )
                .with_hint("game logic uses whole numbers; for time, use minutes (`90m`)"));
            }
            _ => {}
        }
        if word.len() > 1 && word.starts_with('0') {
            return Err(ParseError::new(
                span,
                format!("integer `{word}` has a leading zero"),
            ));
        }
        let value: i64 = word.parse().map_err(|_| {
            ParseError::new(span, format!("integer `{word}` is too large"))
                .with_hint(format!("the largest integer is {}", i64::MAX))
        })?;
        Ok(Token {
            tok: Tok::Int(value),
            span,
        })
    }

    /// The error for `HH:MM` without a day, with the position just after the
    /// hour digits.
    fn bare_time(&mut self, start: usize) -> ParseError {
        let end = self.eat_while(|c| c.is_ascii_digit() || c == ':');
        let text = self.slice(start, end);
        let error = ParseError::new(
            Span::new(start, end),
            format!(
                "`{text}` is not a time: write `dN HH:MM` with a day, e.g. `d3 22:00`; \
                 time-of-day checks will be functions"
            ),
        );
        if self.last_was_day_word {
            error.with_hint("put exactly one space between the day and the time: `d3 22:00`")
        } else {
            error
        }
    }

    /// A dotted path, `true`/`false`, or a time literal `dN HH:MM`.
    fn path(&mut self, start: usize) -> Result<Token, ParseError> {
        self.eat_while(is_word_char);
        while self.peek() == Some('.') {
            let dot = self.pos;
            self.pos += 1;
            if !self.peek().is_some_and(is_word_char) {
                return Err(ParseError::new(
                    Span::new(dot, dot + 1),
                    "expected a path segment after `.`",
                )
                .with_hint("paths have no spaces around `.` and don't end with `.`"));
            }
            self.eat_while(is_word_char);
        }
        let span = Span::new(start, self.pos);
        let text = self.slice(start, self.pos);
        let tok = match text {
            "true" => Tok::Bool(true),
            "false" => Tok::Bool(false),
            _ if is_day_word(text)
                && self.peek() == Some(' ')
                && self.peek_second().is_some_and(|c| c.is_ascii_digit()) =>
            {
                return self.time(start);
            }
            _ => Tok::Path(Path::validate(text, start)?),
        };
        Ok(Token { tok, span })
    }

    /// `dN HH:MM`, with the position at the space after `dN`.
    fn time(&mut self, start: usize) -> Result<Token, ParseError> {
        self.pos += 1;
        let end = self.eat_while(|c| c.is_ascii_digit() || c == ':');
        let span = Span::new(start, end);
        let time: Time = self
            .slice(start, end)
            .parse()
            .map_err(|e: asi_core::CoreError| ParseError::new(span, e.to_string()))?;
        Ok(Token {
            tok: Tok::Time(time),
            span,
        })
    }
}

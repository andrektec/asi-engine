//! Tests for the condition parser and printer.

use asi_core::{Duration, Symbol, Time};
use proptest::prelude::*;

use crate::*;

// ---------------------------------------------------------------------------
// Helpers

fn parse(text: &str) -> Expr {
    match parse_condition(text) {
        Ok(expr) => expr,
        Err(e) => panic!("{text:?} failed to parse: {e} at {:?}", e.span()),
    }
}

fn err(text: &str) -> ParseError {
    match parse_condition(text) {
        Ok(expr) => panic!("{text:?} parsed as {expr:?}"),
        Err(e) => e,
    }
}

/// Asserts that `text` fails with a message containing `message`, at the
/// span covering the text `at` (the first occurrence of `at` in `text`). An
/// empty `at` means the end of the text.
fn assert_err(text: &str, at: &str, message: &str) -> ParseError {
    let e = err(text);
    let start = if at.is_empty() {
        text.len()
    } else {
        text.find(at).unwrap()
    };
    assert_eq!(
        e.span(),
        Span::new(start, start + at.len()),
        "{text:?}: span of {e}"
    );
    assert!(
        e.message().contains(message),
        "{text:?}: message {:?} lacks {message:?}",
        e.message()
    );
    e
}

/// The tree with every span set to the default, for comparing shapes.
fn strip(expr: &Expr) -> Expr {
    let kind = match &expr.kind {
        ExprKind::Literal(l) => ExprKind::Literal(l.clone()),
        ExprKind::Path(p) => ExprKind::Path(p.clone()),
        ExprKind::Call { callee, args, .. } => ExprKind::Call {
            callee: callee.clone(),
            callee_span: Span::default(),
            args: args.iter().map(strip).collect(),
        },
        ExprKind::Neg(e) => ExprKind::Neg(Box::new(strip(e))),
        ExprKind::Not(e) => ExprKind::Not(Box::new(strip(e))),
        ExprKind::Sum { first, rest } => ExprKind::Sum {
            first: Box::new(strip(first)),
            rest: rest
                .iter()
                .map(|t| SumTerm {
                    op: t.op,
                    op_span: Span::default(),
                    operand: strip(&t.operand),
                })
                .collect(),
        },
        ExprKind::Compare { op, lhs, rhs, .. } => ExprKind::Compare {
            op: *op,
            op_span: Span::default(),
            lhs: Box::new(strip(lhs)),
            rhs: Box::new(strip(rhs)),
        },
        ExprKind::And(items) => ExprKind::And(items.iter().map(strip).collect()),
        ExprKind::Or(items) => ExprKind::Or(items.iter().map(strip).collect()),
    };
    Expr::new(kind, Span::default())
}

/// Fully parenthesized text showing the tree's shape, e.g. `((a - b) - c)`.
fn shape(expr: &Expr) -> String {
    let list = |items: &[Expr], sep: &str| {
        let parts: Vec<String> = items.iter().map(shape).collect();
        format!("({})", parts.join(sep))
    };
    match &expr.kind {
        ExprKind::Literal(l) => l.to_string(),
        ExprKind::Path(p) => p.to_string(),
        ExprKind::Call { callee, args, .. } => {
            let parts: Vec<String> = args.iter().map(shape).collect();
            format!("{callee}({})", parts.join(", "))
        }
        ExprKind::Neg(e) => format!("(-{})", shape(e)),
        ExprKind::Not(e) => format!("(!{})", shape(e)),
        ExprKind::Sum { first, rest } => {
            let mut out = shape(first);
            for t in rest {
                out = format!("({out} {} {})", t.op.as_str(), shape(&t.operand));
            }
            out
        }
        ExprKind::Compare { op, lhs, rhs, .. } => {
            format!("({} {} {})", shape(lhs), op.as_str(), shape(rhs))
        }
        ExprKind::And(items) => list(items, " && "),
        ExprKind::Or(items) => list(items, " || "),
    }
}

fn shape_of(text: &str) -> String {
    shape(&parse(text))
}

fn path(text: &str) -> Path {
    text.parse().unwrap()
}

fn lit(expr: &Expr) -> &Literal {
    match &expr.kind {
        ExprKind::Literal(l) => l,
        other => panic!("not a literal: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Grammar rules

#[test]
fn literals() {
    assert_eq!(lit(&parse("0")), &Literal::Int(0));
    assert_eq!(lit(&parse("42")), &Literal::Int(42));
    assert_eq!(lit(&parse("9223372036854775807")), &Literal::Int(i64::MAX));
    assert_eq!(lit(&parse("true")), &Literal::Bool(true));
    assert_eq!(lit(&parse("false")), &Literal::Bool(false));
    assert_eq!(lit(&parse("none")), &Literal::None);
    assert_eq!(parse(" none ").span, Span::new(1, 5));
    assert_eq!(
        lit(&parse("d3 22:00")),
        &Literal::Time(Time::from_day_hm(3, 22, 0).unwrap())
    );
    assert_eq!(lit(&parse("d1 00:00")), &Literal::Time(Time::START));
    for (text, minutes) in [
        ("15m", 15),
        ("2h", 120),
        ("2d", 2880),
        ("1h30m", 90),
        ("0m", 0),
    ] {
        assert_eq!(
            lit(&parse(text)),
            &Literal::Duration(Duration::from_minutes(minutes)),
            "{text}"
        );
    }
    assert_eq!(
        lit(&parse(":bicycle")),
        &Literal::Symbol(Symbol::new("bicycle").unwrap())
    );
}

#[test]
fn literal_spans() {
    assert_eq!(parse("  d3 22:00 ").span, Span::new(2, 10));
    assert_eq!(parse("1h30m").span, Span::new(0, 5));
    assert_eq!(parse(" :walk").span, Span::new(1, 6));
}

#[test]
fn paths() {
    for text in [
        "flag.met_marcos",
        "travel.mode",
        "has",
        "line.case1.carla.042",
        "true.x",
        "none.x",
        "nones",
        "d3",
        "d3.x",
    ] {
        let expr = parse(text);
        assert_eq!(expr.kind, ExprKind::Path(path(text)), "{text}");
        assert_eq!(expr.span, Span::new(0, text.len()));
    }
    let p = path("line.case1.carla.042");
    assert_eq!(
        p.segments().collect::<Vec<_>>(),
        ["line", "case1", "carla", "042"]
    );
    assert_eq!(p.as_str(), "line.case1.carla.042");
}

#[test]
fn calls() {
    let expr = parse("has(clue.receipt)");
    let ExprKind::Call {
        callee,
        callee_span,
        args,
    } = &expr.kind
    else {
        panic!("{expr:?}");
    };
    assert_eq!(callee, &path("has"));
    assert_eq!(*callee_span, Span::new(0, 3));
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].span, Span::new(4, 16));
    assert_eq!(expr.span, Span::new(0, 17));

    assert_eq!(
        shape_of("rolls.passed(check.empathy_01)"),
        "rolls.passed(check.empathy_01)"
    );
    assert_eq!(shape_of("now()"), "now()");
    assert_eq!(shape_of("f(1, a + b, g(x))"), "f(1, (a + b), g(x))");
    assert_eq!(shape_of("f(a || b, !c)"), "f((a || b), (!c))");
    // Whitespace between the path and `(` is allowed.
    assert_eq!(shape_of("f (x)"), "f(x)");
    assert_eq!(shape_of("f(\n  x,\n  y\n)"), "f(x, y)");
}

#[test]
fn operators() {
    for op in ["==", "!=", "<", "<=", ">", ">="] {
        assert_eq!(shape_of(&format!("a {op} b")), format!("(a {op} b)"));
        assert_eq!(shape_of(&format!("a{op}b")), format!("(a {op} b)"));
    }
    assert_eq!(shape_of("a + b"), "(a + b)");
    assert_eq!(shape_of("a - b"), "(a - b)");
    assert_eq!(shape_of("a && b"), "(a && b)");
    assert_eq!(shape_of("a || b"), "(a || b)");
    assert_eq!(shape_of("!a"), "(!a)");
    assert_eq!(shape_of("-a"), "(-a)");
    assert_eq!(shape_of("(a)"), "a");
}

#[test]
fn precedence_and_associativity() {
    for (text, expected) in [
        ("a - b - c", "((a - b) - c)"),
        ("a - b + c", "((a - b) + c)"),
        ("a - (b - c)", "(a - (b - c))"),
        ("!a && b", "((!a) && b)"),
        ("!(a && b)", "(!(a && b))"),
        ("-x + 1", "((-x) + 1)"),
        ("-(x + 1)", "(-(x + 1))"),
        ("a || b && c", "(a || (b && c))"),
        ("a && b || c", "((a && b) || c)"),
        ("a && b && c", "(a && b && c)"),
        ("a || b || c", "(a || b || c)"),
        ("(a && b) && c", "((a && b) && c)"),
        ("a + 1 < b", "((a + 1) < b)"),
        ("a < b && c > d", "((a < b) && (c > d))"),
        ("a == b || !c", "((a == b) || (!c))"),
        // `!` binds looser than comparison, per the grammar.
        ("!a == b", "(!(a == b))"),
        ("!!a", "(!(!a))"),
        ("--a", "(-(-a))"),
        ("a - -b", "(a - (-b))"),
        ("!-a", "(!(-a))"),
        ("t + 15m >= d3 22:00", "((t + 15m) >= d3 22:00)"),
    ] {
        assert_eq!(shape_of(text), expected, "{text}");
    }
}

#[test]
fn chains_are_flat() {
    let ExprKind::Sum { first, rest } = parse("a - b - c").kind else {
        panic!()
    };
    assert_eq!(first.kind, ExprKind::Path(path("a")));
    assert_eq!(rest.len(), 2);
    assert_eq!(rest[0].op, SumOp::Sub);
    assert_eq!(rest[0].op_span, Span::new(2, 3));
    assert_eq!(rest[1].operand.span, Span::new(8, 9));
    let ExprKind::And(items) = parse("a && b && c").kind else {
        panic!()
    };
    assert_eq!(items.len(), 3);
}

#[test]
fn negative_numbers_are_unary_minus_regardless_of_spacing() {
    let neg_one = shape_of("-1");
    assert_eq!(neg_one, "(-1)");
    assert_eq!(shape_of("- 1"), neg_one);
    assert_eq!(shape_of("-45m"), "(-45m)");
    assert_eq!(shape_of("- 45m"), "(-45m)");
    for text in ["x - 1", "x-1", "x -1", "x- 1"] {
        assert_eq!(shape_of(text), "(x - 1)", "{text}");
    }
    assert_eq!(shape_of("x--1"), "(x - (-1))");
    assert_eq!(shape_of("-9223372036854775807"), "(-9223372036854775807)");
}

#[test]
fn spans_cover_parentheses_and_operands() {
    let expr = parse("(a + b)");
    assert_eq!(expr.span, Span::new(0, 7));
    let expr = parse("(a) + b");
    assert_eq!(expr.span, Span::new(0, 7));
    let ExprKind::Sum { first, .. } = &expr.kind else {
        panic!()
    };
    assert_eq!(first.span, Span::new(0, 3));
    let expr = parse("!a == b");
    assert_eq!(expr.span, Span::new(0, 7));
    let ExprKind::Not(inner) = &expr.kind else {
        panic!()
    };
    let ExprKind::Compare { op, op_span, .. } = &inner.kind else {
        panic!()
    };
    assert_eq!((*op, *op_span), (CmpOp::Eq, Span::new(3, 5)));
}

#[test]
fn whitespace_and_newlines() {
    assert_eq!(
        shape_of("flag.a\n  && flag.b\t|| !flag.c\r\n"),
        "((flag.a && flag.b) || (!flag.c))"
    );
    assert_eq!(shape_of("\n\n x"), "x");
}

// ---------------------------------------------------------------------------
// Errors

#[test]
fn comparisons_do_not_chain() {
    let e = err("a < b < c");
    assert_eq!(e.message(), "comparisons can't be chained");
    // The span is on the second operator.
    assert_eq!(e.span(), Span::new(6, 7));
    assert!(e.hint().unwrap().contains("&&"));
    let e = err("a == b == c");
    assert_eq!(e.span(), Span::new(7, 9));
    // Parentheses make it explicit (and the type checker will judge it).
    assert_eq!(shape_of("(a == b) == c"), "((a == b) == c)");
}

#[test]
fn integer_errors() {
    assert_err("007", "007", "leading zero");
    assert_err("x + 01", "01", "leading zero");
    let e = assert_err("9223372036854775808", "9223372036854775808", "too large");
    assert!(e.hint().unwrap().contains("9223372036854775807"));
    // i64::MIN can't be written: the literal itself is out of range.
    assert_err("-9223372036854775808", "9223372036854775808", "too large");
    assert_err(
        "99999999999999999999999",
        "99999999999999999999999",
        "too large",
    );
    assert_err("1.5", "1.5", "fractional");
    assert_err("1.5h", "1.5h", "fractional");
}

#[test]
fn time_errors() {
    for (text, at) in [("22:00", "22:00"), ("x < 9:30", "9:30"), ("00:00", "00:00")] {
        let e = assert_err(text, at, "write `dN HH:MM`");
        assert!(e.message().contains("time-of-day checks will be functions"));
    }
    // Extra whitespace between day and time.
    let e = assert_err("d3  22:00", "22:00", "write `dN HH:MM`");
    assert!(e.hint().unwrap().contains("exactly one space"));
    let e = assert_err("d3\t22:00", "22:00", "write `dN HH:MM`");
    assert!(e.hint().unwrap().contains("exactly one space"));
    // Bad parts are reported over the whole literal.
    assert_err("d0 10:00", "d0 10:00", "days start at 1");
    assert_err("d01 10:00", "d01 10:00", "leading zeros");
    assert_err("d1 24:00", "d1 24:00", "hour");
    assert_err("d1 10:60", "d1 10:60", "minute");
    assert_err("d1 9:00", "d1 9:00", "two-digit");
    assert_err("d1 10:00:00", "d1 10:00:00", "invalid time");
    assert_err(
        "d99999999999999999999 10:00",
        "d99999999999999999999 10:00",
        "too large",
    );
}

#[test]
fn duration_errors() {
    assert_err("15x", "15x", "unknown unit");
    assert_err("1m1h", "1m1h", "order");
    assert_err("05m", "05m", "leading zeros");
    assert_err("15M", "15M", "unit");
    assert_err(
        "99999999999999999999m",
        "99999999999999999999m",
        "too large",
    );
}

#[test]
fn symbol_errors() {
    assert_err(":", ":", "expected a symbol name");
    assert_err("x == : y", ":", "expected a symbol name");
    assert_err(":Bike", ":Bike", "invalid symbol");
    assert_err(":on.foot", ":on.foot", "invalid symbol");
    assert_err(":1st", ":1st", "invalid symbol");
}

#[test]
fn quoted_strings_are_rejected() {
    for (text, at) in [
        (r#"travel.mode == "bicycle""#, r#""bicycle""#),
        ("travel.mode == 'bicycle'", "'bicycle'"),
        (r#"x == "unclosed"#, r#""unclosed"#),
    ] {
        let e = assert_err(text, at, "quoted strings");
        let hint = e.hint().unwrap();
        assert!(
            hint.contains("ADR-001") && hint.contains(":symbol"),
            "{hint}"
        );
    }
}

#[test]
fn path_errors() {
    assert_err("Flag.x", "Flag", "invalid segment `Flag`");
    assert_err("flag.X", "X", "invalid segment `X`");
    assert_err("Has(x)", "Has", "invalid name `Has`");
    assert_err("_a", "_a", "invalid name");
    assert_err("flag._a", "_a", "invalid segment");
    assert_err("ção", "ção", "invalid name");
    assert_err("042.carla", "042", "leading zero");
    assert_err("flag.", ".", "expected a path segment after `.`");
    assert_err("flag..x", ".", "expected a path segment after `.`");
    assert_err("flag .x", ".", "unexpected character `.`");
    assert_err(".x", ".", "unexpected character `.`");
}

#[test]
fn operator_and_token_errors() {
    let e = assert_err("a = b", "=", "unexpected character `=`");
    assert!(e.hint().unwrap().contains("=="));
    let e = assert_err("a & b", "&", "unexpected character");
    assert!(e.hint().unwrap().contains("&&"));
    let e = assert_err("a | b", "|", "unexpected character");
    assert!(e.hint().unwrap().contains("||"));
    let e = assert_err("a # comment", "#", "unexpected character");
    assert!(e.hint().unwrap().contains("comments"));
    assert_err("a * b", "*", "unexpected character `*`");
    assert_err("a\u{a0}b", "\u{a0}", "unexpected character");
}

#[test]
fn structural_errors() {
    assert_err("", "", "expected an expression, found the end");
    assert_err("   ", "", "expected an expression, found the end");
    assert_err("a +", "", "expected an expression, found the end");
    assert_err("a && )", ")", "expected an expression, found `)`");
    assert_err("()", ")", "expected an expression");
    let e = assert_err("(a", "", "expected `)`, found the end");
    assert!(e.hint().unwrap().contains("line 1, column 1"));
    let e = assert_err("x &&\n  (a b", "b", "expected `)`, found `b`");
    assert!(e.hint().unwrap().contains("line 2, column 3"), "{e}");
    assert_err("a b", "b", "expected an operator or the end");
    assert_err("a)", ")", "expected an operator or the end");
    let e = assert_err("a and b", "and", "expected an operator");
    assert!(e.hint().unwrap().contains("&&"));
    let e = assert_err("a or b", "or", "expected an operator");
    assert!(e.hint().unwrap().contains("||"));
    let e = assert_err("f(a,)", ")", "expected an argument after `,`");
    assert!(e.hint().unwrap().contains("trailing comma"));
    assert_err("f(,)", ",", "expected an expression");
    assert_err("f(a b)", "b", "expected `)`");
    let e = assert_err("x == !y", "!", "`!` can't be used here");
    assert!(e.hint().unwrap().contains("(!y)"));
    assert_err("-!x", "!", "`!` can't be used here");
    assert_err("1 (x)", "(", "expected an operator");
}

#[test]
fn first_error_by_position_wins() {
    // The lexer is lazy, so a later bad token doesn't hide an earlier
    // syntax error.
    assert_err("a b \"str\"", "b", "expected an operator");
}

#[test]
fn error_display_includes_hint() {
    let e = err("a = b");
    assert_eq!(
        e.to_string(),
        "unexpected character `=` (hint: use `==` to compare; conditions can't assign)"
    );
}

#[test]
fn line_col_positions() {
    let text = "a &&\n  é + b\n";
    assert_eq!(line_col(text, 0), LineCol { line: 1, column: 1 });
    assert_eq!(line_col(text, 4), LineCol { line: 1, column: 5 });
    assert_eq!(line_col(text, 5), LineCol { line: 2, column: 1 });
    // `é` is two bytes but one column.
    let plus = text.find('+').unwrap();
    assert_eq!(line_col(text, plus), LineCol { line: 2, column: 5 });
    // Inside a multi-byte character: the character's position.
    let e_acute = text.find('é').unwrap();
    assert_eq!(line_col(text, e_acute + 1), LineCol { line: 2, column: 3 });
    // Past the end: the end.
    assert_eq!(line_col(text, 999), LineCol { line: 3, column: 1 });
    assert_eq!(line_col("", 5), LineCol { line: 1, column: 1 });
}

#[test]
fn path_from_str() {
    assert_eq!(path("a.b").to_string(), "a.b");
    for bad in [
        "", "A", "a.", "a..b", ".a", "1a", "a b", "true", "false", "none", "a-b",
    ] {
        assert!(bad.parse::<Path>().is_err(), "{bad:?}");
    }
    let e = "flag.Bad".parse::<Path>().unwrap_err();
    assert_eq!(e.span(), Span::new(5, 8));
}

// ---------------------------------------------------------------------------
// Nesting limit

fn nested_parens(levels: usize) -> String {
    format!("{}x{}", "(".repeat(levels), ")".repeat(levels))
}

#[test]
fn nesting_limit() {
    assert_eq!(MAX_NESTING, 64);
    let wrap =
        |open: &str, close: &str, n: usize| format!("{}x{}", open.repeat(n), close.repeat(n));
    // Each of these openers adds one level.
    for (open, close) in [("(", ")"), ("!", ""), ("-", ""), ("f(", ")")] {
        let at_limit = wrap(open, close, MAX_NESTING);
        assert!(parse_condition(&at_limit).is_ok(), "{open}: 64 levels");
        let over = wrap(open, close, MAX_NESTING + 1);
        let e = parse_condition(&over).unwrap_err();
        assert!(e.message().contains("nested more than 64"), "{open}: {e}");
        // The error points at the 65th opener.
        let start = open.len() * MAX_NESTING + open.len() - 1;
        assert_eq!(e.span(), Span::new(start, start + 1), "{open}");
    }
    // Mixed kinds count together; the error points at the 65th opener.
    let ok = format!("{}{}f(x)", "!".repeat(31), "-".repeat(32));
    assert!(parse_condition(&ok).is_ok());
    let text = format!("{}{}(x)", "!".repeat(32), "-".repeat(32));
    let e = err(&text);
    assert_eq!(e.span(), Span::new(64, 65));
    // Nesting inside one level and then leaving it doesn't accumulate.
    let siblings = vec![nested_parens(64); 10].join(" && ");
    assert!(parse_condition(&siblings).is_ok());
}

#[test]
fn hostile_inputs_error_without_crashing() {
    for unit in ["(", "!", "-", "f(", "-(", "a(b(c("] {
        let text = unit.repeat(100_000 / unit.len());
        assert!(text.len() >= 99_990);
        let e = parse_condition(&text).unwrap_err();
        assert!(e.message().contains("nested more than 64"), "{unit}: {e}");
    }
}

#[test]
fn long_flat_chains_parse_and_print() {
    // Chains are flat, so their length doesn't deepen the tree.
    for sep in [" + ", " - ", " && ", " || "] {
        let text = vec!["a"; 50_000].join(sep);
        let expr = parse(&text);
        assert_eq!(expr.to_string(), text);
    }
}

// ---------------------------------------------------------------------------
// Printing and round trips

#[test]
fn canonical_printing() {
    for (text, printed) in [
        ("a&&b", "a && b"),
        ("( a )", "a"),
        ("((a + b))", "a + b"),
        ("(a + b) + c", "(a + b) + c"),
        ("a + (b + c)", "a + (b + c)"),
        ("(a && b) && c", "(a && b) && c"),
        ("(a || b) && c", "(a || b) && c"),
        ("(a && b) || c", "a && b || c"),
        ("!(a)", "!a"),
        ("! a", "!a"),
        ("- a", "-a"),
        ("-(-a)", "--a"),
        ("!(a == b)", "!a == b"),
        ("(!a) == b", "(!a) == b"),
        ("-(a + b)", "-(a + b)"),
        ("(-a) + b", "-a + b"),
        ("f( a ,b )", "f(a, b)"),
        ("f ()", "f()"),
        ("90m", "1h30m"),
        ("0d", "0m"),
        ("d10   ==  d3 09:05", "d10 == d3 09:05"),
        ("(a < b) == (c < d)", "(a < b) == (c < d)"),
    ] {
        assert_eq!(parse(text).to_string(), printed, "{text}");
    }
}

#[test]
fn degenerate_trees_print_without_panicking() {
    let s = Span::default();
    let e = |kind| Expr::new(kind, s);
    assert_eq!(e(ExprKind::And(vec![])).to_string(), "true");
    assert_eq!(e(ExprKind::Or(vec![])).to_string(), "false");
    assert_eq!(e(ExprKind::Literal(Literal::Int(-5))).to_string(), "-5");
    assert_eq!(
        e(ExprKind::Literal(Literal::Duration(
            Duration::from_minutes(-90)
        )))
        .to_string(),
        "-1h30m"
    );
}

const CORPUS: &[&str] = &[
    // From the spec.
    "has(clue.receipt)",
    "rolls.passed(check.empathy_01)",
    "flag.met_marcos && travel.mode == :bicycle",
    // More.
    "flag.met_marcos",
    "!flag.met_marcos",
    "var.trust_carla >= 3",
    "var.trust_carla - 1 > -2",
    "time.now < d2 23:00",
    "time.now + 1h30m <= d3 06:15",
    "time.now - time.start >= 2d",
    "has(clue.receipt) || has(clue.photo) && !flag.lied",
    "(has(clue.receipt) || has(clue.photo)) && !flag.lied",
    "!(a || b)",
    "!!a",
    "-(-(1))",
    "a - b - c",
    "a - (b - c)",
    "(a - b) - c",
    "f()",
    "f(g(h(1, 2), :x), true, false)",
    "case.accused == none && none != case.first",
    "f(none)",
    "line.case1.carla.042 == line.case1.carla.043",
    "x != :walk && y == :bicycle || z",
    "(a == b) != (c == d)",
    "-x + 1",
    "a && (b && c)",
    "a || (b || c)",
    "((a))",
    "d1 00:00 < d10 23:59",
    "0m == 0m",
    "-45m < 15m",
    "9223372036854775807 > -9223372036854775807",
];

#[test]
fn corpus_round_trips() {
    for text in CORPUS {
        let first = parse(text);
        let printed = first.to_string();
        let second = parse(&printed);
        assert_eq!(strip(&second), strip(&first), "{text} -> {printed}");
        // Printing is idempotent.
        assert_eq!(second.to_string(), printed, "{text}");
    }
}

// ---------------------------------------------------------------------------
// Property test: random trees print and parse back to themselves.

fn at(kind: ExprKind) -> Expr {
    Expr::new(kind, Span::default())
}

fn arb_path() -> impl Strategy<Value = Path> {
    "[a-z][a-z0-9_]{0,4}(\\.[a-z0-9][a-z0-9_]{0,4}){0,3}"
        .prop_filter_map("literal", |s| s.parse::<Path>().ok())
}

fn arb_literal() -> impl Strategy<Value = Literal> {
    prop_oneof![
        (0..=i64::MAX).prop_map(Literal::Int),
        any::<bool>().prop_map(Literal::Bool),
        Just(Literal::None),
        (0..=i64::MAX).prop_map(|m| Literal::Time(Time::from_minutes(m).unwrap_or_default())),
        (0..=i64::MAX).prop_map(|m| Literal::Duration(Duration::from_minutes(m))),
        "[a-z][a-z0-9_]{0,5}".prop_map(|s| Literal::Symbol(Symbol::new(&s).unwrap())),
    ]
}

fn arb_cmp() -> impl Strategy<Value = CmpOp> {
    prop_oneof![
        Just(CmpOp::Eq),
        Just(CmpOp::Ne),
        Just(CmpOp::Lt),
        Just(CmpOp::Le),
        Just(CmpOp::Gt),
        Just(CmpOp::Ge),
    ]
}

fn arb_expr() -> impl Strategy<Value = Expr> {
    let leaf = prop_oneof![
        arb_literal().prop_map(|l| at(ExprKind::Literal(l))),
        arb_path().prop_map(|p| at(ExprKind::Path(p))),
    ];
    leaf.prop_recursive(8, 64, 4, |inner| {
        let sum_op = prop_oneof![Just(SumOp::Add), Just(SumOp::Sub)];
        prop_oneof![
            inner.clone().prop_map(|e| at(ExprKind::Not(Box::new(e)))),
            inner.clone().prop_map(|e| at(ExprKind::Neg(Box::new(e)))),
            (arb_path(), prop::collection::vec(inner.clone(), 0..4)).prop_map(|(callee, args)| {
                at(ExprKind::Call {
                    callee,
                    callee_span: Span::default(),
                    args,
                })
            }),
            (
                inner.clone(),
                prop::collection::vec((sum_op, inner.clone()), 1..4)
            )
                .prop_map(|(first, rest)| at(ExprKind::Sum {
                    first: Box::new(first),
                    rest: rest
                        .into_iter()
                        .map(|(op, operand)| SumTerm {
                            op,
                            op_span: Span::default(),
                            operand,
                        })
                        .collect(),
                })),
            (arb_cmp(), inner.clone(), inner.clone()).prop_map(|(op, lhs, rhs)| at(
                ExprKind::Compare {
                    op,
                    op_span: Span::default(),
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                }
            )),
            prop::collection::vec(inner.clone(), 2..4).prop_map(|v| at(ExprKind::And(v))),
            prop::collection::vec(inner, 2..4).prop_map(|v| at(ExprKind::Or(v))),
        ]
    })
}

proptest! {
    #[test]
    fn random_trees_round_trip(expr in arb_expr()) {
        let printed = expr.to_string();
        let parsed = parse_condition(&printed)
            .map_err(|e| TestCaseError::fail(format!("{printed:?}: {e}")))?;
        prop_assert_eq!(strip(&parsed), expr, "{}", printed);
        prop_assert_eq!(parsed.to_string(), printed);
    }

    #[test]
    fn arbitrary_text_never_panics(text in "[ -~\\n\\t]{0,64}") {
        if let Err(e) = parse_condition(&text) {
            let span = e.span();
            prop_assert!(span.start <= span.end && span.end <= text.len());
            prop_assert!(text.is_char_boundary(span.start) && text.is_char_boundary(span.end));
        }
    }
}

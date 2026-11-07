//! Tests for the type checker and evaluator.

use std::collections::BTreeSet;

use asi_core::{
    Duration, FrozenSchema, Id, IdKind, Key, KindRegistry, Owner, State, StateSchema, Symbol, Time,
    Type, Value,
};
use proptest::prelude::*;

use crate::*;

// ---------------------------------------------------------------------------
// Test schema and functions

fn key(s: &str) -> Key {
    s.parse().unwrap()
}

fn id(s: &str) -> Id {
    s.parse().unwrap()
}

fn kind(s: &str) -> IdKind {
    IdKind::new(s).unwrap()
}

fn sym(s: &str) -> Symbol {
    Symbol::new(s).unwrap()
}

fn symbols(names: &[&str]) -> BTreeSet<Symbol> {
    names.iter().map(|n| sym(n)).collect()
}

fn mode_type() -> Type {
    Type::Enum(symbols(&["walk", "bicycle", "car"]))
}

/// The schema builder, before freezing, so tests can extend it.
fn builder() -> StateSchema {
    let test = Owner::module("test").unwrap();
    let travel = Owner::module("travel").unwrap();
    let mut kinds = KindRegistry::new();
    for (k, owner) in [
        ("clue", Owner::game()),
        ("flag", Owner::game()),
        ("npc", Owner::game()),
        ("check", test.clone()),
    ] {
        kinds.register(kind(k), owner).unwrap();
    }
    let mut schema = StateSchema::new(kinds);
    for (ns, owner) in [
        ("game", Owner::game()),
        ("case", Owner::game()),
        ("var", Owner::game()),
        ("travel", travel),
        ("test", test),
    ] {
        schema.add_namespace(ns, owner).unwrap();
    }
    let empty = || Value::Set(BTreeSet::new());
    let decls = [
        ("game.clues", Type::Set(kind("clue")), empty()),
        ("game.flags", Type::Set(kind("flag")), empty()),
        ("game.met", Type::Set(kind("npc")), empty()),
        ("travel.mode", mode_type(), Value::Enum(sym("walk"))),
        (
            "travel.weather",
            Type::Enum(symbols(&["rain", "sun"])),
            Value::Enum(sym("sun")),
        ),
        ("case.accused", Type::Ref(kind("npc")), Value::Ref(None)),
        ("case.witness", Type::Ref(kind("npc")), Value::Ref(None)),
        ("case.step", Type::Ref(kind("check")), Value::Ref(None)),
        ("var.trust", Type::Int, Value::Int(0)),
        ("var.met", Type::Bool, Value::Bool(false)),
        ("var.deadline", Type::Time, Value::Time(Time::START)),
        ("var.delay", Type::Duration, Value::Duration(Duration::ZERO)),
        ("test.passed", Type::Set(kind("check")), empty()),
        ("test.secret", Type::Int, Value::Int(0)),
    ];
    for (k, ty, default) in decls {
        schema.add_key(key(k), ty, default).unwrap();
    }
    schema.set_holder(&key("game.clues")).unwrap();
    schema.set_holder(&key("game.flags")).unwrap();
    schema
}

fn schema() -> FrozenSchema {
    builder().freeze()
}

fn bool_arg(args: &[Value], i: usize) -> Result<bool, FunctionError> {
    match args.get(i) {
        Some(Value::Bool(b)) => Ok(*b),
        _ => Err(FunctionError::Failed("expected a bool".to_owned())),
    }
}

fn test_passed(args: &[Value], view: &StateView<'_>) -> Result<Value, FunctionError> {
    match args {
        [Value::Ref(Some(check))] => Ok(Value::Bool(view.contains(&key("test.passed"), check)?)),
        [Value::Ref(None)] => Ok(Value::Bool(false)),
        _ => Err(FunctionError::Failed("expected a check".to_owned())),
    }
}

fn test_fails(_: &[Value], _: &StateView<'_>) -> Result<Value, FunctionError> {
    Err(FunctionError::Failed("boom".to_owned()))
}

fn test_sneaky(_: &[Value], view: &StateView<'_>) -> Result<Value, FunctionError> {
    view.get(&key("test.passed"))?;
    view.get(&key("test.secret"))?;
    Ok(Value::Bool(true))
}

fn test_bad_return(_: &[Value], _: &StateView<'_>) -> Result<Value, FunctionError> {
    Ok(Value::Int(1))
}

fn test_max(args: &[Value], _: &StateView<'_>) -> Result<Value, FunctionError> {
    match args {
        [Value::Int(a), Value::Int(b)] => Ok(Value::Int(*a.max(b))),
        _ => Err(FunctionError::Failed("expected two ints".to_owned())),
    }
}

fn test_near(args: &[Value], _: &StateView<'_>) -> Result<Value, FunctionError> {
    let present = matches!(args.first(), Some(Value::Ref(Some(_))));
    Ok(Value::Bool(present && bool_arg(args, 1)?))
}

fn test_mode_is(args: &[Value], view: &StateView<'_>) -> Result<Value, FunctionError> {
    let current = view.get(&key("travel.mode"))?;
    Ok(Value::Bool(args.first() == Some(current)))
}

fn always_true(_: &[Value], _: &StateView<'_>) -> Result<Value, FunctionError> {
    Ok(Value::Bool(true))
}

fn test_fn(name: &str, args: Vec<Type>, ret: Type, reads: &[&str], code: FunctionCode) -> Function {
    Function {
        name: name.to_owned(),
        owner: Owner::module("test").unwrap(),
        args,
        ret,
        reads: reads.iter().map(|k| key(k)).collect(),
        code,
    }
}

fn functions() -> FunctionRegistry {
    let mut registry = FunctionRegistry::with_core();
    let check = Type::Ref(kind("check"));
    let npc = Type::Ref(kind("npc"));
    for function in [
        test_fn(
            "test.passed",
            vec![check],
            Type::Bool,
            &["test.passed"],
            test_passed,
        ),
        test_fn("test.fails", vec![], Type::Bool, &[], test_fails),
        test_fn(
            "test.sneaky",
            vec![],
            Type::Bool,
            &["test.passed"],
            test_sneaky,
        ),
        test_fn("test.bad_return", vec![], Type::Bool, &[], test_bad_return),
        test_fn(
            "test.max",
            vec![Type::Int, Type::Int],
            Type::Int,
            &[],
            test_max,
        ),
        test_fn(
            "test.near",
            vec![npc, Type::Bool],
            Type::Bool,
            &[],
            test_near,
        ),
        test_fn(
            "test.mode_is",
            vec![mode_type()],
            Type::Bool,
            &["travel.mode"],
            test_mode_is,
        ),
        test_fn(
            "test.ghost",
            vec![],
            Type::Bool,
            &["test.missing"],
            always_true,
        ),
    ] {
        registry.register(function).unwrap();
    }
    registry
}

// ---------------------------------------------------------------------------
// Helpers

fn check(text: &str) -> Result<CheckedCondition, Vec<Diagnostic>> {
    compile_condition(text, &schema(), &functions())
}

fn ok(text: &str) -> CheckedCondition {
    match check(text) {
        Ok(checked) => checked,
        Err(errors) => {
            let listed: Vec<String> = errors
                .iter()
                .map(|e| format!("{e} at {:?}", e.span()))
                .collect();
            panic!("{text:?} failed to check: {listed:#?}")
        }
    }
}

fn errs(text: &str) -> Vec<Diagnostic> {
    match check(text) {
        Ok(_) => panic!("{text:?} type-checked"),
        Err(errors) => errors,
    }
}

/// The span of the `nth` occurrence (from 0) of `at` in `text`.
fn span_of(text: &str, at: &str, nth: usize) -> Span {
    let start = text
        .match_indices(at)
        .nth(nth)
        .map(|(i, _)| i)
        .unwrap_or_else(|| panic!("{at:?} not in {text:?}"));
    Span::new(start, start + at.len())
}

/// Asserts that `diag` is at the first `at` in `text` and its message
/// contains `message`.
fn assert_diag(text: &str, diag: &Diagnostic, at: &str, message: &str) {
    assert_eq!(
        diag.span(),
        span_of(text, at, 0),
        "{text:?}: span of {diag}"
    );
    assert!(
        diag.message().contains(message),
        "{text:?}: message {:?} lacks {message:?}",
        diag.message()
    );
}

/// Asserts that `text` fails with exactly one error, at the first `at`,
/// whose message contains `message`. Returns it, to check the hint.
fn assert_err(text: &str, at: &str, message: &str) -> Diagnostic {
    let errors = errs(text);
    assert_eq!(errors.len(), 1, "{text:?}: {errors:#?}");
    assert_diag(text, &errors[0], at, message);
    errors[0].clone()
}

fn state() -> State {
    State::new(&schema())
}

fn eval_in(text: &str, state: &State) -> (Result<bool, EvalError>, Vec<EvalWarning>) {
    let mut warnings = Vec::new();
    let result = ok(text).eval(state, &functions(), &mut warnings);
    (result, warnings)
}

fn eval(text: &str, state: &State) -> bool {
    let (result, warnings) = eval_in(text, state);
    assert!(warnings.is_empty(), "{text:?}: {warnings:?}");
    match result {
        Ok(b) => b,
        Err(e) => panic!("{text:?} failed to evaluate: {e}"),
    }
}

fn keys(names: &[&str]) -> BTreeSet<Key> {
    names.iter().map(|k| key(k)).collect()
}

fn ids(names: &[&str]) -> BTreeSet<Id> {
    names.iter().map(|i| id(i)).collect()
}

// ---------------------------------------------------------------------------
// Spec examples

#[test]
fn spec_examples_type_check() {
    for text in [
        "has(clue.receipt)",
        "flag.met_marcos && travel.mode == :bicycle",
        "time >= d3 22:00",
        "hour(time) >= 22 || hour(time) < 2",
        "case.accused == npc.carla",
        "case.accused != none",
        "test.passed(check.a)",
    ] {
        ok(text);
    }
}

#[test]
fn spec_examples_evaluate() {
    let mut s = state();
    assert!(!eval("has(clue.receipt)", &s));
    s.insert(&key("game.clues"), id("clue.receipt")).unwrap();
    assert!(eval("has(clue.receipt)", &s));

    let text = "flag.met_marcos && travel.mode == :bicycle";
    s.insert(&key("game.flags"), id("flag.met_marcos")).unwrap();
    assert!(!eval(text, &s));
    s.set(&key("travel.mode"), Value::Enum(sym("bicycle")))
        .unwrap();
    assert!(eval(text, &s));

    let late = "hour(time) >= 22 || hour(time) < 2";
    for (time, expected_late, expected_d3) in [
        ("d1 00:00", true, false),
        ("d3 21:59", false, false),
        ("d3 22:00", true, true),
        ("d4 01:59", true, true),
        ("d4 02:00", false, true),
    ] {
        s.set(&Key::core_time(), Value::Time(time.parse().unwrap()))
            .unwrap();
        assert_eq!(eval(late, &s), expected_late, "{time}");
        assert_eq!(eval("time >= d3 22:00", &s), expected_d3, "{time}");
    }

    assert!(!eval("case.accused == npc.carla", &s));
    assert!(!eval("case.accused != none", &s));
    assert!(eval("case.accused == none", &s));
    s.set(&key("case.accused"), Value::Ref(Some(id("npc.carla"))))
        .unwrap();
    assert!(eval("case.accused == npc.carla", &s));
    assert!(!eval("case.accused == npc.marcos", &s));
    assert!(eval("case.accused != none", &s));
    assert!(eval("npc.carla == case.accused", &s));

    assert!(!eval("test.passed(check.a)", &s));
    s.insert(&key("test.passed"), id("check.a")).unwrap();
    assert!(eval("test.passed(check.a)", &s));
    assert!(!eval("test.passed(check.b)", &s));
}

#[test]
fn core_time_functions() {
    let mut s = state();
    s.set(&Key::core_time(), Value::Time("d3 22:05".parse().unwrap()))
        .unwrap();
    assert!(eval(
        "day(time) == 3 && hour(time) == 22 && minute(time) == 5",
        &s
    ));
    assert!(eval("day(d1 00:00) == 1 && hour(d1 23:59) == 23", &s));
    assert!(eval("minute(var.deadline + 61m) == 1", &s));
    let checked = ok("hour(time) >= 22");
    assert_eq!(checked.reads(), &keys(&["core.time"]));
    assert!(checked.ids().is_empty());
}

#[test]
fn bare_ids_mean_has() {
    let mut s = state();
    for (bare, call) in [
        ("flag.x", "has(flag.x)"),
        ("!flag.x", "!has(flag.x)"),
        ("flag.x && clue.y", "has(flag.x) && has(clue.y)"),
        ("flag.x == true", "has(flag.x) == true"),
        ("false != flag.x", "false != has(flag.x)"),
        ("test.near(npc.a, flag.x)", "test.near(npc.a, has(flag.x))"),
    ] {
        let (a, b) = (ok(bare), ok(call));
        assert_eq!(a.reads(), b.reads(), "{bare}");
        assert_eq!(a.ids(), b.ids(), "{bare}");
        for _ in 0..2 {
            assert_eq!(eval(bare, &s), eval(call, &s), "{bare}");
            s.insert(&key("game.flags"), id("flag.x")).unwrap();
            s.insert(&key("game.clues"), id("clue.y")).unwrap();
        }
        s = state();
    }
    let checked = ok("flag.x");
    assert_eq!(checked.reads(), &keys(&["game.flags"]));
    assert_eq!(checked.ids(), &ids(&["flag.x"]));
}

#[test]
fn compared_ids_are_values_not_has() {
    let checked = ok("case.accused == npc.carla");
    assert_eq!(checked.reads(), &keys(&["case.accused"]));
    assert_eq!(checked.ids(), &ids(&["npc.carla"]));
    // `npc` has no holder, yet the comparison is fine.
    ok("case.accused != npc.carla && case.accused == case.witness");
}

#[test]
fn reads_and_ids_cover_everything() {
    let checked = ok(
        "flag.a && has(clue.b) && test.passed(check.c) && time > var.deadline \
         && case.accused != npc.d && test.mode_is(:walk)",
    );
    assert_eq!(
        checked.reads(),
        &keys(&[
            "game.flags",
            "game.clues",
            "test.passed",
            "core.time",
            "var.deadline",
            "case.accused",
            "travel.mode",
        ])
    );
    assert_eq!(
        checked.ids(),
        &ids(&["flag.a", "clue.b", "check.c", "npc.d"])
    );
    assert_eq!(checked.schema(), &schema());
}

// ---------------------------------------------------------------------------
// Type rules: valid

#[test]
fn arithmetic_rules() {
    for text in [
        "var.trust + 1 - 2 == 3",
        "-var.trust < 0",
        "var.delay + 15m - 1h == 0m",
        "-var.delay < 0m",
        "time + 15m > time",
        "time - 1d < time",
        "15m + time > time",
        "time - var.deadline >= 0m",
        "time - var.deadline + 15m >= 0m",
        "time - 15m + 15m == time",
        "test.max(var.trust, 3) + 1 > 0",
    ] {
        ok(text);
    }
}

#[test]
fn comparison_rules() {
    for text in [
        "1 < 2",
        "var.trust <= 2",
        "time > d1 00:00",
        "var.delay >= 15m",
        "var.met == true",
        "var.met != var.met",
        "travel.mode == :walk",
        ":car != travel.mode",
        "travel.mode == travel.mode",
        "case.accused == case.witness",
        "none != case.accused",
        "game.met == game.met",
        "(1 < 2) == (3 < 4)",
    ] {
        ok(text);
    }
}

#[test]
fn function_argument_rules() {
    for text in [
        "test.passed(check.a)",
        "test.passed(none)",
        "test.passed(case.step)",
        "test.mode_is(:car)",
        "test.mode_is(travel.mode)",
        "test.near(case.accused, var.met)",
        "test.near(npc.a, flag.a || var.met)",
    ] {
        ok(text);
    }
}

// ---------------------------------------------------------------------------
// Type rules: errors

#[test]
fn unknown_keys() {
    let e = assert_err("var.trus > 0", "var.trus", "unknown state key `var.trus`");
    assert_eq!(e.hint(), Some("did you mean `var.trust`?"));
    let e = assert_err("game.zzz", "game.zzz", "unknown state key");
    assert_eq!(e.hint(), None);
    assert_err("core.date", "core.date", "unknown state key");
}

#[test]
fn unknown_kinds_and_namespaces() {
    let e = assert_err(
        "clu.receipt",
        "clu",
        "`clu` in `clu.receipt` is neither a state namespace nor an ID kind",
    );
    assert_eq!(e.hint(), Some("did you mean `clue.receipt`?"));
    let e = assert_err("travle.mode == :walk", "travle", "neither");
    assert_eq!(e.hint(), Some("did you mean `travel.mode`?"));
    let e = assert_err("zzzz.y", "zzzz", "neither");
    assert_eq!(e.hint(), None);
}

#[test]
fn single_words() {
    let e = assert_err("tme > d1 00:00", "tme", "unknown name `tme`");
    assert_eq!(e.hint(), Some("did you mean `time`?"));
    let e = assert_err("var", "var", "unknown name");
    assert!(e.hint().unwrap().contains("is a namespace"));
    let e = assert_err("flag", "flag", "unknown name");
    assert!(e.hint().unwrap().contains("is an ID kind"));
    let e = assert_err("hour > 2", "hour", "unknown name");
    assert!(e.hint().unwrap().contains("is a function"));
    assert_err("x", "x", "unknown name");
}

#[test]
fn ids_need_a_holder_for_has() {
    let e = assert_err(
        "has(check.a)",
        "check.a",
        "ID kind `check` has no holder set",
    );
    assert!(e.hint().unwrap().contains("holder"));
    assert_err("npc.carla", "npc.carla", "ID kind `npc` has no holder set");
    assert_err("var.met && !npc.carla", "npc.carla", "no holder");
}

#[test]
fn has_takes_one_written_id() {
    assert_err("has()", "has()", "`has` takes one ID, found 0 arguments");
    assert_err(
        "has(clue.a, clue.b)",
        "has(clue.a, clue.b)",
        "found 2 arguments",
    );
    let e = assert_err(
        "has(case.accused)",
        "case.accused",
        "`has` takes an ID such as `clue.receipt`, found `id<npc>`",
    );
    assert!(e.hint().unwrap().contains("!= none"));
    assert_err("has(var.met)", "var.met", "found `bool`");
    assert_err("has(1)", "1", "found `int`");
}

#[test]
fn ids_in_invalid_positions() {
    for (text, at, message) in [
        (
            "clue.a + 1 > 0",
            "clue.a",
            "the ID `clue.a` can't be used with `+`",
        ),
        ("1 - clue.a > 0", "clue.a", "can't be used with `-`"),
        (
            "-clue.a > 0",
            "clue.a",
            "unary `-` applies to `int` and `duration`",
        ),
        (
            "var.trust == clue.a",
            "==",
            "can't compare `int` with the ID `clue.a`",
        ),
        (
            "clue.a == clue.b",
            "==",
            "can't compare the ID `clue.a` with the ID `clue.b`",
        ),
        (
            "test.max(clue.a, 1) > 0",
            "clue.a",
            "argument 1 of `test.max` must be `int`",
        ),
    ] {
        assert_err(text, at, message);
    }
    // Both operands are reported.
    let text = "clue.a <= clue.b";
    let errors = errs(text);
    assert_eq!(errors.len(), 2, "{errors:#?}");
    assert_diag(text, &errors[1], "clue.b", "can't be used with `<=`");
}

#[test]
fn enum_symbols() {
    let e = assert_err(
        "travel.mode == :bicyle",
        ":bicyle",
        "`:bicyle` is not a symbol of the enum it is compared with",
    );
    assert_eq!(e.hint(), Some("did you mean `:bicycle`?"));
    let e = assert_err(":plane != travel.mode", ":plane", "not a symbol");
    assert_eq!(e.hint(), Some("the symbols are :bicycle, :car, :walk"));
    let e = assert_err(":a == :b", "==", "two symbols can't be compared");
    assert!(e.hint().is_some());
    assert_err(
        "var.trust == :a",
        "==",
        "can't compare `int` with the symbol `:a`",
    );
    assert_err(
        "travel.mode == travel.weather",
        "==",
        "can't compare `enum{:bicycle, :car, :walk}` with `enum{:rain, :sun}`",
    );
    assert_err(":a + 1 > 0", ":a", "the symbol `:a` can't be used with `+`");
    assert_err(
        "test.mode_is(:boat)",
        ":boat",
        "argument 1 of `test.mode_is`",
    );
}

#[test]
fn arithmetic_errors() {
    let e = assert_err(
        "time + time == time",
        "+",
        "`+` can't be applied to `time` and `time`",
    );
    assert!(e.hint().unwrap().contains("subtract two times"));
    let e = assert_err("var.trust + 15m > 0", "+", "`int` and `duration`");
    assert_eq!(e.hint(), Some("write durations with a unit, e.g. `15m`"));
    assert_err("15m - time > time", "-", "`duration` and `time`");
    assert_err("var.met + 1 > 0", "+", "`bool` and `int`");
    assert_err(
        "-time > time",
        "time",
        "unary `-` applies to `int` and `duration`, found `time`",
    );
    assert_err("-var.met", "var.met", "found `bool`");
    // The error is at the operator that fails, and the sum isn't checked
    // further.
    let text = "1 + 2 + time - 3 > 0";
    let errors = errs(text);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert_eq!(errors[0].span(), span_of(text, "+", 1));
    assert!(errors[0].message().contains("`int` and `time`"));
}

#[test]
fn comparison_errors() {
    let e = assert_err(
        "var.trust < time",
        "<",
        "`<` can't compare `int` with `time`",
    );
    assert!(e.hint().unwrap().contains("hour(time)"));
    assert_err("var.delay >= 15", ">=", "`duration` with `int`");
    assert_err("var.met > false", ">", "`bool` with `bool`");
    assert_err("travel.mode < :walk", ":walk", "can't be used with `<`");
    assert_err(
        "var.trust == var.met",
        "==",
        "can't compare `int` with `bool`",
    );
    assert_err("time != 15m", "!=", "`time` with `duration`");
}

#[test]
fn bool_operator_errors() {
    assert_err("!3", "3", "the operand of `!` must be `bool`, found `int`");
    assert_err(
        "var.met && 1",
        "1",
        "an operand of `&&` must be `bool`, found `int`",
    );
    assert_err(
        "var.trust || var.met",
        "var.trust",
        "an operand of `||` must be `bool`",
    );
    assert_err("!:a", ":a", "found the symbol `:a`");
    assert_err("!none", "none", "found `none`");
}

#[test]
fn conditions_must_be_bool() {
    for (text, found) in [
        ("var.trust", "`int`"),
        ("1 + 2", "`int`"),
        ("time", "`time`"),
        ("15m", "`duration`"),
        (":walk", "the symbol `:walk`"),
        ("none", "`none`"),
        ("case.accused", "`id<npc>`"),
        ("game.met", "`set<npc>`"),
        ("travel.mode", "`enum{:bicycle, :car, :walk}`"),
        ("test.max(1, 2)", "`int`"),
    ] {
        let e = assert_err(
            text,
            text,
            &format!("a condition must be `bool`, found {found}"),
        );
        if text == "case.accused" {
            assert!(e.hint().unwrap().contains("!= none"));
        }
    }
}

#[test]
fn single_id_errors() {
    let e = assert_err(
        "case.accused == clue.a",
        "==",
        "can't compare `id<npc>` with the ID `clue.a`",
    );
    assert!(e.hint().unwrap().contains("kind `npc`"));
    assert_err(
        "case.accused != case.step",
        "!=",
        "can't compare `id<npc>` with `id<check>`",
    );
    let e = assert_err("travel.mode == none", "==", "can't compare");
    assert!(e.hint().unwrap().contains("`none` can only be compared"));
    assert_err("none == none", "==", "`none` can't be compared with `none`");
    assert_err(
        "none == clue.a",
        "==",
        "can't compare `none` with the ID `clue.a`",
    );
    assert_err(
        "case.accused < none",
        "none",
        "`none` can't be used with `<`",
    );
    assert_err("case.accused + 1 > 0", "+", "`id<npc>` and `int`");
    assert_err(
        "test.passed(npc.a)",
        "npc.a",
        "argument 1 of `test.passed` must be `id<check>`, found the ID `npc.a`",
    );
    assert_err(
        "test.passed(case.accused)",
        "case.accused",
        "must be `id<check>`",
    );
}

#[test]
fn function_call_errors() {
    let e = assert_err(
        "test.max(1) > 0",
        "test.max(1)",
        "`test.max` takes 2 arguments, found 1",
    );
    assert_eq!(
        e.hint(),
        Some("the signature is `test.max(int, int) -> int`")
    );
    assert_err(
        "test.passed() || var.met",
        "test.passed()",
        "takes 1 argument, found 0",
    );
    assert_err(
        "test.max(1, true) > 0",
        "true",
        "argument 2 of `test.max` must be `int`, found `bool`",
    );
    assert_err(
        "hour(1) > 0",
        "1",
        "argument 1 of `hour` must be `time`, found `int`",
    );
    let e = assert_err(
        "test.pased(check.a)",
        "test.pased",
        "unknown function `test.pased`",
    );
    assert_eq!(e.hint(), Some("did you mean `test.passed`?"));
    let e = assert_err("hs(clue.a)", "hs", "unknown function");
    assert_eq!(e.hint(), Some("did you mean `has`?"));
    assert_err(
        "test.ghost()",
        "test.ghost",
        "declares that it reads `test.missing`, which is not in the schema",
    );
}

#[test]
fn several_errors_are_reported_together() {
    let text = "var.nope && clue.a + 1 > 0 && :a == :b || !3 || test.max(true) > time";
    let errors = errs(text);
    let expected = [
        ("var.nope", "unknown state key"),
        ("clue.a", "can't be used with `+`"),
        ("==", "two symbols"),
        ("3", "the operand of `!`"),
        ("test.max(true)", "takes 2 arguments"),
        ("true", "argument 1 of `test.max` must be `int`"),
    ];
    assert_eq!(errors.len(), expected.len(), "{errors:#?}");
    for (diag, (at, message)) in errors.iter().zip(expected) {
        assert_diag(text, diag, at, message);
    }
    // Errors inside an unknown function's arguments are reported too.
    let errors = errs("nope(var.nope)");
    assert_eq!(errors.len(), 2, "{errors:#?}");
}

#[test]
fn parse_errors_become_diagnostics() {
    let errors = errs("var.met &&");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].span(), Span::new(10, 10));
    let errors = errs("a < b < c");
    assert!(errors[0].hint().unwrap().contains("&&"));
    assert_eq!(
        errors[0].to_string(),
        format!(
            "{} (hint: {})",
            errors[0].message(),
            errors[0].hint().unwrap()
        )
    );
}

#[test]
fn hand_built_deep_trees_are_rejected_without_overflow() {
    let mut expr = Expr::new(ExprKind::Literal(Literal::Bool(true)), Span::default());
    for _ in 0..1000 {
        expr = Expr::new(ExprKind::Not(Box::new(expr)), Span::default());
    }
    let errors = check_condition(&expr, &schema(), &functions()).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message().contains("nested too deeply"));
}

/// Runs `f` on a thread with a 1 MiB stack, the main-thread size on Windows.
fn with_small_stack(f: fn()) {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn deep_trees_fit_a_small_stack() {
    with_small_stack(hand_built_deep_trees_are_rejected_without_overflow);
    with_small_stack(deepest_parsable_conditions_check_and_evaluate);
}

#[test]
fn deepest_parsable_conditions_check_and_evaluate() {
    // 64 levels of parentheses, each adding `||`, `&&` and `==` nodes.
    let mut text = "var.met".to_owned();
    for _ in 0..MAX_NESTING {
        text = format!("var.met || var.met && ({text}) == var.met");
    }
    assert!(!eval(&text, &state()));
    // `-(` is two levels.
    let mut text = "1".to_owned();
    for _ in 0..MAX_NESTING / 2 {
        text = format!("var.trust + -({text})");
    }
    assert!(eval(&format!("{text} == 1"), &state()));
    // The most nodes per level (`||`, `&&`, `==`, `+`): ill-typed, but the
    // checker still recurses to the bottom, and reports one error there.
    let mut text = "var.met".to_owned();
    for _ in 0..MAX_NESTING {
        text = format!("var.met || var.met && 1 + ({text}) == 1");
    }
    let errors = errs(&text);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].message().contains("`int` and `bool`"));
}

// ---------------------------------------------------------------------------
// Registry

#[test]
fn registry_rules() {
    let mut registry = FunctionRegistry::with_core();
    for name in ["day", "hour", "minute"] {
        let f = registry.get(name).unwrap();
        assert_eq!(f.args, [Type::Time]);
        assert_eq!(f.ret, Type::Int);
        assert!(f.owner.is_engine());
        assert!(f.reads.is_empty());
    }
    assert!(FunctionRegistry::new().get("hour").is_none());
    let names: Vec<&str> = registry.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["day", "hour", "minute"]);

    registry
        .register(test_fn("test.a.b", vec![], Type::Bool, &[], always_true))
        .unwrap();
    assert_eq!(
        registry.register(test_fn("test.a.b", vec![], Type::Int, &[], always_true)),
        Err(RegistryError::Duplicate {
            name: "test.a.b".to_owned(),
            owner: Owner::module("test").unwrap()
        })
    );
    let with_owner = |name: &str, owner: Owner| Function {
        owner,
        ..test_fn(name, vec![], Type::Bool, &[], always_true)
    };
    let bad = [
        ("has", Owner::engine()),
        ("has", Owner::module("test").unwrap()),
        ("hour", Owner::engine()),
        ("rolls.passed", Owner::module("test").unwrap()),
        ("test", Owner::module("test").unwrap()),
        ("test.", Owner::module("test").unwrap()),
        ("testy.a", Owner::module("test").unwrap()),
        ("test.A", Owner::module("test").unwrap()),
        ("a.b", Owner::engine()),
        ("Day", Owner::engine()),
        ("game.thing", Owner::game()),
    ];
    for (name, owner) in bad {
        assert!(
            registry.register(with_owner(name, owner)).is_err(),
            "{name}"
        );
    }
    registry
        .register(with_owner("test.1st", Owner::module("test").unwrap()))
        .unwrap();
    registry
        .register(with_owner("weekday", Owner::engine()))
        .unwrap();
    let err = registry
        .register(with_owner("rolls.passed", Owner::module("test").unwrap()))
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid function name `rolls.passed` for module `test`: functions of module `test` \
         must be named `test.<name>`"
    );
    let err = registry
        .register(with_owner("x", Owner::game()))
        .unwrap_err();
    assert!(err.to_string().contains("only modules and the engine"));
}

// ---------------------------------------------------------------------------
// Evaluation

#[test]
fn and_or_short_circuit() {
    let s = state();
    assert!(!eval("false && test.fails()", &s));
    assert!(eval("true || test.fails()", &s));
    assert!(!eval("var.met && test.fails()", &s));
    assert!(!eval("true && false && test.fails()", &s));
    for text in ["true && test.fails()", "false || test.fails()"] {
        let (result, _) = eval_in(text, &s);
        let Err(EvalError::Function { name, span, error }) = result else {
            panic!("{text}: {result:?}");
        };
        assert_eq!(name, "test.fails");
        assert_eq!(span, span_of(text, "test.fails()", 0));
        assert_eq!(error, FunctionError::Failed("boom".to_owned()));
    }
}

#[test]
fn int_and_duration_overflow_saturates() {
    let mut s = state();
    s.set(&key("var.trust"), Value::Int(i64::MAX)).unwrap();
    s.set(
        &key("var.delay"),
        Value::Duration(Duration::from_minutes(i64::MIN)),
    )
    .unwrap();
    for (text, expected, kind, at) in [
        (
            "var.trust + 1 == 9223372036854775807",
            true,
            WarningKind::IntOverflow,
            "var.trust + 1",
        ),
        (
            "-var.trust - 2 == -9223372036854775807 - 1",
            true,
            WarningKind::IntOverflow,
            "-var.trust - 2",
        ),
        (
            "var.delay - 1m == var.delay",
            true,
            WarningKind::DurationOverflow,
            "var.delay - 1m",
        ),
        (
            "-var.delay > 0m",
            true,
            WarningKind::DurationOverflow,
            "-var.delay",
        ),
    ] {
        let (result, warnings) = eval_in(text, &s);
        assert_eq!(result, Ok(expected), "{text}");
        assert_eq!(warnings.len(), 1, "{text}: {warnings:?}");
        assert_eq!(warnings[0].kind(), kind, "{text}");
        assert_eq!(warnings[0].span(), span_of(text, at, 0), "{text}");
    }
    // Negating i64::MIN saturates at i64::MAX.
    s.set(&key("var.trust"), Value::Int(i64::MIN)).unwrap();
    let (result, warnings) = eval_in("-var.trust == 9223372036854775807", &s);
    assert_eq!(result, Ok(true));
    assert_eq!(warnings[0].kind(), WarningKind::IntOverflow);
    assert_eq!(
        warnings[0].to_string(),
        "integer overflow; saturated at the limit"
    );
}

#[test]
fn times_clamp_and_saturate() {
    let mut s = state();
    let text = "time - 1d == d1 00:00";
    let (result, warnings) = eval_in(text, &s);
    assert_eq!(result, Ok(true));
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].kind(), WarningKind::TimeBeforeStart);
    assert_eq!(warnings[0].span(), span_of(text, "time - 1d", 0));
    assert_eq!(
        warnings[0].to_string(),
        "time before `d1 00:00`; clamped to `d1 00:00`"
    );

    // Clamped as soon as it happens, so adding it back doesn't restore it.
    let (result, warnings) = eval_in("time - 1d + 1d == d2 00:00", &s);
    assert_eq!((result, warnings.len()), (Ok(true), 1));

    s.set(&Key::core_time(), Value::Time("d2 00:00".parse().unwrap()))
        .unwrap();
    s.set(
        &key("var.delay"),
        Value::Duration(Duration::from_minutes(i64::MAX)),
    )
    .unwrap();
    let (result, warnings) = eval_in("time + var.delay > time", &s);
    assert_eq!(result, Ok(true));
    assert_eq!(warnings[0].kind(), WarningKind::TimeOverflow);
    let (result, warnings) = eval_in("var.delay + time > time", &s);
    assert_eq!(
        (result, warnings[0].kind()),
        (Ok(true), WarningKind::TimeOverflow)
    );
    let (result, warnings) = eval_in("time - -var.delay > time", &s);
    assert_eq!(
        (result, warnings[0].kind()),
        (Ok(true), WarningKind::TimeOverflow)
    );
    let (result, warnings) = eval_in("time - var.delay == d1 00:00", &s);
    assert_eq!(
        (result, warnings[0].kind()),
        (Ok(true), WarningKind::TimeBeforeStart)
    );
    // Time minus time is negative when the first is earlier, without a
    // warning.
    assert!(eval("var.deadline - time == -1d", &s));
}

#[test]
fn undeclared_reads_are_errors() {
    let (result, _) = eval_in("test.sneaky()", &state());
    let Err(EvalError::Function { name, error, .. }) = &result else {
        panic!("{result:?}");
    };
    assert_eq!(name, "test.sneaky");
    assert_eq!(
        error,
        &FunctionError::UndeclaredRead {
            key: key("test.secret")
        }
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "function `test.sneaky` failed: read key `test.secret`, which it doesn't declare"
    );
}

#[test]
fn wrong_schema_is_an_error() {
    let checked = ok("var.met");
    let mut other = builder();
    other
        .add_key(key("var.extra"), Type::Int, Value::Int(0))
        .unwrap();
    let other_state = State::new(&other.freeze());
    let mut warnings = Vec::new();
    assert_eq!(
        checked.eval(&other_state, &functions(), &mut warnings),
        Err(EvalError::WrongSchema)
    );
    // A clone of the same schema is the same schema.
    let frozen = schema();
    let checked = compile_condition("var.met", &frozen, &functions()).unwrap();
    assert_eq!(
        checked.eval(&State::new(&frozen.clone()), &functions(), &mut warnings),
        Ok(false)
    );
}

#[test]
fn separately_built_identical_schema_evaluates() {
    let condition_schema = schema();
    let state_schema = schema();
    let checked = compile_condition("var.met", &condition_schema, &functions()).unwrap();
    let mut state = State::new(&state_schema);
    state.set(&key("var.met"), Value::Bool(true)).unwrap();
    let mut warnings = Vec::new();
    assert_eq!(checked.eval(&state, &functions(), &mut warnings), Ok(true));
    assert!(warnings.is_empty());
}

#[test]
fn registry_mismatches_are_errors() {
    let checked = ok("test.passed(check.a)");
    let s = state();
    let mut warnings = Vec::new();
    let empty = FunctionRegistry::with_core();
    assert!(matches!(
        checked.eval(&s, &empty, &mut warnings),
        Err(EvalError::UnknownFunction { name, .. }) if name == "test.passed"
    ));
    let mut changed = FunctionRegistry::with_core();
    changed
        .register(test_fn(
            "test.passed",
            vec![Type::Ref(kind("check"))],
            Type::Bool,
            &[],
            test_passed,
        ))
        .unwrap();
    assert!(matches!(
        checked.eval(&s, &changed, &mut warnings),
        Err(EvalError::FunctionChanged { .. })
    ));
    let (result, _) = eval_in("test.bad_return()", &s);
    assert_eq!(
        result,
        Err(EvalError::BadReturn {
            name: "test.bad_return".to_owned(),
            span: Span::new(0, 17),
            expected: Type::Bool,
            found: Box::new(Value::Int(1)),
        })
    );
}

#[test]
fn functions_get_their_arguments() {
    let mut s = state();
    assert!(eval("test.max(3, -7) == 3", &s));
    assert!(!eval("test.near(none, true)", &s));
    assert!(eval("test.near(npc.a, true)", &s));
    assert!(!eval("test.near(case.accused, true)", &s));
    s.set(&key("case.accused"), Value::Ref(Some(id("npc.b"))))
        .unwrap();
    assert!(eval("test.near(case.accused, true)", &s));
    assert!(eval("test.mode_is(:walk)", &s));
    assert!(!eval("test.mode_is(:car)", &s));
}

#[test]
fn set_equality() {
    let mut s = state();
    assert!(eval("game.met == game.met", &s));
    s.insert(&key("game.met"), id("npc.a")).unwrap();
    assert!(!eval("game.met != game.met", &s));
}

// ---------------------------------------------------------------------------
// Property test: well-typed random conditions never panic or fail.

/// Builds condition text of a given type from a stream of random choices.
struct Gen<'a> {
    choices: std::slice::Iter<'a, u32>,
}

const INTS: [i64; 6] = [0, 1, 7, 1440, i64::MAX - 1, i64::MAX];
const MINUTES: [i64; 6] = [0, 1, 59, 4200, i64::MAX - 1000, i64::MAX];

impl Gen<'_> {
    fn pick(&mut self, n: usize) -> usize {
        self.choices.next().map_or(0, |&c| c as usize % n)
    }

    fn one<'s>(&mut self, items: &[&'s str]) -> &'s str {
        items[self.pick(items.len())]
    }

    fn int(&mut self, depth: u32) -> String {
        let choice = if depth == 0 {
            self.pick(3)
        } else {
            self.pick(9)
        };
        match choice {
            0 => INTS[self.pick(INTS.len())].to_string(),
            1 => "var.trust".to_owned(),
            2 => format!("hour({})", self.time(0)),
            3 => format!("({} + {})", self.int(depth - 1), self.int(depth - 1)),
            4 => format!("({} - {})", self.int(depth - 1), self.int(depth - 1)),
            5 => format!("-({})", self.int(depth - 1)),
            6 => format!("test.max({}, {})", self.int(depth - 1), self.int(depth - 1)),
            7 => format!("day({})", self.time(depth - 1)),
            _ => format!("minute({})", self.time(depth - 1)),
        }
    }

    fn time(&mut self, depth: u32) -> String {
        let choice = if depth == 0 {
            self.pick(3)
        } else {
            self.pick(6)
        };
        match choice {
            0 => Time::from_minutes(MINUTES[self.pick(MINUTES.len())])
                .unwrap_or_default()
                .to_string(),
            1 => "time".to_owned(),
            2 => "var.deadline".to_owned(),
            3 => format!("({} + {})", self.time(depth - 1), self.duration(depth - 1)),
            4 => format!("({} - {})", self.time(depth - 1), self.duration(depth - 1)),
            _ => format!("({} + {})", self.duration(depth - 1), self.time(depth - 1)),
        }
    }

    fn duration(&mut self, depth: u32) -> String {
        let choice = if depth == 0 {
            self.pick(2)
        } else {
            self.pick(6)
        };
        match choice {
            0 => Duration::from_minutes(MINUTES[self.pick(MINUTES.len())]).to_string(),
            1 => "var.delay".to_owned(),
            2 => format!(
                "({} + {})",
                self.duration(depth - 1),
                self.duration(depth - 1)
            ),
            3 => format!(
                "({} - {})",
                self.duration(depth - 1),
                self.duration(depth - 1)
            ),
            4 => format!("({} - {})", self.time(depth - 1), self.time(depth - 1)),
            _ => format!("-({})", self.duration(depth - 1)),
        }
    }

    fn boolean(&mut self, depth: u32) -> String {
        const LEAVES: [&str; 16] = [
            "true",
            "false",
            "var.met",
            "flag.a",
            "flag.b",
            "has(clue.a)",
            "test.passed(check.a)",
            "test.passed(case.step)",
            "test.near(case.accused, flag.a)",
            "case.accused == npc.a",
            "case.accused != none",
            "case.accused == case.witness",
            "travel.mode == :walk",
            ":car != travel.mode",
            "test.mode_is(:bicycle)",
            "game.met == game.met",
        ];
        const CMP: [&str; 6] = ["==", "!=", "<", "<=", ">", ">="];
        let choice = if depth == 0 { 0 } else { self.pick(8) };
        match choice {
            0 => self.one(&LEAVES).to_owned(),
            1 => format!("!({})", self.boolean(depth - 1)),
            2 => format!(
                "({} && {})",
                self.boolean(depth - 1),
                self.boolean(depth - 1)
            ),
            3 => format!(
                "({} || {})",
                self.boolean(depth - 1),
                self.boolean(depth - 1)
            ),
            4 => {
                let op = self.one(&CMP);
                format!("({} {op} {})", self.int(depth - 1), self.int(depth - 1))
            }
            5 => {
                let op = self.one(&CMP);
                format!("({} {op} {})", self.time(depth - 1), self.time(depth - 1))
            }
            6 => {
                let op = self.one(&CMP);
                format!(
                    "({} {op} {})",
                    self.duration(depth - 1),
                    self.duration(depth - 1)
                )
            }
            // `!` first, so the two sides are never both bare IDs.
            _ => format!(
                "((!({})) == ({}))",
                self.boolean(depth - 1),
                self.boolean(depth - 1)
            ),
        }
    }
}

/// A random valid state of the test schema.
fn random_state(choices: &[u32]) -> State {
    let mut g = Gen {
        choices: choices.iter(),
    };
    let mut s = state();
    let ints = [i64::MIN, -1, 0, 1, 22, i64::MAX];
    let trust = ints[g.pick(ints.len())];
    s.set(&key("var.trust"), Value::Int(trust)).unwrap();
    s.set(&key("var.met"), Value::Bool(g.pick(2) == 1)).unwrap();
    for k in ["core.time", "var.deadline"] {
        let m = MINUTES[g.pick(MINUTES.len())];
        s.set(&key(k), Value::Time(Time::from_minutes(m).unwrap()))
            .unwrap();
    }
    let delay = ints[g.pick(ints.len())];
    s.set(
        &key("var.delay"),
        Value::Duration(Duration::from_minutes(delay)),
    )
    .unwrap();
    for (set, members) in [
        ("game.flags", ["flag.a", "flag.b"]),
        ("game.clues", ["clue.a", "clue.b"]),
        ("game.met", ["npc.a", "npc.b"]),
        ("test.passed", ["check.a", "check.b"]),
    ] {
        for member in members {
            if g.pick(2) == 1 {
                s.insert(&key(set), id(member)).unwrap();
            }
        }
    }
    let npcs = [None, Some(id("npc.a")), Some(id("npc.b"))];
    for k in ["case.accused", "case.witness"] {
        s.set(&key(k), Value::Ref(npcs[g.pick(3)].clone())).unwrap();
    }
    let steps = [None, Some(id("check.a"))];
    s.set(&key("case.step"), Value::Ref(steps[g.pick(2)].clone()))
        .unwrap();
    let modes = ["walk", "bicycle", "car"];
    s.set(&key("travel.mode"), Value::Enum(sym(modes[g.pick(3)])))
        .unwrap();
    s
}

proptest! {
    #[test]
    fn well_typed_conditions_evaluate_without_panicking(
        expr_choices in prop::collection::vec(any::<u32>(), 0..200),
        state_choices in prop::collection::vec(any::<u32>(), 20),
        depth in 0u32..5,
    ) {
        let text = Gen { choices: expr_choices.iter() }.boolean(depth);
        let checked = compile_condition(&text, &schema(), &functions())
            .map_err(|e| TestCaseError::fail(format!("{text}: {e:?}")))?;
        let state = random_state(&state_choices);
        let mut warnings = Vec::new();
        let result = checked.eval(&state, &functions(), &mut warnings);
        prop_assert!(result.is_ok(), "{}: {:?}", text, result);
        for warning in &warnings {
            prop_assert!(warning.span().end <= text.len());
        }
    }

    #[test]
    fn arbitrary_parsed_trees_check_without_panicking(text in "[ -~]{0,48}") {
        if let Ok(expr) = parse_condition(&text)
            && let Err(errors) = check_condition(&expr, &schema(), &functions())
        {
            prop_assert!(!errors.is_empty());
            for e in &errors {
                prop_assert!(e.span().end <= text.len());
            }
        }
    }
}

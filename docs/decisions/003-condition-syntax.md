# 003. Condition syntax

- **Status:** Accepted
- **Date:** 2026-11-04

## Context

`docs/spec.md` ("Expression language") sketches the condition grammar but
leaves the lexical details open: how negative numbers, times and durations are
tokenized, what a path may look like, whether comparisons chain, and how deep
expressions may nest. [ADR-001](001-core-value-model.md) removed string values
and [ADR-002](002-identifier-and-literal-syntax.md) fixed identifier and
literal formats; the parser in `asi_expr` must apply them consistently. Content
is untrusted input (mods), so the parser must also survive hostile text.

## Decision

### Grammar

```
expr    := or
or      := and ( "||" and )*
and     := not ( "&&" not )*
not     := "!" not | cmp
cmp     := sum ( ( "==" | "!=" | "<" | "<=" | ">" | ">=" ) sum )?
sum     := unary ( ( "+" | "-" ) unary )*
unary   := "-" unary | primary
primary := literal | path | call | "(" expr ")"
call    := path "(" ( expr ( "," expr )* )? ")"
literal := int | "true" | "false" | time | duration | symbol
```

This differs from the spec's sketch in three ways: `sum` is built from `unary`
(so `-x` exists), `literal` has no `"string"`, and `call` spells out its
argument list. Note that `!` binds looser than comparison: `!a == b` is
`!(a == b)`.

### Operators
- **Comparisons don't chain.** `a < b < c` is a parse error whose hint says to
  combine comparisons with `&&`. Parentheses (`(a == b) == c`) are accepted;
  whether they type-check is the type checker's call.
- **Negative numbers come from unary minus**, not the lexer. There are no
  negative literal tokens, so `-1`, `- 1`, `-45m`, `x - 1`, `x-1` and `x -1`
  parse the same way regardless of spacing. Consequently `i64::MIN` cannot be
  written (its magnitude doesn't fit `i64`); this is accepted.

### Literals
- **int:** ASCII digits, no leading zeros except `0`, must fit `i64`. Out of
  range is an error, never a wrap. A digit run followed by `.` and a digit
  (`1.5`) is an error saying there are no fractional numbers.
- **time:** `dN HH:MM` as in ADR-002, lexed as one token: an identifier of the
  form `d` plus digits, followed by exactly one space and a digit, starts a
  time literal, which runs over the following digits and colons and is then
  validated by `asi_core` (so `d0 10:00` and `d1 9:00` get core's messages). A
  bare `HH:MM` is an error saying to write `dN HH:MM` and that time-of-day
  checks will be functions; if it follows `dN` with the wrong whitespace, the
  hint says to use exactly one space.
- **duration:** ADR-002's format without the sign (`15m`, `1h30m`, `2d`). A
  word that starts with a digit and isn't all digits is a duration.
- **symbol:** `:name`, where `name` is one name segment.
- **Quoted strings** (`"..."` or `'...'`) are rejected with a hint pointing to
  ADR-001: use `:symbols` or IDs.

### Paths and calls
- A path is one token: segments joined by `.` with **no whitespace around the
  dots**. The first segment is a name segment and later ones are path segments
  (ADR-002), so `line.case1.carla.042` is one path.
- The single words `true` and `false` are boolean literals, never paths.
  Longer paths may start with them (`true.x`); the schema decides whether
  such a namespace exists.
- The parser does not decide whether a path is a state key, an ID or a
  function. A path whose next token is `(` is a call; whitespace before the
  `(` is allowed, since nothing else could follow a path there. Canonical
  text has no space.

### Whitespace and comments
- Whitespace is space, tab and newline; `\r` is also accepted so CRLF line
  endings work. Conditions may span lines. There are no comments.

### Nesting limit
- Parentheses, `!`, unary `-` and call argument lists each add one level of
  nesting. More than 64 levels is an error, so hostile input can't overflow
  the stack.

### Syntax tree
- Every node carries a byte span into the expression text. A parenthesized
  expression is its inner node with the span widened over the parentheses.
- Chains of `||`, `&&` and `+`/`-` are stored **flat** (a list of operands,
  evaluated left to right) rather than as nested binary nodes. A chain of
  any length then adds only one level to the tree, so the tree's depth is
  bounded by the nesting limit, and every later pass (type checker,
  evaluator, printer, `Drop`) can recurse without risk of a stack overflow.
  `a - b - c` is one `Sum` meaning `(a - b) - c`; `(a - b) - c` written with
  parentheses is a `Sum` whose first operand is a `Sum`.
- Canonical printing uses the fewest parentheses that reproduce the same
  tree, so `(a && b) && c` keeps its parentheses.

### Errors
- The parser reports the first error by position (the lexer is lazy), with a
  span, a message and an optional hint. A helper turns a byte offset into a
  1-based line and column (columns counted in characters) within the
  expression text; the content loader adds the file and the expression's
  position.

## Consequences

- Writers can't put spaces around the dots of a path (`flag . x`), or use
  more than one space inside a time literal.
- `asi_core::ident` exposes `name_segment_error` and `path_segment_error` so
  the parser applies exactly the identifier rules of ADR-002.
- The evaluator must fold `Sum`, `And` and `Or` lists left to right (and
  short-circuit `&&`/`||`).
- Time-of-day checks still need functions over the current time (ADR-002).
- If the language later gains `*` or `/`, a new level goes between `sum` and
  `unary`, and the flat-chain rule should apply to it too.

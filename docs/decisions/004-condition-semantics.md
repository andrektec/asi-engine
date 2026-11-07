# 004. Condition semantics, core time, functions and single-ID values

- **Status:** Accepted
- **Date:** 2026-11-05

## Context
ADR-003 fixed the condition syntax, but not what names mean, how the current time is
read, which operations exist, how modules add functions, or how a key can hold a single
content ID. The type checker and evaluator (Slice 3) need all of these.

## Decision

### 1. Holder sets and ID conditions
- The schema may mark one `set<kind>` key as the **holder** of that kind (e.g.
  `game.clues` for `clue`). At most one holder per kind.
- `has(id)` is true when `id` is in its kind's holder set. Calling it with an ID whose
  kind has no holder is a load error.
- An ID used on its own where a `bool` is expected means `has(id)`:
  `flag.met_marcos && x` is `has(flag.met_marcos) && x`. Both forms are equivalent.
- Otherwise IDs appear in conditions only as function arguments or in comparisons with
  single-ID values (section 7). Any other use (`clue.a + 1`) is a load error.
- The engine registers no ID kinds; games and modules do.

### 2. Core time
- The schema always contains the namespace `core` (owner: the engine) with the key
  `core.time` of type `time`, default `d1 00:00`. `core` is reserved: no module or
  game may declare it, and `core` can't be a module name.
- The single word `time` in a condition is `core.time`. No other single-word path is
  valid in a condition (except a call like `has(...)`).

### 3. Time of day
- Core functions `day(time) -> int`, `hour(time) -> int` and `minute(time) -> int`
  return the parts of a time (day starts at 1, hour 0–23, minute 0–59).

### 4. Types and operations
- `+` and `-`: int ± int → int; duration ± duration → duration;
  time ± duration → time; duration + time → time; time - time → duration.
  Unary `-`: int and duration only.
- `<`, `<=`, `>`, `>=`: both sides int, both time, or both duration.
- `==`, `!=`: both sides the same type. A symbol literal takes the enum type of the
  other side and must be one of its symbols. Two bare symbols can't be compared.
- `&&`, `||`, `!`: bool only. A condition's top level must be bool.
- Anything else is a load error with the span and a hint.
- At runtime, int and duration overflow saturate at the limit, and a time below
  `d1 00:00` clamps to it; each such case is reported as a warning. Evaluation never
  panics.
- `&&` and `||` evaluate left to right and short-circuit.

### 5. Function registry
- Modules (and the core) register functions with: a dotted name, argument types
  (value types or `id<kind>`), a return type, the state keys it reads, and pure Rust
  code. Module function names start with the module name (`rolls.passed`); core
  function names are one word (`day`, `hour`, `minute`). `has` is a built-in form,
  not a registry function, because which key it reads depends on its argument.
- Function code gets a read-only view of the state limited to its declared keys;
  reading any other key is an error. It may not use the clock, randomness, IO or
  anything outside its arguments and that view.
- A checked condition lists every key it reads (directly, via `has`, or via
  functions) and every ID it mentions, for the analyzer and the content loader.

### 6. Module state is shaped by the questions conditions ask
- Module state holds what conditions test (finite: bools, enums, sets, single IDs,
  ints, times), not logs. A module that records history for display keeps it out of
  anything a condition or function reads, so the analyzer can still reason about the
  game. Example: `rolls` keeps `rolls.passed`, `rolls.failed` and `rolls.locked` as
  sets of checks, plus an optional display-only history.

### 7. Single-ID values
- New type `id<kind>`: holds one ID of that kind, or `none`. Default is usually `none`.
  Example: `case.accused: id<npc>`.
- `none` is a keyword literal (like `true`/`false`); the bare word `none` is never a path.
- `==` and `!=` compare an `id<kind>` value with an ID of the same kind, with `none`, or
  with another `id<kind>` of the same kind. Comparing with an ID of another kind, or
  `none` with something that isn't `id<kind>`, is a load error. No other operations.
- An ID compared this way is a value, not `has()`: `case.accused == npc.carla` does not
  check any holder set.

## Consequences
- Story files can write `flag.met_marcos`, `time >= d3 22:00` and
  `case.accused == npc.carla` as in the spec and design doc.
- Time-of-day checks read `hour(time) >= 22`.
- The content loader must verify that every ID a condition mentions is defined.
- `asi_core` gains holder sets, the `core` namespace and the `id<kind>` type; the
  condition parser gains the `none` literal.
- The analyzer abstracts an `id<kind>` key as one choice among the IDs conditions
  compare it against, plus "other" and `none`.

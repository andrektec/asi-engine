# 002. Identifier syntax, literal formats and schema details

- **Status:** Accepted
- **Date:** 2026-11-04

## Context

[ADR-001](001-core-value-model.md) fixed the value model but left the exact
syntax of identifiers and time literals, and some schema details, to the
implementation of `asi_core` (Slice 1). These choices show up in content files,
saves and error messages, so they should be written down once.

This record also supersedes one line of ADR-001 §2: the bare `HH:MM` time
literal meaning day 1 (see "Time and duration literals" below).

## Decision

### Identifiers
- Names are ASCII only and lowercase, with no length limit. There are two
  segment shapes:
  - A **name segment** matches `[a-z][a-z0-9_]*` (starts with a letter).
  - A **path segment** matches `[a-z0-9][a-z0-9_]*` (may start with a digit).
- Symbols, ID kinds, namespaces and module names are one name segment.
- IDs are `kind.path` and state keys are `namespace.name`, where `path` and
  `name` are dotted paths of one or more path segments
  (`scene.case1.diner.arrive`, `travel.npc.carla`). So the first segment is
  always letter-first and later ones may start with a digit:
  `line.case1.carla.042` is valid, `042.carla` is not.
- Namespaces and ID kinds share one name space: declaring a namespace with the
  name of a registered ID kind is an error, so `clue.x` can never be read as
  both a key and an ID.
- Symbols are stored without the colon; their text form is `:name`.
- `Id` and `Key` order by their first segment, then the rest, compared bytewise.

### Time and duration literals
- `Time` is never negative: game start is the earliest time. Arithmetic that
  would go before it returns `None`, like overflow does.
- Time literals are strict: `dN HH:MM` with exactly one space, no leading zeros
  in `N`, and two-digit hour and minute. This is the only form; Display writes
  it too.
- **Supersedes ADR-001 §2's `HH:MM` line.** A bare `HH:MM` (meaning day 1) is
  not a time literal. Day 1 is rarely what a writer means by `22:00`; usually
  they mean "22:00 on whatever day it is", which is a time-of-day check, not a
  point in time. Those checks will come from the expression language (e.g. a
  function over the current time) instead.
- Durations may be negative. A duration literal is an optional `-` and one or
  more `<number><unit>` components, units `d`, `h`, `m` each at most once and in
  that order (`1d2h30m`). Numbers have no leading zeros and may exceed their
  unit's range (`90m`). Display is canonical (`90m` shows as `1h30m`, zero as
  `0m`), and every displayed value parses back to the same value.
- `Time - Time` is infallible (`Time::duration_since`), because two
  non-negative `i64`s always have a difference that fits.

### Schema
- An owner is either the game or a module with a one-segment name; `game` is
  reserved and can't be a module name.
- ID kinds are registered, all of them, before keys are declared:
  `StateSchema::new` takes the finished `KindRegistry` and checks `set<kind>`
  types as each key is declared.
- An empty set matches every set type. An enum type with no symbols is allowed
  but no default can match it, so declaring a key with one always fails.

## Consequences

- Content is lowercase-only for names. Fluent line IDs (which allow hyphens)
  aren't IDs in this sense; they stay content keys, per ADR-001.
- A game can't model events before its start time; it picks a later start day
  instead.
- The expression lexer (Slice for `asi_expr`) must tokenize combined
  durations like `1h30m` and negative literals consistently with this ADR.
- The expression language needs a way to test time of day, since `HH:MM` alone
  is no longer a literal.
- ID paths can carry numbered parts (`line.case1.carla.042`) without a letter
  prefix.

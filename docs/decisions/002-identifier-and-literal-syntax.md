# 002. Identifier syntax, literal formats and schema details

- **Status:** Proposed
- **Date:** 2026-11-04

## Context

[ADR-001](001-core-value-model.md) fixed the value model but left the exact
syntax of identifiers and time literals, and some schema details, to the
implementation of `asi_core` (Slice 1). These choices show up in content files,
saves and error messages, so they should be written down once.

## Decision

### Identifiers
- A **segment** matches `[a-z][a-z0-9_]*`: ASCII only, lowercase, starting with
  a letter. No length limit.
- Symbols, ID kinds, namespaces and module names are one segment.
- IDs are `kind.path` and state keys are `namespace.name`, where `path` and
  `name` are dotted paths of one or more segments (`scene.case1.diner.arrive`,
  `travel.npc.carla`).
- Symbols are stored without the colon; their text form is `:name`.
- `Id` and `Key` order by their first segment, then the rest, compared bytewise.

### Time and duration literals
- `Time` is never negative: game start is the earliest time. Arithmetic that
  would go before it returns `None`, like overflow does.
- Time literals are strict: `dN HH:MM` with exactly one space, no leading zeros
  in `N`, and two-digit hour and minute. `HH:MM` means day 1. Display always
  uses `dN HH:MM`.
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

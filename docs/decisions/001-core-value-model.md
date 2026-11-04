# 001. Core value model, time and IDs

- **Status:** Accepted
- **Date:** 2026-11-04

## Context
M0 needs a fixed value model before `asi_core` and `asi_expr` are written. The spec
left four points open: string values, time representation, how time moves, and ID typing.

## Decision

### 1. No string values
- Game state and expressions have no string type.
- Values from a closed set are enums written as `:symbol` (`travel.mode == :bicycle`).
- Content entities are referenced by dotted IDs (`clue.receipt_photo`).
- Quoted text appears only as content keys (e.g. Fluent line IDs), never as a value.
- Free-text player input is matched by the owning module, which then sets enums/flags.
- Reason: every value domain stays finite and known, so the analyzer can reason about it.

### 2. Time is integer minutes
- `time` is an `i64` count of minutes since game start; `duration` is an `i64` of minutes.
- Literals: `dN HH:MM` with days starting at 1 (`d1 00:00` == 0), `HH:MM` meaning day 1,
  and durations `15m`, `2h`, `1d`.
- Calendars (weekdays, dates) are not core; a module or the game maps day numbers.

### 3. Time can move in both directions; events fire on forward crossing
- Any effect may set or advance time, forwards or backwards.
- Moving time from `old` to `new` with `new > old` fires every scheduled event with
  `old < at <= new`, in order of `at`, then event ID. Events scheduled while processing are
  included if they fall in the range. Moving backwards fires nothing.
- Scheduled events remain on the timeline after firing until explicitly cancelled, so
  crossing their time again fires them again.
- Moving time does not undo state; re-firing reapplies the event's effects.
- An event's own effects may not move time backwards (load error), preventing infinite
  re-fire loops.

### 4. Typed IDs
- Every ID has a kind given by its prefix (`clue`, `flag`, `npc`, `location`, ...).
- Kinds are registered by modules (in Rust) or declared by the game (in content), like
  state schemas.
- Sets are typed by kind (`set<clue>`); mixing kinds is a load-time type error.
- Referencing an ID that no content defines is a load error with file and line.

## Consequences
- The grid example in the ASI design doc must use IDs/symbols instead of `"23h"`.
- The analyzer can treat time as intervals but must allow revisiting earlier intervals.
- Content typos surface at load, not at runtime.

# ASI Engine — Technical Spec

Nov 4, 2026 · @LW

## Purpose and principles

The ASI Engine is an open-source Rust engine for branching, plot-heavy terminal games, built first for ASI (closed source, specified in the [ASI design doc](https://claude.ai/artifact/1AjhQC1VapVgEpbSQQhbiV)) and usable for any other game, the way Source was built for Half-Life 2.

1. **ASI drives the features.** The engine supports every ASI feature natively, but nothing ASI-specific lives in it. Generalize only when a real second use appears.
2. **One-way dependency.** The engine never imports game code. ASI is just the first game on it.
3. **Everything optional is a module.** A game enables only what it needs. A game without any OS integration is a valid engine game.
4. **Rules are declarative.** Conditions and effects use one analyzable expression language, never embedded scripting, so tools can reason about every game.
5. **Deterministic by construction.** The same save plus the same inputs always gives the same result.
6. **Open by default.** Content is loose, readable files, easy to mod and to datamine, like RPG Maker.
7. **The engine can change during ASI's development.** No API stability promise until ASI ships.

## Layers at a glance

&#91;embedded content: engine layers · games, standard modules, core, SDK tools\]

Each layer depends only on the layers below it: games choose modules, modules extend the core, and the SDK tools link the same core and modules the games run on.

## Ways to make a game

There are three ways to build on the engine, and all three run the same core and the same analyzer.

| Kind | What it is | Needs | Can add |
| --- | --- | --- | --- |
| Code + data game | A Rust crate depending on the engine, compiled into the game's own binary | Rust toolchain | Custom modules, node types, tool windows, presentation |
| Data-only game | A game folder (story files, strings, assets, a manifest) run by the engine's ready-made **player** binary | No programming | Anything the standard modules offer |
| Mod | A data overlay loaded on top of an existing game | No programming | New or replaced content by ID |

- **ASI is a code + data game.**
- **The engine's demo game is data-only.** It proves the boundary and doubles as the reference for the player binary.
- **No runtime code plugins.** Rust has no stable plugin ABI, so code extensions are compiled in. WASM plugins are the possible path if code-level mods are ever wanted.
- **Custom node types stay analyzable.** A game defines them as compositions of engine expressions. Opaque Rust logic is allowed only where it can't affect story reachability (presentation, tool UI).

## Core

The core is everything every game needs: state, story graph, time, randomness, content, text, sound and error handling. It has no terminal or OS dependency, so the analyzer, tests and bots can run it headless.

**State.** A single typed store, namespaced per module (`core.*`, `grids.*`, `travel.*`, game namespaces).

- Value types: `bool`, `int`, `enum` (a closed set of symbols), `set<id>`, `time`, `duration`. No floats in game logic.
- Modules declare their state schema at registration. Unknown keys are a load error.
- Ordered containers only (`BTreeMap`, `IndexMap`), so iteration order never varies between runs.

**Story graph.** Scenes contain nodes. A node has an optional `when` condition, content (line IDs) and choices. A choice has `when`, `effects`, `time_cost` and `goto`. Endings are terminal nodes. Modules add node kinds (a grid submission, a travel action) built from the same parts.

**Clock and events.** Game time is a state value, never the wall clock. Every action advances it by its cost. Scheduled events fire when time crosses their threshold, checked after every action, in a stable order.

**Randomness.** One seeded RNG stream (ChaCha) whose state lives in the save. Modules draw from named sub-streams so adding a roll in one quest doesn't shift rolls elsewhere.

**Content loading.** Through a virtual file system (see Content format). All content is validated at load: schema, IDs, references, expression types.

**Localization.** Fluent (`fluent-rs`). Every line has a stable ID; text, voice file and subtitle timing are keyed by ID and locale, with a fallback chain (e.g. pt-BR → en).

**Audio.** A core service over `kira`: music with crossfades, SFX, voice lines with ducking. Cues are triggered by content, never by game logic timing.

**Errors.** Content errors fail at load with file and line. At runtime a release build never panics on bad state: it reports, clamps to a valid value and keeps at least one action available.

## Expression language

One small, typed, side-effect-free language for conditions, plus a separate effect syntax for changes; both are parsed and type-checked at load, so a typo in content is a load error, not a runtime bug.

**Conditions (grammar sketch).**

```
expr    := or
or      := and ( "||" and )*
and     := not ( "&&" not )*
not     := "!" not | cmp
cmp     := sum ( ( "==" | "!=" | "<" | "<=" | ">" | ">=" ) sum )?
sum     := primary ( ( "+" | "-" ) primary )*
primary := literal | path | call | "(" expr ")"
literal := int | bool | "string" | time | duration | :symbol
path    := ident ( "." ident )*          # flag.met_marcos, travel.mode
call    := path "(" args? ")"           # has(clue.receipt), rolls.passed(check.empathy_01)
```

- Time literals: `22:00`, `d3 22:00`. Durations: `15m`, `2h`.
- No loops, no user-defined functions, no variables. Every expression terminates.
- Functions are registered by modules with typed signatures and the state keys they read.

**Effects.** A list of statements, applied in order:

```yaml
effects:
  - set flag.met_marcos
  - give clue.receipt_photo
  - var.trust_carla += 1
  - time += 15m
  - schedule event.witness_leaves at d2 23:00
```

Modules register effect verbs the same way, declaring the keys they write.

**Static checks at load:** types match, paths exist in some module's schema, enum symbols are valid, and functions are called with correct argument types.

## Content format

YAML is the first file format for one canonical data model; other frontends may compile to the same model later, and every loader reads through a virtual file system so archives and mods slot in without touching game code.

**Game folder layout.**

```
my-game/
  game.yaml            # manifest: id, version, enabled modules, locales, entry node
  story/               # one file per scene; YAML now, script syntax later
  strings/pt-BR/*.ftl  # Fluent strings per locale
  strings/en/*.ftl
  assets/              # images, audio, documents, websites
  voice/pt-BR/...      # voice files keyed by line ID
```

**IDs.** Every scene, node, choice, line, clue, flag and asset has a stable, dotted ID (`case1.diner.arrive`). IDs never change once content ships, because saves, logs, voice files and translations reference them.

**Schema.** The engine publishes a JSON Schema generated from the Rust types (core plus enabled modules), so editors such as VS Code validate and autocomplete story files.

**Diff-friendly by rule.** One file per scene, deterministic key order, no generated noise. The node editor writes the same files a human would.

**Second frontend (later).** A lightweight script syntax for dialogue-heavy writing, in the spirit of ink or Yarn Spinner, compiling to the same model. The editor, analyzer and runtime never see the difference.

**Virtual file system.**

- Mount order: the base game, then mod folders in a declared order.
- A later mount can add content or replace an existing ID.
- Loose folders now; an archive mount type (like Source's VPKs) can be added to the SDK later without changing loaders.
- Files the OS must open are extracted to a real folder at runtime regardless of mount type.

## Module system

A module is a Rust crate implementing one `Module` trait; it registers everything it adds at startup, and must describe its state to the analyzer so every game stays analyzable.

**What a module registers.**

- **State schema:** its namespaced keys and their types.
- **Expression functions and effect verbs:** typed signatures plus the keys each one reads or writes.
- **Node kinds:** built from core parts (conditions, effects, choices), plus any outcome-classification logic.
- **Events:** what it emits and what it reacts to.
- **Tools:** tool views it provides (see Runtime).
- **Presentation hooks:** how its content is shown; opaque Rust is fine here.

**Analyzer contract.** For each state key, the module declares an abstraction the analyzer can use:

- **Booleans and enums:** exact.
- **Ints and time:** intervals between the constants that conditions actually compare against.
- **Sets:** one boolean per element that conditions test.
- **Custom:** a module-provided abstraction function, for state like a grid's submission (abstracted to its outcome tag).

If a module can't provide an abstraction, its keys are excluded from reachability analysis and the analyzer reports that, rather than silently guessing.

**Standard modules.** All optional, enabled per game in `game.yaml`:

| Module | Provides | Used by ASI |
| --- | --- | --- |
| `grids` | Logic-grid model, solver, verdict rules, easy/hard entry, per-route solvability, grid tool view | Yes |
| `travel` | Location graph, travel modes, routes and travel time, NPC schedules, map tool view | Yes |
| `rolls` | Skills, white (retryable) and red (one-shot) checks, roll history | Yes |
| `companion` | Assistant-style presenter: transcripts, suggested replies, narrator, cross-app listening popups, skill modules as plugin output | Yes |
| `osbridge` | Case folder, opening files in OS apps, local HTTP server for websites, file tools such as a decryptor | Yes |
| `devlayer` | Dev tools as gameplay: state editor, story graph viewer, completion tracker, launch-parameter overrides, real-shell escape, tamper detection, anomalies, second layer | Yes |
| `steam` | Achievements, Cloud saves, launch options | Yes |

`devlayer` runs in two modes: **SDK only** (present in development builds, absent from release) or **gameplay** (shipped as part of the game, as in ASI).

## Runtime and presentation

At runtime one process, the hub, owns the state and is its only writer; tool views render anywhere and change state only by sending actions to the hub.

**Hub and tool views.**

- A **tool view** is a ratatui component rendering into a rectangle it's given. It never assumes it has its own window.
- A **host** places views: in separate terminal windows (ASI's default), as panes inside the hub window (single-window tiled mode, for Steam Deck later and for development), or headless (tests, bots).
- Tool processes are the same binary in a tool mode (`game tool <name>`).

**IPC protocol.**

- Transport: a localhost socket opened by the hub, shared with the `osbridge` web server. A per-session random token, passed to spawned tools via an environment variable, authenticates every connection.
- Messages: JSON lines with a protocol version.
  - `subscribe(paths)`: the hub pushes state changes for those keys.
  - `action(id, args)`: a request to change state. The hub validates it, applies it and records it in the input stream.
  - `event(name, data)`: notifications such as "file decrypted" or "call started".
- Because every change passes through the hub's action queue, tool actions are part of the deterministic input stream.

**Terminal backends.**

- **Bundled WezTerm:** full feature set (fonts, palette, inline images, mouse). The SDK's launcher starts it with each game's terminal config.
- **Any terminal:** a degraded mode detected by capability (no inline images, fewer colors), for development and modders.
- Capabilities are queried once at startup and exposed to presentation code; game logic never depends on them.

**Input.** Keyboard and mouse through crossterm. The core provides a hit-testing registry rebuilt every frame and a mouse-capture toggle. Everything must also be playable by keyboard alone.

**Accessibility.** A core concern, not a module.

- Every view must provide an accessible text representation of what it shows.
- A screen-reader mode switches presentation to linear text with no box drawing or animation, and sends speech through an accessibility sink: Tolk on Windows (NVDA, JAWS, Narrator) and the platform speech APIs (`tts` crate) elsewhere.
- Font scaling, colorblind-safe palettes, a no-glitch option and subtitles are engine settings available to every game.

## Saves, determinism and replays

A save is the complete state plus the RNG state, and a replay is a save plus the recorded input stream; together they let any session be reproduced exactly.

**Determinism rules.**

- All randomness comes from the seeded RNG stored in the state.
- Game logic uses game time only, never the wall clock.
- Every change enters through the hub's action queue: player input, tool actions, scheduled events.
- Ordered containers only; no floats in game logic.
- Presentation (animations, audio timing) may use real time, because it never feeds back into state.

**Save format.**

- JSON, human-readable, with a header holding the engine version, game ID, game version, save-format version and enabled modules.
- A checksum over the body. A mismatch marks the save as externally edited, which `devlayer` can turn into gameplay.
- Versioned migrations: each save-format version has a migration to the next, so old saves keep loading.
- Autosave and manual slots are a core feature.

**Replays.** A replay file holds a starting save plus every action with its game-time stamp.

- **Bug reports:** players attach a replay, and you reproduce the issue exactly.
- **Regression tests:** recorded playthroughs re-run in CI, and any change in outcome fails the test.
- **Bot playtesting:** bots follow paths produced by the analyzer through the real runtime, headless, and every failure comes out as a replay.

## SDK tools

The SDK is the set of tools that ship with the engine; each links the same core and modules as the games, so tools and games can never disagree about the rules.

| Tool | What it does |
| --- | --- |
| **Analyzer** | Reachability over abstracted state: endings with example paths, unreachable content, softlocks, TODO edges, anomaly proofs (unreachable in normal play), hint reachability, escape guarantees for `devlayer`. Modular per quest. Exports graphs. |
| **Node editor** | Tauri + React Flow app. Reads and writes the story files. Shows analyzer results live, "play from here" with a forged state, grouping and search by scene, side-by-side locales. Generic: shows each game's custom node kinds from the module schema. |
| **Player** | The ready-made runtime for data-only games and mods. Also used to playtest any game's content in development. |
| **State editor** | Inspect and edit any save. The same tool `devlayer` can ship as gameplay. |
| **Replay runner and bots** | Re-run replays, run regression suites, drive bots along analyzer paths headless. |
| **Launcher** | Starts the bundled WezTerm with a game's terminal config. Used by the `steam` module or standalone. |
| **Packager** | Builds a release folder: content validation, schema check, asset extraction rules. Archive output later. |
| **Module checks** | Module-specific validators, such as `grids` checking that each puzzle has exactly one solution. |

All tools are command-line first, so they run in CI; the editor is the only GUI.

## Repository, crates and licensing

The engine lives in its own open-source repository as a Cargo workspace; ASI lives in a separate closed repository that depends on it.

| Crate | Contents |
| --- | --- |
| `asi_core` | State, story graph, clock and events, RNG, module registry, actions |
| `asi_expr` | Expression and effect parser, type checker, evaluator |
| `asi_content` | Virtual file system, YAML loading, validation, JSON Schema generation |
| `asi_l10n` | Fluent integration, line IDs, locale fallback |
| `asi_audio` | kira wrapper |
| `asi_ipc` | Hub ↔ tool protocol |
| `asi_tui` | ratatui views, hosts, hit-testing, accessibility sink |
| `asi_save` | Save format, checksum, migrations, replays |
| `asi_mod_grids`, `asi_mod_travel`, `asi_mod_rolls`, `asi_mod_companion`, `asi_mod_osbridge`, `asi_mod_devlayer`, `asi_mod_steam` | Standard modules |
| `asi_analyzer` | Analyzer library and CLI |
| `asi_player` | Data-only player binary |
| `asi_tools` | State editor, replay runner and bots, launcher, packager |
| `editor/` | Tauri + React Flow node editor |
| `examples/demo` | The data-only demo game |

- **License:** MIT/Apache-2.0 dual license, the Rust ecosystem convention, is the suggested default for the engine. ASI's own code and content stay closed.
- **ASI depends on the engine** by git revision during development, and by released versions later.

## Milestones and open questions

Each milestone ends with something runnable, and the data-only demo proves the engine-game boundary before any ASI content exists.

1. **M0 — Core skeleton:** `asi_core`, `asi_expr`, `asi_content`, `asi_save`. State, expressions, story graph, clock and events, RNG, YAML loading, saves. A headless text runner for tests.
2. **M1 — Analyzer and replays:** reachability, endings, softlocks, TODO report; replay recording and regression tests.
3. **M2 — Player and demo:** `asi_tui` with the hub host and a basic dialogue presenter, localization, audio. The data-only demo game playable end to end.
4. **M3 — Standard modules:** `rolls`, `grids`, `travel`, `companion`, each exercised by demo content.
5. **M4 — Tools and OS:** `asi_ipc`, tool windows, `osbridge`, the launcher with bundled WezTerm, `devlayer`.
6. **M5 — Editor and bots:** the node editor, bot playtesting along analyzer paths.
7. **Then ASI's vertical slice,** in the ASI repository, as defined in the design doc.

**Open questions.**

- [ ] Engine license: confirm MIT/Apache-2.0.
- [ ] Demo game concept: small, data-only, using a few standard modules.
- [ ] Crate naming: keep the `asi_` prefix, or give the engine its own name before going public?
- [ ] Second-frontend script syntax: design it once writing in YAML starts to hurt.
- [ ] Mod distribution: manual folders only, or Steam Workshop later.
- [ ] Real shell on Windows: PowerShell vs cmd (shared with the design doc).

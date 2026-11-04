# Architecture decision records

This folder records the design decisions behind the ASI Engine. Together with
[`docs/spec.md`](../spec.md), these records are authoritative: if code disagrees
with them, either the code is wrong or a new record is needed.

## When to write one

Write an ADR whenever a PR makes a design decision that the spec doesn't
already settle: a choice between approaches, a new dependency with
architectural weight, a change to the shape of a public API, or a reversal of
an earlier decision.

## Format

Each record is one Markdown file named `NNN-short-title.md`, where `NNN` is the
next free three-digit number. Start by copying [`000-template.md`](000-template.md). Each
record has:

- **Number and title:** in the heading, e.g. `# 007. Use IndexMap for state namespaces`.
- **Status:** `Proposed`, `Accepted`, `Rejected`, `Deprecated`, or
  `Superseded by NNN`.
- **Context:** the problem and the forces at play, in a few paragraphs.
- **Decision:** what we chose, stated plainly.
- **Consequences:** what becomes easier or harder, and any follow-up work.

Records are never deleted. To change a decision, write a new record and mark
the old one `Superseded by NNN`.

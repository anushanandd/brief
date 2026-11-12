# 0001: Where knowledge lives

2026-11-11 · Accepted

## Context

Brief's docs had grown to about 18,500 words. Most of `docs/architecture/` restated the code in prose: each detector's thresholds, each module's flow. Every behavior change needed a matching doc edit, and agents were told to read an architecture page before each change, though they worked out behavior from the code anyway. A 2026 study of repository context files for coding agents (Gloaguen et al., _Evaluating AGENTS.md_, arXiv 2602.11988) found they raise cost by about a fifth without a reliable gain in task success, that repository overviews don't help, and that what does help is conventions an agent can't infer.

## Decision

Each kind of knowledge lives where it stays true:

- How a module works: its header comment, changed in the same diff as the code. A value is named by its constant, not repeated.
- Rules every part keeps, and where things are: `ARCHITECTURE.md`, short and slow to change, naming modules rather than describing them.
- What decides a conflict: `PRINCIPLES.md`. How the interface looks and moves: `DESIGN.md`.
- What an agent can't infer and needs on every task (commands, rules no test catches, gotchas): `AGENTS.md`, kept short.
- Why a choice was made: a dated record here, which never needs updating.

Where a doc and the code disagree, the code wins. A rule that must always hold should become a test, lint, or hook rather than a sentence.

## Consequences

`docs/architecture/` is gone; its content moved into module headers or was cut where the code already said it. There is no single prose catalogue of what Brief computes: the README lists features, and each module says what it is for.

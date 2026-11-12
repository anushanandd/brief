# 0010: Rules are data, interpreted and recomputed whole

Recorded 2026-11-11 from the earlier architecture docs; measured October 2026 · Accepted

## Context

Brief's evidence rules (what a document, a deposit, or a record proves) include ones the user approves, so they must load at run time. Datalog crates such as `ascent` compile rules written in Rust, which can't load a rule from data.

## Decision

Rules are data (`evidence/programs.json`), checked before use (`Program::validate`), and run by a small interpreter (`evidence::engine`). Rules and chapters are recomputed whole on every read rather than kept incrementally: in October 2026 they took 1.6 ms of a 45 ms release-build derive, and the view is cached between reads.

## Consequences

No dependency, and approved rules work without a build. Incremental evidence can wait until a measurement says the derive is slow.

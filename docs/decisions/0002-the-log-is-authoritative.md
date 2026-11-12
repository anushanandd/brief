# 0002: The change log is authoritative

2026-10-16 · Accepted

## Context

Brief saved its finance state as tables, with a per-record log beside them. Either could be wrong after a bug, and nothing said which to believe. Brief runs on one Mac, but a second device (a phone, say) may want the same data later.

## Decision

Every write to the finance state is one change in an append-only log, committed in the same transaction as the state it produced and the hash of that state. The log has full authority: the saved tables are its cache, kept so a read never waits on replay. Tables that don't hash to the log's latest state are rebuilt by replay before any reader sees them.

The log is written so a second device could merge it later without a new format: each changeset names its parent, the device that wrote it, and its sequence number there, and its id is a hash of all of it. Sync itself isn't built. A second device would connect directly over an encrypted peer link (Tailscale), so there is still no server.

## Consequences

- An update that changes a calculation re-projects from the log and saves a checkpoint, so later opens compare cleanly.
- Evidence a provider later replaces (a pending charge, a dropped hold) stays in the log.
- A launch costs one hash, not a replay; replay is checked in the background and reported by Data health.
- Only a log that can't be read fails an open, into recovery.

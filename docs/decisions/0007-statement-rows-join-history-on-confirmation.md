# 0007: Statement rows join history only when confirmed

2026-10-14 · Accepted

## Context

An issuer's statement is its final record, and Plaid's history can differ from it: a row Plaid never reported, or one it reports late. Adding every unmatched statement row automatically would count a late Plaid row twice.

## Decision

Imported statement rows reconcile against Plaid's. A row with no matching Plaid transaction inside Plaid's coverage is surfaced for review and joins history only when the user confirms it, and only until Plaid reports it.

## Consequences

Nothing is counted twice, and gaps are reported rather than filled in silently. This is the one place Brief asks for confirmation rather than acting on a confidence ([0009](0009-confidence-not-confirmation.md)), because these rows would enter totals.

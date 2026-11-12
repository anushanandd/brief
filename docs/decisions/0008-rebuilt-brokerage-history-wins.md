# 0008: A rebuilt brokerage history wins over the provider's

Recorded 2026-11-11 from the earlier architecture docs; decided before then, date not recorded · Accepted

## Context

SnapTrade's balance history for a brokerage can be off by the size of a transfer, which distorts returns and net worth history.

## Decision

Brief rebuilds a brokerage's history from its activity, priced at Alpaca's daily closes (`financial_engine::performance`). The provider's own history is used only where the records can't explain the account.

## Consequences

Returns and history follow what the account actually did. Rebuilding needs Alpaca closes for every symbol ever held, and an account whose records are incomplete falls back to the provider's figures.

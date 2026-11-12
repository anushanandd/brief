# 0003: Report periods are trailing days

2026-11-11 · Accepted

## Context

Every chart's D, W, M, Q, Y, and A meant the trailing 1, 7, 30, 90, or 365 days, or all history, and so did Home's recap. Money's reports agreed for every period but the year, which ran from January 1. Home linked its figures to the reports as custom dates to keep them equal, so a report opened from Home said "Custom". Benefits relied on the reports' January year.

## Decision

A report's period means what it means on every chart: its trailing days through today (`insights::reports`, the same lengths as `insights::period_changes::WINDOWS`). Home links to a report by period name. Benefits, which follows a card's calendar year, asks for the year as custom days from January 1, and keeps its own year instead of changing the period the other pages share.

## Consequences

- A figure on Home opens on a report that shows the same figure.
- Spending still uses calendar periods: this month, this quarter, the year to date, and statement cycles, so it can step back through whole periods. Its Week matches the reports' week, but its Month, Quarter, and Year don't match the reports' trailing ones, and Money's strip on Spending mixes the two. Whether Money should use one kind of period throughout is undecided.

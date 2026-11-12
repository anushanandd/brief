# 0005: Moves are measured in layers

Recorded 2026-11-11 from the earlier architecture docs; decided before then, date not recorded · Accepted

## Context

Raw percent changes and news volume make poor signals of what mattered. A volatile stock's ordinary month looks large, a steady stock's real move looks small, and news volume tracks a company's size, not what happened to it.

## Decision

Every move is split into layers (the market's, its group's, and its own), with its own part measured against the holding's typical day. The same model judges today, a period, and a holding's page. Events are ranked by comparing their visible parts in order, never by one combined score. Thresholds are calibrated from saved session counts, shown in Diagnostics, before they are tuned.

## Consequences

A steady stock's real move stands out, and a volatile one's ordinary noise doesn't. Every ranking can be explained by the parts it shows. The model's details are in `summary::sessions` and `summary::eventlog`.

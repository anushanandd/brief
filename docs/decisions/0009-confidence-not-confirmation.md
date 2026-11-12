# 0009: Inferences carry a confidence, not a confirmation

2026-10-17 · Accepted

## Context

Brief infers a lot: chapters of a life, suggested rules, links between records, needs like a bill that may be missed. Asking the user to confirm each one turned understanding into chores.

## Decision

An inference carries a confidence and acts on it: it is raised only at or above its threshold (`insights::attention::RAISE_CONFIDENCE`, for needs), shown with its confidence, and dismissed or corrected in one step. `insights::precision` tracks how often each detector is accepted, so a threshold changes on evidence. Results about money still need an explained rule, and statement rows that would enter totals still need confirming ([0007](0007-statement-rows-join-history-on-confirmation.md)).

## Consequences

The user corrects only what's wrong. An inference is never a notification, since it may be wrong and a notification interrupts.

# 0013: Card rules are saved data

2026-11-11 · Accepted

## Context

Brief tracks a credit card's benefits (statement credits, their windows, and what's left to use) by reading the card's activity against its terms. The terms were one card's rules, compiled into the app, so Brief could only follow that card, and the repository published which card it was.

## Decision

A card product's rules are data (`insights::benefits::CardRules`), saved in settings like any other setting that changes results, so they reach the change log and backups. The user imports a rules file in Settings, and research can later draft one from the issuer's published terms for any card the user holds. Brief ships no real card's rules. Tests and the browser preview read one invented card, `insights/example-card.json`, which covers every feature the engine supports.

Everything that was specific to one card moved into the rules: how its issuer words a statement credit, how its official name reads, and how long a renewal credit lasts. A card is read against the product assigned to it, else the one product its issuer's official name names; the spending card also falls back to the only saved product.

## Consequences

- Brief tracks nothing until a card's rules are saved, and supports any card whose terms can be written as rules.
- Classification needs the rules (an issuer credit is told apart by its descriptor), so the transaction policy receives them, and records each credit's benefit for later readers.
- Golden records that exercised a real card's rules are kept with those rules, outside the repository, and run when present.

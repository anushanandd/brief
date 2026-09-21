# Platinum benefit rules

This file documents the rules implemented in `src/lib/spending.ts`. It is not an issuer balance, enrollment check, or complete statement of American Express terms. Each rule stores the official terms URL shown by the interface; review that source before changing an allowance or eligibility rule.

## Implemented allowances

| Benefit                     | Implemented window and allowance                                                           |
| --------------------------- | ------------------------------------------------------------------------------------------ |
| Digital entertainment       | $20 monthly from 2025-01-01; $25 monthly from 2025-09-18                                   |
| Uber Cash                   | $15 monthly and $35 in December, using `Pacific/Honolulu` for the window                   |
| Uber One                    | $120 annually from 2025-09-18                                                              |
| Walmart+                    | $12.95 plus tax monthly; no remaining-dollar estimate                                      |
| Resy                        | $100 quarterly from 2025-09-18; Tock descriptors are eligible for matching from 2026-09-15 |
| lululemon                   | $75 quarterly from 2025-09-18                                                              |
| Hotel credit                | $200 annually from 2025-01-01; $300 semiannually from 2025-09-18, using `America/Chicago`  |
| Airline fee                 | $200 annually from 2025-01-01                                                              |
| CLEAR+                      | $209 annually from 2025-01-01; $219 annually from 2026-07-01                               |
| Oura Ring                   | $200 annually from 2025-09-18                                                              |
| Equinox                     | $300 annually from 2025-01-01                                                              |
| Global Entry / TSA PreCheck | $120 / up to $85; renewal check is four years after the last unreversed matched credit     |
| SoulCycle bike              | $300 per matched qualifying purchase                                                       |
| Saks Fifth Avenue           | $50 semiannually through 2026-06-30; later identified credits remain in history            |

The code contains the exact merchant and credit descriptor expressions, eligibility copy, effective dates, and official URLs. Do not duplicate those expressions here.

## Evidence policy

Benefit calculations use transactions from the saved spending account. The posting date (`postedOn`, falling back to `date`) determines benefit history and windows.

For a matched benefit, a transaction becomes a detected credit only when it matches the benefit's credit descriptor or has an explicit `benefitConfirmed: true` override. `benefitConfirmed: false` rejects it. Without explicit confirmation, generic refunds, unmatched positive transactions, pending credits, and purchases do not increase issuer-posted credit totals. Identified reversals reduce those totals.

The feed does not link a purchase to a credit or identify the allowance window that produced a credit. Consequently:

- credits within the first 56 days of a window are period-uncertain when a prior window exists;
- the hotel rule uses 90 days;
- a matched reversal in the current window makes its remaining estimate uncertain;
- a snapshot saved before the window cannot establish a remaining estimate;
- uncertain rows keep their evidence but return no remaining amount.

Walmart+, Uber Cash, renewal benefits, and per-purchase benefits never receive a pooled remaining-dollar estimate.

## Uber Cash

Uber Cash is not treated as an issuer-posted Amex credit. One or more posted matching Uber purchases in a calendar month produce one estimated usage entry: $15, or $35 in December. Pending purchases do not count, and multiple purchases do not create multiple estimates. A −$9.99 charge whose merchant is exactly `Uber` is assigned to Uber One rather than Uber Cash.

These estimates appear separately from posted credit totals and Analytics. They are deliberately conservative because the card feed does not identify the tender used inside Uber.

## Change checklist

When changing these rules:

1. Verify current official terms and effective dates.
2. Update the rule in `src/lib/spending.ts`; keep this table synchronized.
3. Preserve the saved-account-ID boundary and posting-date semantics.
4. Preserve uncertainty for near-reset credits, reversals, and stale snapshots.
5. Add or update focused tests in `src/lib/spending.test.ts` and route coverage when visible behavior changes.
6. Use synthetic transactions only.

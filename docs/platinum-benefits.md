# Platinum benefit rules

This file documents the rules in `src-tauri/src/insights/amex-platinum.json`, interpreted by `src-tauri/src/insights/benefits.rs`. It is not an issuer balance, enrollment check, or complete statement of American Express terms. Each rule stores the official terms URL shown by the interface; review that source before changing an allowance or eligibility rule.

## Implemented allowances

| Benefit                     | Implemented window and allowance                                                           |
| --------------------------- | ------------------------------------------------------------------------------------------ |
| Digital entertainment       | $20 monthly from 2025-01-01; $25 monthly from 2025-09-18                                   |
| Uber Cash                   | $15 monthly and $35 in December, using `Pacific/Honolulu` for the window                   |
| Uber One                    | $120 annually from 2025-09-18; a remainder under $9.99 is used up                          |
| Walmart+                    | $12.95 plus tax monthly; no remaining-dollar estimate                                      |
| Resy                        | $100 quarterly from 2025-09-18; Tock descriptors are eligible for matching from 2026-09-15 |
| lululemon                   | $75 quarterly from 2025-09-18                                                              |
| Hotel credit                | $200 annually from 2025-01-01; $300 semiannually from 2025-09-18, using `America/Chicago`  |
| Airline fee                 | $200 annually from 2025-01-01                                                              |
| CLEAR+                      | $209 annually from 2025-01-01; $219 from 2026-07-01; a remainder under $125 is used up     |
| Oura Ring                   | $200 annually from 2025-09-18                                                              |
| Equinox                     | $300 annually from 2025-01-01                                                              |
| Global Entry / TSA PreCheck | $120 / up to $85; renewal check is four years after the last unreversed matched credit     |
| SoulCycle bike              | $300 per matched qualifying purchase                                                       |
| Saks Fifth Avenue           | $50 semiannually through 2026-06-30; later identified credits remain in history            |

The code contains the exact merchant and credit descriptor expressions, eligibility copy, effective dates, and official URLs. Do not duplicate those expressions here.

## Evidence policy

Benefit calculations use transactions from the saved spending account. The posting date (`postedOn`, falling back to `date`) places activity in history and in windows, except that a credit paired with its purchase counts toward the purchase's window.

For a matched benefit, a transaction becomes a detected credit only when it matches the benefit's credit descriptor or has an explicit `benefitConfirmed: true` override. `benefitConfirmed: false` rejects it. Without explicit confirmation, generic refunds, unmatched positive transactions, pending credits, and purchases do not increase issuer-posted credit totals. Identified reversals reduce those totals.

Plaid has no card-benefit or statement-credit data: Liabilities covers statements, payments, and APRs, and credits arrive only as transactions. Plaid assigns them the benefit's spending category, a transfer, or a bank fee, usually at `LOW` confidence, so its category never identifies a credit. Descriptors are matched against the merchant, Plaid's description, and the bank's original descriptor, because Plaid's cleanup can rename a credit (a Saks credit arrived as "Goodwill", filed as income). A positive transaction matching a credit descriptor, or the issuer's generic Amex/Platinum "… Credit"/"… Reimbursement" wording, is classified as a credit everywhere, including descriptors for benefits that the rule data does not cover yet. Only rule-matched credits count toward benefit totals.

The same identified benefit ID separates recurring positive-credit series in Expected activity. A forecast still requires the normal posted cadence evidence and uses observed transaction amounts; the allowance, an unused benefit, or a purchase match never creates an expected credit.

The feed does not link a purchase to a credit or identify the window that produced it, so `benefits/attribution.rs` infers both:

- A credit pairs with the closest earlier unpaired purchase of the same benefit that it fits, within the terms' posting limit (56 days; hotels 90), preferring an exact amount. A paired credit counts toward the window of its purchase, even when it posts after that window closes.
- A reversal pairs with the latest earlier credit of the same amount and counts wherever that credit does.
- An unpaired credit posted within the posting delay of a window's start may belong to that window or the previous one, unless only one of them can hold it under its allowance (allowances that exclude tax never decide this). Such a credit makes the remaining amount a range rather than unknown. An unpaired reversal always may belong to either window.
- After three paired credits, the longest observed delay plus three days, capped at the terms' limit, replaces the limit as the posting delay for that benefit.
- A posted qualifying purchase without a credit is expected within the posting delay, late after it but within the terms' limit, and not credited after that. A purchase in a window whose allowance certain credits already used expects no credit.
- `minimumCharge` marks the smallest qualifying charge; a smaller remainder is used up rather than left.
- A snapshot saved before the window cannot establish a remaining estimate.

Walmart+, Uber Cash, renewal benefits, and per-purchase benefits never receive a pooled remaining-dollar estimate.

Each calendar window that ended on a month end has an outcome. Its unused allowance is known only once the window is settled: the posting delay has passed since it ended and no purchase in it still awaits a credit. A window that began before the saved card history is `partial-history`; Walmart+ is `not-tracked`. Unused allowance counts as missed for a benefit the card has used (any qualifying purchase or posted credit) and as unclaimed otherwise. The Spending metrics show posted credits net of reversals inside the selected range, and missed allowance for settled windows that ended in it, as a range when credits can't be placed. Hidden benefits are excluded from every total.

## Uber Cash

Uber Cash is not treated as an issuer-posted Amex credit. One or more posted matching Uber purchases in a calendar month produce one estimated usage entry: $15, or $35 in December. Pending purchases do not count, and multiple purchases do not create multiple estimates. A −$9.99 charge whose merchant is exactly `Uber` is assigned to Uber One rather than Uber Cash.

Uber applies Uber Cash before charging the card, so a card charge means the month's Uber Cash ran out, while Uber Cash spent without a card charge never appears in the feed. A month without a posted Uber charge is therefore `not-visible`, never missed. These estimates appear separately from posted credit totals and Analytics.

## Other cards

Each rule file names its card product. `benefits::products()` lists the files that exist; adding a card is adding its rule file there, with its own verified terms. The saved spending card is tracked under the default product without any setting. Any other credit card is tracked only after the user assigns it a product in Settings, and its credits, windows, and fee recovery are computed from that card's own transactions.

## Annual fee

The rule data records the Basic Card's annual fee ($895, from the issuer's fee page, checked 2026-10-04) and the descriptor its charge posts under. Fee recovery sets posted credits, net of reversals and excluding hidden benefits, against that fee. A credit on a purchase made for someone else used the allowance but saved them the money, so it is reported apart (`forOthers`) and doesn't count toward the fee. The membership year starts at the latest fee charge in the saved card history from the past year; without one it is the calendar year, and the interface says so, because the feed does not report the card's anniversary. Uber Cash estimates never count. Only the fee was re-verified on that date, so `termsReviewed` is unchanged.

## Change checklist

When changing these rules:

1. Verify current official terms and effective dates.
2. Update the rule data in `src-tauri/src/insights/amex-platinum.json` (and its `version` and `termsReviewed`); change `benefits.rs` only when a new kind of rule is needed. Keep this table synchronized.
3. Preserve the saved-account-ID boundary and posting-date semantics.
4. Preserve uncertainty for unpaired near-reset credits, unpaired reversals, and stale snapshots.
5. Add or update focused Rust tests for the rule. Records in `src/data/fixtures/insights-golden.jsonl` that the change intentionally alters must be updated with a reason; route coverage changes only when visible behavior changes.
6. Use synthetic transactions only.

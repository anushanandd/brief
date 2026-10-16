# Platinum benefit rules

This file documents the rules in `src-tauri/src/insights/amex-platinum.json`, interpreted by `src-tauri/src/insights/benefits.rs`. It is not an issuer balance, enrollment check, or complete statement of American Express terms. Each rule stores the official terms URL shown by the interface; review that source before changing an allowance or eligibility rule.

## Implemented allowances

| Benefit                     | Implemented window and allowance                                                               |
| --------------------------- | ---------------------------------------------------------------------------------------------- |
| Digital entertainment       | $20 monthly from 2025-01-01; $25 monthly from 2025-09-18                                       |
| Uber Cash                   | $15 monthly and $35 in December, using `Pacific/Honolulu` for the window                       |
| Uber One                    | $120 annually from 2025-09-18; a remainder under $9.99 is used up                              |
| Walmart+                    | $12.95 plus tax monthly; no remaining-dollar estimate                                          |
| Resy                        | $100 quarterly from 2025-09-18; Tock descriptors are eligible for matching from 2026-09-15     |
| lululemon                   | $75 quarterly from 2025-09-18                                                                  |
| Hotel credit                | $200 annually from 2025-01-01; $300 semiannually from 2025-09-18, using `America/Chicago`      |
| Airline fee                 | $200 annually from 2025-01-01                                                                  |
| CLEAR+                      | $209 annually from 2025-01-01; $219 from 2026-07-01; a remainder under $125 is used up         |
| Oura Ring                   | $200 annually from 2025-09-18                                                                  |
| Equinox                     | $300 annually from 2025-01-01                                                                  |
| Global Entry / TSA PreCheck | $120 every four years / up to $85 every four and a half, timed from the last unreversed credit |
| SoulCycle bike              | $300 per matched qualifying purchase                                                           |
| Saks Fifth Avenue           | $50 semiannually through 2026-06-30; later identified credits remain in history                |

The code contains the exact merchant and credit descriptor expressions, eligibility copy, effective dates, and official URLs. Do not duplicate those expressions here.

## Evidence policy

Benefit calculations use transactions from the saved spending account. The transaction date (`occurredOn`, the date the merchant processed it, falling back to `postedOn`) places activity in history and in windows, as the issuer counts it: a purchase processed by a window's last day counts there even when it posts in the next, and the issuer dates its credit the same. The hotel terms say so directly ("processed by June 30, 11:59 p.m. Central"; the issuer "relies on the merchant's processing of transactions to determine the transaction date"), and portal attributions match. Posting dates time credits: delays, when a credit is due or late, and when a closed window settles.

For a matched benefit, a positive transaction becomes a detected credit only when it matches the benefit's credit descriptor, has a `benefitConfirmed: true` override, or was assigned to the benefit by hand (`benefitId`). An assignment overrides the descriptor rules for any transaction, so a restaurant charge can count as a Resy purchase. `benefitConfirmed: false` removes a transaction from every benefit. A negative transaction is a reversal only by the issuer's wording; confirming a purchase keeps it a purchase. Without one of these, generic refunds, unmatched positive transactions, pending credits, and purchases do not increase issuer-posted credit totals. Identified reversals reduce those totals.

Plaid has no card-benefit or statement-credit data: Liabilities covers statements, payments, and APRs, and credits arrive only as transactions. Plaid assigns them the benefit's spending category, a transfer, or a bank fee, usually at `LOW` confidence, so its category never identifies a credit. Descriptors are matched against the merchant, Plaid's description, and the bank's original descriptor, because Plaid's cleanup can rename a credit (a Saks credit arrived as "Goodwill", filed as income). A positive transaction matching a credit descriptor, or the issuer's generic Amex/Platinum "… Credit"/"… Reimbursement" wording, is classified as a credit everywhere, including descriptors for benefits that the rule data does not cover yet. Only rule-matched credits count toward benefit totals.

The same identified benefit ID separates recurring positive-credit series in Expected activity. A forecast still requires the normal posted cadence evidence and uses observed transaction amounts; the allowance, an unused benefit, or a purchase match never creates an expected credit.

The feed does not link a purchase to a credit or identify the window that produced it, so `benefits/attribution.rs` infers both:

- A credit pairs with the closest earlier unpaired purchase of the same benefit that it fits, within the terms' posting limit (56 days; hotels 90), preferring an exact amount. Failing that, it pairs with one dated up to three days after it, since the issuer can credit the authorization before the charge settles. A paired credit counts toward the window of its purchase, even when it posts after that window closes.
- A reversal pairs with the latest earlier credit of the same amount and counts wherever that credit does.
- An unpaired credit counts toward the window of its own transaction date. Only one the feed dates by posting alone, within the posting delay of a window's start, may belong to that window or the previous one, unless only one of them can hold it under its allowance (allowances that exclude tax never decide this). Such a credit makes the remaining amount a range rather than unknown. An unpaired reversal always may belong to either window.
- The posting delay is the usual delay between a purchase posting and its credit posting (`typicalDelayDays`, 7 by default, since credits usually post within days). After three paired credits, the longest observed delay plus three days, capped at the terms' limit, replaces it for that benefit. The terms' limit only bounds pairing and decides when a credit is no longer coming.
- A posted qualifying purchase without a credit is expected within the posting delay after it posts, late after that but within the terms' limit, and not credited after that. A purchase in a window whose allowance certain credits already used expects no credit.
- `minimumCharge` marks the smallest qualifying charge; a smaller remainder is used up rather than left.
- A snapshot saved before the window cannot establish a remaining estimate. A window that began before the saved card history is judged by the credits it shows, and says where the history starts.

Walmart+, Uber Cash, renewal benefits, and per-purchase benefits never receive a pooled remaining-dollar estimate.

Rules marked `recurring` (digital entertainment, Uber One, Walmart+) are spent by a subscription, so they are left out of the use-first order and expiry reminders and appear in their own group. Uber Cash stays in the use-first order until a card charge suggests it ran out.

Each calendar benefit has a history of periods from January of the saved history's first year through December of this year (`benefits/history.rs`). A change of cadence ends the old allowance the day before the new one starts, so the hotel credit's 2025 has a $200 period through Sep 17 and a $300 one after it. A closed period's unused allowance is known only once it is settled: the posting delay has passed since it ended and no purchase in it still awaits a credit. Until then it is `settling`. A period the saved card history reaches only partway is judged by the credits it shows, taken as the whole of what it credited, so what it left is exact. A period that ended before the history, or one the history reaches with no credit or charge in it, is `before-history`; Walmart+ is `not-tracked`. A settled period is `full`, `partial`, or `missed`, and its unused allowance is missed. One that leaves less than a dollar is `full`, since cents buy nothing, but those cents still count as missed so totals stay exact; an open window's cents are likewise not left to spend. Hiding a benefit, in Settings or from its details on Benefits, removes its rows, reminders, review items, and missed allowance; its posted credits still count as earned and toward the fee.

## Planned credits

An open or future period lists the credits it should still receive, in order: posted purchases waiting for theirs, then the charges recurring series are expected to make through December (`forecast_within`), each capped by what the period has left. A series counts toward the benefit its latest charge qualifies for as a purchase, so a YouTube Premium subscription plans digital entertainment credits. Uber Cash plans nothing, and a closed period plans nothing new. Plans are estimates; only posted credits count as captured.

## Review

Matches the rules can't settle on their own are queued for a decision, newest first, and each decision is saved as a transaction annotation:

- A positive amount from a benefit's merchant without the credit wording may be the credit or a return: `benefitConfirmed` true or false.
- The issuer's credit wording that no rule recognizes is assigned to a benefit (`benefitId`) or rejected.
- A credit with no qualifying purchase is shown with the card charge that most likely earned it: at least as large, posted within the benefit's usual delay before it, preferring a charge from the benefit's own merchant (which may also post up to three days after it), then an exact amount, then the closest day. Linking assigns that charge to the benefit, even after the benefit retired, so attribution pairs them; otherwise confirming the credit stops asking.
- A wrong match is unlinked from the details of the purchase or its credit, which removes the purchase from every benefit (`benefitConfirmed: false`), so its credit asks again.
- A matched purchase whose credit never came within the terms' limit is rejected as not eligible, or confirmed, which keeps its warning.

`clearBenefit` forgets both decisions on a transaction, which is how a decision is undone.

## Uber Cash

Uber Cash is not treated as an issuer-posted Amex credit. One or more posted matching Uber purchases in a calendar month produce one estimated usage entry: $15, or $35 in December. Pending purchases do not count, and multiple purchases do not create multiple estimates. A −$9.99 charge whose merchant is exactly `Uber` is assigned to Uber One rather than Uber Cash.

Uber applies Uber Cash before charging the card, so a card charge means the month's Uber Cash ran out, while Uber Cash spent without a card charge never appears in the feed. A month without a posted Uber charge is therefore `not-visible`, never missed. These estimates appear separately from posted credit totals and Analytics.

## Other cards

Each rule file names its card product. `benefits::products()` lists the files that exist; adding a card is adding its rule file there, with its own verified terms. The saved spending card is tracked under the default product without any setting. Any other credit card is tracked only after the user assigns it a product in Settings, and its credits, windows, and fee recovery are computed from that card's own transactions.

## Annual fee

The rule data records the Basic Card's annual fee ($895, from the issuer's fee page, checked 2026-10-04) and the descriptor its charge posts under. Fee recovery sets posted credits, net of reversals, against that fee. A credit on a purchase made for someone else used the allowance but saved them the money, so it is reported apart (`forOthers`) and doesn't count toward the fee. The membership year starts at the latest fee charge in the saved card history from the past year; without one it is the calendar year, or the saved history's first day when that is later, and the interface says which, because the feed does not report the card's anniversary. The year ends a year after the fee charge, or on December 31 without one (`through`). `expected` adds only the credits the benefit periods already plan before then (waiting purchases and forecast charges, never an unused allowance), leaving out purchases made for someone else. Uber Cash estimates never count. Only the fee was re-verified on that date, so `termsReviewed` is unchanged.

## Change checklist

When changing these rules:

1. Verify current official terms and effective dates.
2. Update the rule data in `src-tauri/src/insights/amex-platinum.json` (and its `version` and `termsReviewed`); change `benefits.rs` only when a new kind of rule is needed. Keep this table synchronized.
3. Preserve the saved-account-ID boundary, transaction-date windows, and posting-date timing.
4. Preserve uncertainty for near-reset credits dated only by posting, unpaired reversals, and stale snapshots.
5. Add or update focused Rust tests for the rule. Records in `src/data/fixtures/insights-golden.jsonl` that the change intentionally alters must be updated with a reason; route coverage changes only when visible behavior changes.
6. Use synthetic transactions only.

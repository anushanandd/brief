# 0014: Card rules are drafted from evidence and approved

2026-11-12 · Superseded in part by [0015](0015-card-rules-apply-themselves.md): drafts no longer wait for approval

## Context

Card rules are saved data ([0013](0013-card-rules-are-saved-data.md)), so Brief needs a way to get them for any card. Writing them by hand doesn't scale, and issuers publish terms as prose full of exceptions. A model that writes rules from a page would invent figures that look right. An open catalog exists (us-credit-card-benefits, MIT, a few hundred US cards with each credit's amount, cadence, reset, and statement wording), but it is community-maintained and its own reviews flag shallow checks.

## Decision

A draft is built from evidence in layers, and each term carries its best evidence:

1. The catalog gives the first draft. Brief downloads the whole file, so the request says nothing about which card the user holds. Its terms start as Suggested.
2. The issuer's terms page, read through Exa with only the card's public name or page, confirms or contradicts each cap and cadence. Rust splits the page into sentences and checks amounts and periods itself; the on-device model may point at sentences, never state a figure. A term a sentence states is Explained; one a sentence contradicts is a Conflict.
3. Credits that posted on the user's card confirm the statement wording and the amounts. Those terms are Observed. This never leaves the Mac.
4. The user reviews the draft, edits it, and saves it, like an imported file.

Saved rules record their catalog entry and terms page with hashes. A changed entry or page produces a new draft shown as a diff; applying it keeps the old allowance as a dated change.

## Consequences

- Most cards start from the catalog, and any card can be drafted from its terms page.
- No figure is saved unless the user approved it, and each one shows where it came from.
- Brief doesn't contribute corrections back to the catalog, since they would reveal which cards the user holds.
- Bank account rules (fee waivers, bonuses) are prefilled from their terms the same way, into the evidence rule templates.

# 0015: Card rules apply themselves; the user's edits win

2026-11-12 · Accepted · Supersedes the approval step of [0014](0014-card-rules-are-drafted-from-evidence.md)

## Context

Under 0014 a card tracked nothing until the user found, reviewed, and saved its rules, or imported a file. Most people never would, so benefits stayed empty for cards the catalog already describes.

## Decision

With the card benefits catalog on (Settings › Connections, on by default), Brief applies rules itself after each refresh, at launch, and after a settings save:

- A credit card no saved rules cover gets its catalog card's rules when the match is clear: every distinctive word of the catalog card's name is among the names Plaid reports for the account, and no other card scores within 0.25 of it (the issuer bonus). The rules are checked against the card's own credits, saved with `origin: catalog` and the day they were applied, and assigned to the card.
- A closer call is listed as "Which card is this?" with its candidates. Picking one applies it.
- When a catalog card's entry changes, rules Brief applied are drafted again and replaced. A changed allowance is kept as a dated change from that day, so earlier periods keep the terms they had.
- Rules the user reviewed, edited, or imported are `origin: user`. Brief never changes them; a catalog or terms change is shown as a diff to apply or dismiss.

The catalog is downloaded whole, at most once a day, so the request names no card. Turning it off stops the download and leaves saved rules as they are.

## Consequences

- Benefits appear for a covered card without the user doing anything.
- A wrong match costs a review rather than silence, so the match rule is strict and a close call waits for the user.
- Applied rules are ordinary settings changes in the log, so they can be restored like any edit.
- Terms pages are still read only on request with research on; applying never reads one.

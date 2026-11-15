# 0017: Bank and employer rules and life chapters are retired

2026-11-12 · Accepted · Supersedes [0010](0010-rules-are-data.md)

## Context

Brief could explain records with published bank and employer rules ("programs"), found by web research, drafted from a page into a template, and approved by the user. It also detected life chapters (jobs, homes, account openings, spending shifts), which set where typical spending starts, fed Taxes' residency, and answered the assistant's "where did I live" questions.

On the owner's data neither earned its place. Brief ships no programs and none was ever approved, so rules concluded nothing; finding and approving one asked for paperwork the rest of Brief never asks for. Chapters were shown on their own page and changed little else. Card rules, which apply themselves from the open catalog, do the useful part of the same idea.

## Decision

Remove the rule engine, research for rules, rule drafting, chapters, planned chapters, and their pages, assistant tools, and graph objects. Keep card rules and the web lookups they use to read a card's terms; each lookup is still logged before it runs, and Diagnostics shows the log.

## Consequences

- Settings saved with `chapters`, `plannedChapters`, `explanationReviews`, or `programs` still load and drop the keys, so backups and the change log replay.
- Typical spending and usual amounts read every month again, not from a confirmed chapter's start. Retirement coverage counts only the monthly costs entered in its profile.
- A bonus that repaid a fee is income and the fee is spending, as the ledger records them.
- Taxes no longer shows residency; a part-year return's states come from the W-2s instead.
- Schema version 25 clears the saved object graph, which named chapters and rules; the next read rebuilds it.

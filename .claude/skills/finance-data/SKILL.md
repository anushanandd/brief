---
name: finance-data
description: Change Brief's data-driven finance rules - card benefit terms, tax tables, retirement contribution limits, and document readers (pay statements, tax forms). Use before editing a card rules file, example-card.json, tax-tables.json, retirement-limits.json, forms.json, or payslip.json.
---

These rules live in data files, not code, so a rule change never needs a code change.

**Card benefits.** A card's terms are a rules file (`CardRules`), saved in settings. Brief applies them from the open card catalog when a card's match is clear (`card_drafts/apply.rs`, `origin: catalog`); rules the user reviews or imports are `origin: user` and never change by themselves. Brief ships none; the only one in the repo is the invented `src-tauri/src/insights/example-card.json` that tests and the preview read. Read `src-tauri/src/insights/benefits.rs` and its tests first; they show how credits, periods, and matches work and where they can't be settled. A real card's rules are the user's own data and never belong in the repo.

**Benefit logos.** A benefit's logo is asked of Logo.dev by the brand its rule names (`insights/benefits/brands.rs`), and every catalog brand is asked for once a day so the requests don't single out a card. Words that name a kind of benefit rather than a brand go in its `GENERIC` list. Never add a brand list or logo images to the repo; a rule that resolves to the wrong logo sets its `logoDomain`.

**Tax rates and limits.** Rates change in `src-tauri/src/insights/tax-tables.json`, and contribution limits in `retirement-limits.json` beside it. Copy each figure from the IRS revenue procedure or notice the file cites. Never extrapolate a missing year; a year without published figures stays missing.

**Document readers.** Teach readers new labels through `src-tauri/src/documents/forms.json` and `payslip.json`. A reading never fills in a figure the document doesn't print, and never replaces one the user typed. If a reader changes what it reads, raise `READER_VERSION` so earlier readings are offered again.

Validate reader changes against real documents with the ignored test, run locally only. It prints what Brief read and keeps nothing:

```sh
BRIEF_DOCUMENTS_DIR=/path/to/documents cargo test --manifest-path src-tauri/Cargo.toml documents::tests::reads_a_folder -- --ignored --nocapture
```

Keep only synthetic text in the committed tests.

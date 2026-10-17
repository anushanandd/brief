---
name: finance-data
description: Change Brief's data-driven finance rules - card benefit terms, tax tables, retirement contribution limits, and document readers (pay statements, tax forms). Use before editing amex-platinum.json, tax-tables.json, retirement-limits.json, forms.json, or payslip.json.
---

These rules live in data files, not code, so a rule change never needs a code change.

**Card benefits.** Terms change in `src-tauri/src/insights/amex-platinum.json`. Read `docs/platinum-benefits.md` first; it explains how credits, periods, and matches work and where they can't be settled.

**Tax rates and limits.** Rates change in `src-tauri/src/insights/tax-tables.json`, and contribution limits in `retirement-limits.json` beside it. Copy each figure from the IRS revenue procedure or notice the file cites. Never extrapolate a missing year; a year without published figures stays missing.

**Document readers.** Teach readers new labels through `src-tauri/src/documents/forms.json` and `payslip.json`. A reading never fills in a figure the document doesn't print, and never replaces one the user typed. If a reader changes what it reads, raise `READER_VERSION` so earlier readings are offered again.

Validate reader changes against real documents with the ignored test, run locally only. It prints what Brief read and keeps nothing:

```sh
BRIEF_DOCUMENTS_DIR=/path/to/documents cargo test --manifest-path src-tauri/Cargo.toml documents::tests::reads_a_folder -- --ignored --nocapture
```

Keep only synthetic text in the committed tests.

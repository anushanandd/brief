---
name: regen-fixtures
description: Regenerate Brief's synthetic contract fixtures after changing the finance, summary, news, or transaction-policy contract. Use when a Rust fixture test or the Zod field-preservation test fails because a contract changed on purpose.
---

A contract change must pass the Rust fixture test and the Zod field-preservation test.

Regenerate with `BRIEF_UPDATE_CONTRACT_FIXTURE=1`, running each test on its own and in this order, then run `pnpm exec oxfmt`:

1. `native_finance_fixture_matches_typed_projection`
2. `browser_seed_contains_native_transaction_policy` (only when transaction policy changed)
3. `browser_seed_insights_match_native_insights`
4. `native_summary_contract_matches_the_renderer_fixture` (only when the summary contract changed)
5. `native_news_contract_carries_tags_relevance_and_filings` (only when the news contract changed)

For example:

```sh
BRIEF_UPDATE_CONTRACT_FIXTURE=1 cargo test --manifest-path src-tauri/Cargo.toml native_finance_fixture_matches_typed_projection
```

Each fixture is compiled into its test, so a run that rewrites the fixture fails once. Run it again to confirm it passes.

Never regenerate `src/data/fixtures/insights-golden.jsonl`. When a rule changes on purpose, edit the affected records by hand and explain why in the commit.

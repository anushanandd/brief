---
name: regen-fixtures
description: Regenerate Brief's synthetic contract fixtures and the renderer's generated types after changing the finance, summary, news, or transaction-policy contract. Use when a Rust fixture test fails because a contract changed on purpose, or the TypeScript types no longer match Rust's.
---

A contract change must pass the Rust fixture tests and the frontend typecheck.

The renderer's types in `src/lib/bindings` are generated from Rust's by `cargo test`. After renaming or removing a type, regenerate them from scratch so its old file goes:

```sh
rm -rf src/lib/bindings && (cd src-tauri && cargo test export_bindings)
```

Regenerate fixtures with `BRIEF_UPDATE_CONTRACT_FIXTURE=1`, running each test on its own and in this order, then run `pnpm exec oxfmt`:

1. `native_finance_fixture_matches_typed_projection`
2. `browser_seed_contains_native_transaction_policy` (only when transaction policy changed)
3. `browser_seed_insights_match_native_insights` (writes the preview's view, Spending periods, and reports, and the empty view tests start from: `seed-view.json`, `seed-spending.json`, `seed-reports.json`, `seed-activity-totals.json`, `empty-view.json`, compact and left out of the formatter)
4. `period_card_reports_match_the_renderer_fixture` (when reports change: the seed's reports that `today-cards.test.tsx` reads, `src/data/fixtures/period-card-reports.json`)
5. `route_reports_match_the_renderer_fixture` (when reports or the rollup change, or a route test's scenario or report request does: Rust's reports and account totals over the route scenarios, `src/data/fixtures/route-reports.json`, which `src/pages/routes.fixtures.ts` reads). The scenarios and requests come from the route tests themselves; to record them, start from an empty inputs file and repeat until the route tests pass, since each pass reaches renders that needed the last pass's answers:

   ```sh
   rm src/data/fixtures/route-report-inputs.json
   BRIEF_RECORD_ROUTE_REPORTS=1 pnpm exec vitest run src/pages --no-file-parallelism
   (cd src-tauri && BRIEF_UPDATE_CONTRACT_FIXTURE=1 cargo test route_reports_match_the_renderer_fixture)
   ```

   Both files are compact and left out of the formatter.

6. `native_summary_contract_matches_the_renderer_fixture` (only when the summary contract changed)
7. `native_news_contract_carries_tags_relevance_and_filings` (only when the news contract changed)

For example:

```sh
(cd src-tauri && BRIEF_UPDATE_CONTRACT_FIXTURE=1 cargo test native_finance_fixture_matches_typed_projection)
```

Fixtures compiled into their test fail once on the run that rewrites them; run it again to confirm it passes.

Never regenerate `src/data/fixtures/insights-golden.jsonl`. When a rule changes on purpose, edit the affected records by hand and explain why in the commit.

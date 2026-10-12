# Agent guide

Brief is a personal, local-first macOS finance app (Tauri: Rust + React). Read [ARCHITECTURE.md](ARCHITECTURE.md) before touching providers, refresh, persistence, IPC, or market data. Its invariants are binding. Read [.interface-design/system.md](.interface-design/system.md) before interface work.

## Scope

- No servers, cloud sync, multi-user, mobile layouts, or hosted services. Providers stay read-only.
- The window minimum width is 860 px. Don't use computer-use tools.
- Prefer the standard library, platform features, and existing dependencies.
- Preserve unrelated working-tree changes.

## Rules that are easy to break

- Rust computes every financial result. React formats `snapshot.insights` and native classification (through `src/lib/transaction-kind.ts`) and must not reimplement either.
- Result-changing settings go through `FinanceSettings`: read them with `financeSettings(data)` and change them with `useFinanceEdits`. Never use renderer storage for them.
- Benefit terms change in `src-tauri/src/insights/amex-platinum.json`, not in code. Read [docs/platinum-benefits.md](docs/platinum-benefits.md) first.
- Tax rates change in `src-tauri/src/insights/tax-tables.json`, not in code, copied from the IRS revenue procedure the file cites. Never extrapolate a missing year.
- Holdings chart selection must not replace or stop the portfolio market subscriptions.
- Never call billed Plaid endpoints (live balances, Liabilities) on an ordinary or background refresh.
- Don't gate native actions with `window.confirm`; the macOS WebView treats it as Cancel. Use an in-app confirmation with pending and error states.
- Don't rename or replace a SQLite file while a connection has it open. Restore uses SQLite's Backup API.
- Use card-based navigation, not top tabs. Keep headings short, with no subtitle that restates them.
- Every control outside a list row carries `data-command` naming a registered action; `routes.test.tsx` fails otherwise.
- Keep shortcut access, accessible names, unavailable states, and reduced-motion behavior. Focus never draws a ring and Tab never moves focus; the keyboard works through shortcuts.

## Commands

```sh
pnpm dev                                               # browser preview (synthetic data)
pnpm tauri dev                                         # native app
pnpm test                                              # frontend tests
pnpm check                                             # format, lint, test, build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
```

Run checks in proportion to the change. Docs-only changes need no build.

## Tests

- Keep tests beside their module. The `src/pages/routes*.test.tsx` files check server-rendered route wiring only, not effects, canvas pixels, or interaction; they share the mocked scenario in `routes.harness.tsx`.
- Assert financial results, unavailable states, accessible controls, and trust boundaries, not styling.
- Pin dates and use synthetic amounts. Prefer completion signals over sleeps.
- Never put credentials, tokens, or real financial data in fixtures, logs, screenshots, or tests.
- Contract changes must pass the Rust fixture test and the Zod field-preservation test. Regenerate synthetic fixtures with `BRIEF_UPDATE_CONTRACT_FIXTURE=1`, running each test separately and in this order, then `pnpm exec oxfmt`:
  1. `native_finance_fixture_matches_typed_projection`
  2. `browser_seed_contains_native_transaction_policy` (only when transaction policy changed)
  3. `browser_seed_insights_match_native_insights`
  4. `native_summary_contract_matches_the_renderer_fixture` (only when the summary contract changed)
  5. `native_news_contract_carries_tags_relevance_and_filings` (only when the news contract changed)

  The fixture is compiled into each test, so a test that rewrites it fails once; run it again to confirm.

- Never regenerate `src/data/fixtures/insights-golden.jsonl`. When a rule changes on purpose, edit the affected records and explain why.

## Gotchas

- The browser preview reads `src/data/seed.json` and `seed-insights.json`. Integrations, edits, rules, and Apple Intelligence exist only in the native app.
- Insights depend on the local day. Tests pass an explicit `now` and a named time zone.
- Keep Tauri crates and their npm packages on the same major.minor, and update both lockfiles together.
- `liveline@0.0.7` is patched via `pnpm-workspace.yaml`. Read [docs/liveline-patch-review.md](docs/liveline-patch-review.md) before changing it, and restart Vite afterward.
- Market sockets: preserve timestamp monotonicity, feed separation, correction repair, hidden-window shutdown, and stale cleanup. Price ticks may be coalesced; bar corrections may not be dropped. All-time Holdings history may reuse a cached prefix only after a fresh full verification with an identical overlap.
- Startup reveal waits for local reads only. Saved market observations must never be labeled live.
- `pnpm tauri dev` signs builds through `scripts/macos-signed-runner.sh` so Keychain access survives rebuilds. Brief must run from `target/debug/brief`, the path the Keychain trusts; a copy elsewhere can't read saved connections. The runner signs the build in `deps/` that cargo copies from, so later runs launch an already-signed binary (re-signing costs most of a second), and it starts alongside the dev server, waiting for it itself.
- The dev icon is embedded at build time. Regenerate `icon.icns` and `128x128@2x.png` from `icon.svg`, then restart.
- Don't run `cargo clean` unless necessary.

## Documentation

Docs describe what code can't: purpose, invariants, boundaries, and setup. Code and tests remain the source of truth.

- Update a doc only when it would otherwise mislead a reader.
- Prefer editing or deleting sentences over adding them.
- Don't document what the code, tests, or UI already show: layouts, pixel values, copy, or per-page behavior.
- State each fact in one place and link to it elsewhere.
- In the final response, name any doc you changed, or say why none needed to change.

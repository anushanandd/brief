# Agent guide

Brief is a personal, local-first macOS finance app (Tauri: Rust + React). Read [PRINCIPLES.md](PRINCIPLES.md) first; it settles conflicts. Read [DESIGN.md](DESIGN.md) before interface work, and [ARCHITECTURE.md](ARCHITECTURE.md) before touching providers, refresh, storage, IPC, or market data.

## Commands

```sh
pnpm dev                                               # browser preview (synthetic data)
pnpm tauri dev                                         # native app
pnpm test                                              # frontend tests
pnpm check                                             # format, lint, test, build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
```

Run checks in proportion to the change. Docs-only changes need none.

## Rules no test catches yet

- Don't launch the native app from an agent session; it reads and writes real data.
- Don't gate native actions with `window.confirm`; the macOS WebView treats it as Cancel.
- Result-changing settings go through `FinanceSettings` (`financeSettings(data)`, `useFinanceEdits`), never renderer storage.
- Never call billed Plaid endpoints on an ordinary or background refresh.
- Don't rename or replace a SQLite file while a connection holds it; restore uses SQLite's Backup API.
- Holdings chart selection must not stop the portfolio market subscriptions.
- Never regenerate `src/data/fixtures/insights-golden.jsonl`. Edit the records a deliberate rule change affects, and say why.
- Preserve unrelated working-tree changes. Prefer the standard library and existing dependencies.

## Skills

- `regen-fixtures`: regenerating contract fixtures after a contract change.
- `finance-data`: benefit terms, tax tables, contribution limits, and document readers.

## Gotchas

- The browser preview reads `src/data/seed.json` and `seed-insights.json`. Integrations, edits, and Apple Intelligence exist only in the native app.
- Insights depend on the local day; tests pass an explicit `now` and a named time zone.
- Keep Tauri crates and their npm packages on the same major.minor, and update both lockfiles together.
- `liveline@0.0.7` is patched; read [docs/liveline-patch-review.md](docs/liveline-patch-review.md) before changing it, and restart Vite afterward.
- Market sockets: keep timestamps monotonic, feeds separate, corrections repaired, hidden windows shut down, and stale data cleaned. Price ticks may coalesce; bar corrections may not drop.
- `pnpm tauri dev` signs builds through `scripts/macos-signed-runner.sh`. Brief must run from `target/debug/brief`, the path Keychain trusts.
- Don't run `cargo clean` unless necessary.

## Code and docs

Keep tests beside their module; assert results, unavailable states, and trust boundaries, not styling. Pin dates and use synthetic amounts. Match the surrounding code, and comment modules and non-obvious functions rather than every line. Docs say what code can't; name any doc you changed in your final response.

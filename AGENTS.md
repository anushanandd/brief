# Brief agent guide

## Scope

- Brief is a personal, local-first macOS app. Do not add server, multi-user, cloud-sync, web-product, mobile, or production-service architecture.
- Do not use computer-use tools or add mobile layouts. The native window minimum width is 860 px.
- Preserve read-only providers and the absence of a hosted backend.
- Prefer the standard library, platform features, and existing dependencies.
- Preserve unrelated working-tree changes.

## Read the relevant documentation

Do not load every document for every task. Read:

- [ARCHITECTURE.md](ARCHITECTURE.md) for persistence, IPC, providers, refreshes, or market-state changes.
- [.interface-design/system.md](.interface-design/system.md) for interface work.
- [docs/platinum-benefits.md](docs/platinum-benefits.md) before changing benefit rules.
- [docs/liveline-patch-review.md](docs/liveline-patch-review.md) before changing or upgrading Liveline.

Code and tests are the source of truth. Update:

- `README.md` when setup, commands, integrations, privacy notes, or top-level capabilities change.
- `ARCHITECTURE.md` when ownership, persistence, IPC, provider, refresh, or polling flows change.
- `AGENTS.md` when repository rules, commands, verification requirements, or recurring gotchas change.
- Specialized docs only when their implemented rules change.

Do not update docs for unrelated implementation details. In the final response, name updated documentation or explain why none was needed.

## Interface rules

- Use card-based navigation, not horizontal top tabs. Add a detail route only when it materially helps.
- Keep page, card, and section headings concise. Do not add subtitles that restate a heading; put necessary scope, provenance, or instructions beside the relevant control or state.
- Preserve keyboard access, accessible names, unavailable states, and reduced-motion behavior.

## Invariants

- Rust owns provider access, credentials, the committed snapshot, and durable finance state. Credentials and provider tokens belong only in macOS Keychain.
- Refreshes are staged, bounded by one 120-second provider/projection deadline, validated, and atomically committed. Failure must leave the previous snapshot and caches usable.
- Plaid cursor and page changes stay temporary until pagination and the matching balance request complete.
- `FinanceProvider` owns committed data. `LiveMarketProvider` overlays only a compatible Rust valuation projection.
- One native market service owns held-symbol feeds. Holdings chart selection must not replace portfolio subscriptions or stop them on route cleanup.
- Keep canonical Holdings bars separate from latest trade or indicative quote observations. The market-price cache is separate from finance state.
- Spending and Platinum benefits use the saved spending account ID, never provider ordering. The Accounts selector excludes that account.
- Rust assigns shared transaction classification after annotations. React reads it through `src/lib/transaction-kind.ts` and must not implement a second policy.
- Financial calculations are deterministic. Persist the projection calendar date rather than recomputing it from the machine's current timezone. Apple Intelligence may explain supplied evidence but must not calculate or mutate finance data.
- Never put credentials, tokens, or real financial data in logs, fixtures, screenshots, or tests.

## Code map

- `src/lib/api.ts`: browser fallback and Tauri IPC client
- `src/lib/schema.ts`: renderer runtime contract
- `src/hooks/`: committed finance state, refresh, and live-market overlay
- `src-tauri/src/lib.rs`: Tauri app state and command registration
- `src-tauri/src/refresh.rs`: refresh and commit orchestration
- `src-tauri/src/market.rs`: live-market command orchestration
- `src-tauri/src/finance_contract.rs`: native output types
- `src-tauri/src/financial_engine.rs`: canonical projection
- `src-tauri/src/transaction_policy.rs`: transaction classification
- `src-tauri/src/providers/mod.rs`: shared provider contracts and caches
- `src-tauri/src/providers/http.rs`: provider errors, retries, and diagnostics
- `src-tauri/src/providers/market_stream.rs`: shared held-symbol sockets
- `src-tauri/src/providers/holding_market.rs`: security history and price cache
- `src-tauri/src/storage.rs`: staging, validation, recovery, and commits

## Commands

```sh
pnpm dev
pnpm tauri dev
pnpm test
pnpm check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
```

Run checks proportional to the change. Documentation-only changes need no application build. Use targeted tests while iterating; run `pnpm check` for frontend or cross-boundary changes and Rust formatting/tests for Rust changes.

## Verification

- Keep pure-function tests beside their module. `src/pages/routes.test.tsx` verifies server-rendered route wiring, not mounted effects, canvas pixels, or interaction.
- Assert financial results, unavailable states, accessible controls, and trust boundaries. Avoid styling inventories and assertions that repeat configuration.
- Pin calendar fixtures and use synthetic amounts. Prefer explicit completion signals or polling over arbitrary sleeps.
- Preserve migration, recovery, contract field-preservation, pagination, and stale-request/cancellation regressions.
- Native contract changes must pass the Rust projection fixture and the Zod field-preservation test. Regenerate only synthetic fixtures:

  ```sh
  BRIEF_UPDATE_CONTRACT_FIXTURE=1 cargo test --manifest-path src-tauri/Cargo.toml native_finance_fixture_matches_typed_projection
  ```

  When transaction policy changes, use the same environment variable with `browser_seed_contains_native_transaction_policy`, then rerun normal Rust and frontend tests.

## Gotchas

- Browser development uses `src/data/seed.json`; real integrations exist only in Tauri.
- `liveline@0.0.7` is patched through `pnpm-workspace.yaml`. Keep ESM, CommonJS, and both declaration outputs synchronized; restart Vite after patch changes.
- Market sockets are app-owned and request-scoped. Preserve timestamp monotonicity, feed separation, correction repair, hidden-window shutdown, and stale-cleanup tests. Price observations may be coalesced; bar corrections may not be silently dropped.
- All-time Holdings history may reuse a prefix only after fresh full verification and an identical overlap.
- Startup reveal waits for local reads, never network or market readiness. Saved observations must not be labeled live.
- Do not gate native actions with `window.confirm`; the macOS WebView treats it as Cancel. Use an in-app confirmation with pending and error states.
- Do not rename or replace a SQLite database while a connection has it open. Normal backup restore uses SQLite's Backup API; filesystem replacement is only for recovery paths with no open primary connection.
- Keep the app-data directory and financial/cache databases private, bound provider response bodies before decoding, and remove migrated legacy plaintext only after a valid SQLite backup exists.
- Do not run `cargo clean` unless explicitly necessary.
- The macOS dev icon is embedded in the Rust executable. Regenerate `icon.icns` and `128x128@2x.png` from `icon.svg`, then restart `pnpm tauri dev`; `build.rs` watches the generated assets for rebuilds.
- `pnpm tauri dev` signs rebuilt native executables through `scripts/macos-signed-runner.sh` so Keychain access survives rebuilds. It uses `APPLE_SIGNING_IDENTITY` or the first valid Apple Development identity.

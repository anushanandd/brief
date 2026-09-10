# Brief agent guide

## Scope

- Brief is a personal, local-first macOS app. Do not add production, multi-user, server, cloud-sync, web-product, or mobile architecture.
- Do not use computer use or attempt mobile views or mobile testing. The native window has a minimum width of 860 px.
- Preserve the read-only provider model and the absence of a hosted backend.
- Prefer the standard library, platform features, and existing dependencies over new abstractions or packages.

## Documentation contract

- At the start of every task, read [README.md](README.md) and [ARCHITECTURE.md](ARCHITECTURE.md).
- Treat code and tests as the source of truth. If documentation disagrees, verify the behavior and update the documentation.

<!-- - Update `README.md` when setup, commands, integrations, or user-facing capabilities change.
- Update `ARCHITECTURE.md` when data flow, state ownership, persistence, IPC, providers, or polling change.
- Update `AGENTS.md` when repository rules, commands, verification requirements, or recurring gotchas change.
- Do not edit documentation when the change does not affect it.
- In the final response, name the documentation updated, or state why no documentation update was needed. -->

## Interface design

- Do not use horizontal top tab bars for navigation. Use card-based navigation, and add a clickable detail route only when the deeper view materially helps.
- Do not add subtitles or captions that merely restate a page title or list the page's contents. Keep page headers concise; use supporting copy only for actionable instructions, state, provenance, or necessary financial scope.

## Invariants

- Rust owns provider access, secrets, and durable state. Credentials and provider tokens belong only in macOS Keychain.
- Refreshes are staged, validated, and atomically committed. A failure must leave the prior snapshot and provider caches usable.
- Plaid pagination applies pages to temporary state and publishes its cursor and cache only after the complete operation succeeds.
- `FinanceProvider` holds committed state. Live quotes remain route-scoped: Home polls; other current routes use the committed snapshot.
- Spending and Platinum benefits use the saved spending account ID, never provider ordering.
- Financial calculations are deterministic. Apple Intelligence may explain supplied evidence but must not calculate or mutate finance data.
- Never place credentials, tokens, or real financial data in logs, fixtures, screenshots, or tests.

## Code map

- `src/lib/api.ts`: browser fallback and Tauri IPC client
- `src/lib/schema.ts`, `src/lib/normalize.ts`: renderer contract and normalization
- `src/hooks/`: committed finance state, refreshes, and live market overlay
- `src-tauri/src/lib.rs`: Tauri commands and refresh orchestration
- `src-tauri/src/providers.rs`: Plaid, SnapTrade, and Alpaca integrations
- `src-tauri/src/storage.rs`: versioned state, locking, validation, and atomic writes

## Commands

```sh
pnpm dev
pnpm tauri dev
pnpm test
pnpm check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
```

Run checks proportional to the change. Use targeted tests while iterating; run `pnpm check` for frontend or cross-boundary changes and Rust formatting/tests for Rust changes. Documentation-only changes need no application build.

## Gotchas

- Browser development uses `src/data/seed.json`; real integrations exist only in Tauri.
- `liveline@0.0.7` is patched through `pnpm-workspace.yaml`; review the patch before upgrading it.
- Do not run `cargo clean` unless explicitly needed; the rebuild is expensive.
- Preserve unrelated working-tree changes.

# Brief

Brief is a private, local-first personal finance app for macOS. React renders the interface; Tauri and Rust connect read-only providers, build the committed financial snapshot, and store it locally. Brief has no hosted backend or cloud synchronization.

## Capabilities

- Home, Accounts, Spending, Analytics, Holdings, and Activity workspaces
- Plaid bank, card, and investment imports
- SnapTrade brokerage accounts, positions, and activity
- Alpaca portfolio pricing, benchmark data, and security charts
- Alpha Vantage ticker news and earnings dates
- Local transaction annotations, account links, display preferences, and CSV exports
- Optional on-device Apple Intelligence answers from bounded, supplied evidence

Browser development uses deterministic synthetic data from `src/data/seed.json`. Provider connections and durable financial edits are available only in the native app.

## Privacy and storage

Provider credentials and access tokens are stored in macOS Keychain. The committed snapshot, provider caches, annotations, account links, and sync history are stored in `finance-state.sqlite3` under the app data directory. Market, startup, and news caches use separate local SQLite files.

The finance database and caches are ordinary local application files; Keychain does not encrypt them. Protect the macOS account with normal login security and FileVault.

Financial providers are read-only. Remote merchant and security logos are disabled by default; enabling them allows Logo.dev or Plaid's logo host to receive the relevant ticker or merchant-domain identifier.

See [ARCHITECTURE.md](ARCHITECTURE.md) for ownership, persistence, refresh, and market-data flows.

## Develop

Install the JavaScript dependencies:

```sh
pnpm install
```

Run the browser preview with synthetic data:

```sh
pnpm dev
```

Run the native macOS app with provider support:

```sh
pnpm tauri dev
```

Provider credentials and connections are configured in Settings. Brief loads the last committed snapshot first and refreshes configured providers in the background when that snapshot is at least one hour old.

## Verify

Frontend checks:

```sh
pnpm check
```

Rust checks:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
```

## Documentation

- [AGENTS.md](AGENTS.md): repository rules and verification requirements
- [ARCHITECTURE.md](ARCHITECTURE.md): system boundaries and data flows
- [.interface-design/system.md](.interface-design/system.md): interface principles and shared tokens
- [docs/platinum-benefits.md](docs/platinum-benefits.md): implemented benefit rules and evidence limits
- [docs/liveline-patch-review.md](docs/liveline-patch-review.md): patched chart behavior and upgrade checklist

# Brief

Brief is a private, local-first personal finance app for macOS. React renders the interface; Tauri and Rust connect read-only providers, build the committed financial snapshot, and store it locally. Brief has no hosted backend or cloud synchronization.

## Capabilities

- Home, Accounts, Spending, Analytics, Holdings, Activity, and Data Health workspaces
- Plaid bank, card, and investment imports, plus an on-demand view of Plaid Recurring Transactions for manual accuracy checks
- SnapTrade brokerage accounts, positions, and activity
- Brokerage balances calculated from cash and positions, with the dated SnapTrade total shown on individual brokerage graphs
- Alpaca portfolio pricing, benchmark data, and security charts
- Alpha Vantage ticker news and earnings dates
- Local recurring-activity forecasts across accounts, including recognized monthly Platinum credits and banking streams
- Local transaction annotations, account links, display preferences (including Analytics chart order), and CSV exports
- Time-weighted and money-weighted investment returns, volatility, drawdown, benchmark comparison, and explicit data-quality limits
- Validated portable financial backups with retained-state restore protection
- Optional on-device Apple Intelligence answers from bounded, supplied evidence
- ⌘H or the sidebar control temporarily masks personal financial numbers and balance-history charts
- ⌘K searches local activity descriptions and filters alongside pages, accounts, and holdings

Browser development uses deterministic synthetic data from `src/data/seed.json`. Provider connections and durable financial edits are available only in the native app.

## Privacy and storage

Provider credentials and access tokens are stored in macOS Keychain. The committed snapshot, provider caches, annotations, account links, and sync history are stored in `finance-state.sqlite3` under the app data directory. Market, startup, and news caches use separate local SQLite files.

The finance database and caches are ordinary local application files; Keychain does not encrypt them. Protect the macOS account with normal login security and FileVault. Portable `.briefbackup` files are also unencrypted financial databases. They exclude Keychain credentials and disposable market, startup, and news caches, and should be stored somewhere private.

Financial providers are read-only. Remote merchant and security logos are disabled by default; enabling them allows Logo.dev or Plaid's logo host to receive the relevant ticker or merchant-domain identifier.

Value masking is for screen privacy. Public security prices, market changes, and price-history charts remain visible. Masking does not change stored data, accessibility descriptions, or CSV exports, and resets when Brief restarts.

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

Native development builds are signed before launch using `APPLE_SIGNING_IDENTITY`, or the first valid Apple Development identity in Keychain. This lets Keychain remember provider-vault access across Rust rebuilds. On first use, choose **Always Allow** when macOS asks to use the signing key and provider credentials. Set `APPLE_SIGNING_IDENTITY` explicitly when more than one development identity is installed.

Provider credentials and connections are configured in Settings. The Plaid recurring check requires Plaid's Recurring Transactions add-on; it fetches provider-detected streams only when requested and does not save them or use them in Expected activity. Brief loads the last committed snapshot first and refreshes configured providers in the background when that snapshot is at least one hour old.

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

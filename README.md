# Brief

Brief is a private personal-finance app for macOS. It reads your bank, card, and brokerage accounts, keeps everything on your Mac, and shows where you stand: net worth, spending, holdings, expected bills, and card benefits.

There is no server and no cloud sync. Providers are read-only.

## Features

- **Workspaces:** Today, Money, Investments, Accounts, Activity, and Data health
- **Sources:** Plaid (banks, cards, investments), SnapTrade (brokerages), Alpaca (prices, charts, and news), SEC EDGAR (company filings), Alpha Vantage (earnings)
- **Insights:** expected bills and income, card statement dates and utilization, Amex Platinum credits, unusual or duplicate charges, subscription price changes, trips, investment returns, dividend income
- **Taxes:** a 1099 preview, wash sales, losses to harvest, and, with a tax profile, an estimated tax
- **Your data:** transaction edits, category rules, CSV export, and validated backups
- **Optional extras:** on-device Apple Intelligence answers, daily background refresh, Open at login
- **Keyboard-first:** ⌘K command palette, `?` key guide, ⌘H hides amounts on screen

## Privacy

- Credentials and provider tokens live only in macOS Keychain.
- Financial data lives in SQLite files in Brief's app-data folder, readable only by your user. **These files are not encrypted**, so use FileVault.
- `.briefbackup` files are complete, unencrypted copies of your financial data. Exporting or restoring one requires macOS authentication.
- Logos are off by default. Turning them on sends tickers and merchant and institution domains to Logo.dev and Plaid.
- Plaid live balances, card statement details, investment activity, and recurring streams are billed per request or per connection. They run only when you turn them on.

## Develop

Requirements: macOS 14+, Xcode, Rust 1.89+, Node, and pnpm. Apple Intelligence needs macOS 26.

```sh
pnpm install
pnpm dev          # browser preview with synthetic data
pnpm tauri dev    # native app with real providers
```

Add provider credentials in Settings → Connections. Native dev builds are code-signed so Keychain access survives rebuilds; if you have several Apple Development identities, set `APPLE_SIGNING_IDENTITY`.

## Verify

```sh
pnpm check                                             # format, lint, test, build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
```

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md): how the system fits together and what must stay true
- [AGENTS.md](AGENTS.md): rules for coding agents
- [.interface-design/system.md](.interface-design/system.md): interface principles and tokens
- [docs/platinum-benefits.md](docs/platinum-benefits.md): benefit rules and their limits
- [docs/liveline-patch-review.md](docs/liveline-patch-review.md): the chart library patch

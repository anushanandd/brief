# Brief

Brief is a private finance app for macOS. It reads your bank, card, and brokerage accounts, your statements, and your tax documents, keeps all of it on your Mac, and shows where you stand: net worth, spending, holdings, bills, card benefits, and taxes. There is no server, and every provider is read-only.

## What it does

- **Home:** net worth, what needs attention, what's coming up, and recent activity.
- **Money:** spending by category and merchant, recurring bills, cash flow, income, interest, fees, and card benefits.
- **Investments:** live portfolio value, each holding with its news and filings, retirement accounts, dividends, and realized gains.
- **Activity:** every transaction in one ledger, with search, filters, edits, splits, and the evidence behind each field.
- **Documents and taxes:** pay statements and tax forms read on the Mac, a 1099 preview, wash sales, and an estimated tax.
- **Keyboard first:** ⌘K reaches every command; ⌘H hides amounts on screen.

## How it works

```text
Plaid · SnapTrade · Alpaca · SEC EDGAR · statements · documents
              │  read-only, retried, bounded
              ▼
   Rust: typed records, decimal math, one validated commit
              │
              ▼
   SQLite on your Mac  ──►  insights derived on every read
              │  Tauri IPC
              ▼
   React: formats what Rust computed
```

Rust owns everything that touches money or secrets; the interface only formats. See [ARCHITECTURE.md](ARCHITECTURE.md) for the details.

## Three problems worth reading the code for

1. **Statements reconciled against Plaid.** An imported bank or card statement is matched row by row to what Plaid reported, amounts that disagree are flagged, and a row Plaid never sent joins history only after you confirm it ([`statements/`](src-tauri/src/statements)).
2. **Every field carries its evidence.** A purchase's merchant, place, and category are resolved from the issuer's export, the bank's descriptor, and Plaid in a fixed order of trust, and each field is graded as confirmed, supported, weak, or conflicting ([`merchants.rs`](src-tauri/src/merchants.rs), [`quality.rs`](src-tauri/src/quality.rs)).
3. **A refresh commits completely or not at all.** Providers are fetched into staging under one deadline, projected with decimal math, validated, and committed in a single SQLite transaction; a failure leaves the last good state untouched ([`refresh.rs`](src-tauri/src/refresh.rs)).

## Privacy

- Credentials and provider tokens live only in macOS Keychain.
- Financial data lives in SQLite files readable only by your user. **They aren't encrypted**, so use FileVault. Document originals are encrypted.
- Backups are complete copies of your data. Exporting or restoring one requires macOS authentication.
- Logos are off by default; turning them on sends tickers and merchant and institution domains to Logo.dev and Plaid.
- Billed Plaid products run only when you turn them on.

## Develop

Requirements: macOS 14 or later, Xcode, Rust 1.89 or later, Node, and pnpm. Apple Intelligence needs macOS 26.

```sh
pnpm install
pnpm dev          # browser preview with synthetic data
pnpm tauri dev    # native app with real providers
pnpm check        # format, lint, test, build
cargo test --manifest-path src-tauri/Cargo.toml
```

Add provider credentials in Settings → Connections. Native dev builds are code-signed so Keychain access survives rebuilds; with several Apple Development identities, set `APPLE_SIGNING_IDENTITY`.

## Documentation

- [PRINCIPLES.md](PRINCIPLES.md): what Brief is for and how it decides
- [DESIGN.md](DESIGN.md): how it should look and move
- [ARCHITECTURE.md](ARCHITECTURE.md): how the system fits together
- [AGENTS.md](AGENTS.md): guidance for coding agents

## License

[PolyForm Noncommercial 1.0.0](LICENSE). You may read, run, and modify Brief for personal, noncommercial use.

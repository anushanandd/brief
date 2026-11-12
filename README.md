# Brief

A personal finance app for macOS. It reads bank, card, and brokerage accounts, statements, and tax documents, keeps every record on the Mac, and computes every figure in Rust.

<!--
Screenshot: take one from the browser preview (`pnpm dev`, synthetic data only), save it as
docs/screenshot.png, and replace this comment with:

![Brief's Home page, showing synthetic data](docs/screenshot.png)
-->

There is no server. Brief is a Tauri 2 app: a Rust backend in `src-tauri/`, a React 19 and TypeScript interface in `src/`, and SQLite on the Mac. Every provider is read-only.

## What it does

- **Home:** net worth over time, what needs attention, and what's coming up.
- **Money:** spending by period, category, and merchant; cash flow; recurring bills; a 90-day cash forecast; card statements and benefit credits.
- **Activity:** one ledger of every transaction, with search, edits, splits, and the evidence behind each field.
- **Investments:** live portfolio value, holdings with their news and SEC filings, returns beside the S&P 500, tax lots, and dividends.
- **Documents and taxes:** pay statements and tax forms read on the Mac and matched to deposits; realized gains, wash sales, and an estimated tax.
- **Assistant:** questions answered on-device by Apple Intelligence, quoting only figures Rust computed.

It is built for the keyboard: a command palette reaches every page, record, and action.

## How it's built

```text
Plaid · SnapTrade · Alpaca · SEC EDGAR · Alpha Vantage · statements · documents
              │  read-only, retried, bounded
              ▼
   Rust: typed records, decimal math, one validated commit
              │
              ▼
   SQLite change log + snapshot  ──►  insights derived on every read
              │  Tauri IPC, types generated from Rust
              ▼
   React: formats what Rust computed
```

**Rust computes, React displays.** Every total, forecast, and classification is computed in Rust with decimal math. The interface's types are generated from the Rust structs ([ts-rs](https://github.com/Aleph-Alpha/ts-rs)), and CI fails if they drift.

**The change log is the source of truth.** Every write is one change, applied by the same function live and in replay, and saved with a hash of the state it produced in one SQLite transaction. A test runs a generated person's life through the real refresh and edit paths and requires the log to rebuild the saved tables exactly ([`storage/change.rs`](src-tauri/src/storage/change.rs), [`storage/tests/replay.rs`](src-tauri/src/storage/tests/replay.rs)).

**A refresh commits completely or not at all.** Every provider is fetched into staging under one deadline, then projected, validated, and committed in a single transaction. A provider that fails keeps its last good data ([`refresh.rs`](src-tauri/src/refresh.rs)).

**The database recovers itself.** Migrations run in one transaction after a copy of the old version is kept, every write is preceded by a backup, and a database that can't be read opens its newest readable copy read-only ([`storage/generations.rs`](src-tauri/src/storage/generations.rs)).

**Live market data survives corrections.** One shared socket per feed serves the whole app, keeps timestamps in order, and treats a corrected or cancelled trade as something to reconcile ([`providers/market_stream.rs`](src-tauri/src/providers/market_stream.rs)).

**The assistant can't make up a number.** Apple's on-device model sees about 4,096 tokens, so Rust plans each question first: it resolves the dates, offers the model at most four tools, and answers a clear question itself. Every figure in an answer is checked against what the tools returned ([`assistant_planner.rs`](src-tauri/src/insights/assistant_planner.rs), [`foundation_model/figures.rs`](src-tauri/src/foundation_model/figures.rs)). An 81-question eval set checks the routing in CI.

**Tests run on synthetic lives.** Results depend only on the data and the local day, so tests pin both. Over 800 Rust tests and 600 frontend tests run on generated people, and none on real data.

[ARCHITECTURE.md](ARCHITECTURE.md) has the rules every part keeps and a map of the code, and [docs/decisions/](docs/decisions/) records why the main choices were made.

## Privacy

Credentials live only in macOS Keychain. Financial data is in SQLite files readable only by your user; they aren't encrypted, so use FileVault. Document originals are sealed with AES-256-GCM under a key in Keychain.

Data leaves the Mac only for the providers that supply it. Logos, place lookups, and web research are off until you turn them on, and there is no telemetry. At launch and once a day Brief asks GitHub whether a newer release exists; the request carries nothing but what any web request does, and an update installs only when you choose it.

## Install

Download the disk image from the [latest release](https://github.com/anushanandd/brief/releases/latest). It's signed with a Developer ID and notarized by Apple, and Brief offers each later release itself (Brief › Check for Updates…). It needs an Apple silicon Mac with macOS 14 or later.

## Run it

You need macOS 14 or later, Xcode, rustup (it installs the Rust that `rust-toolchain.toml` pins), Node, and pnpm. Apple Intelligence needs macOS 26.

```sh
pnpm install
pnpm dev          # browser preview with synthetic data, no credentials
pnpm tauri dev    # native app; add provider credentials in Settings → Connections
pnpm check        # format, lint, test, and build the interface
(cd src-tauri && cargo test)
```

Native builds are code-signed with one identity, Developer ID when the Keychain has it and Apple Development otherwise, so Keychain access survives rebuilds and releases; set `APPLE_SIGNING_IDENTITY` to choose another. `pnpm release` installs a release build in /Applications, and `pnpm release --publish` also notarizes it and publishes it as a GitHub release; `scripts/release.sh` lists what each needs. Run `git config core.hooksPath .githooks` once per clone to scan each commit for secrets and run the checks before each push.

## License

Brief is a personal project and isn't accepting contributions. [PolyForm Noncommercial 1.0.0](LICENSE): you may read, run, and modify it for personal, noncommercial use.

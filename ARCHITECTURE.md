# Architecture

Brief is a single-user macOS app built with Tauri. Rust owns everything that touches money or secrets: provider access, credentials, calculations, and saved state. React draws what Rust computed. Nothing runs on a server.

This page covers the rules every part of the code keeps and where the parts live. How a module works is in its header comment, next to the code; when the two disagree, the code is right. Why the main choices were made is in [docs/decisions/](docs/decisions/), and what wins when goals conflict is in [PRINCIPLES.md](PRINCIPLES.md).

```text
Plaid · SnapTrade · Alpaca · SEC EDGAR · Alpha Vantage · CNBC RSS · Apple Wallet
                 │  read-only, retried, bounded
                 ▼
      providers ──► staged provider caches
                 │
                 ▼
      financial_engine: typed records, decimal math
                 │
                 ▼
      storage: validate, then commit the state and its change
               to the change log in one SQLite transaction
                 │  on every read
                 ▼
      edits + settings ──► transaction policy ──► insights
                 │  Tauri IPC
                 ▼
      types generated from Rust ──► React Query ──► routes
```

Live market prices take a separate path. They can overlay the saved snapshot on screen, but they never change saved finance data.

## Rules every part keeps

**Money is computed once, in Rust.** Totals, forecasts, classifications, and reports are computed in Rust, and React only formats them. Each question has one answer everyone reads: the transaction policy pairs transfers (`pair_transfers`), and no other code matches the two sides of a transfer.

**Nothing is invented.** A missing, unsupported, or non-USD value stays unavailable. It's never turned into zero, and a total that's missing a part says so.

**The change log is the truth.** Every write is one change in an append-only log, committed in the same transaction as the state it produced. The saved tables are a cache of the log, and replaying the log must rebuild them (`storage::change`, `database::changesets`).

**A refresh is all or nothing.** It is staged, bounded by one deadline, validated, and committed in one transaction (`refresh`). A failure leaves the previous state usable. Plaid's sync cursor only advances once every page and the matching balance request have succeeded.

**Results are deterministic.** They depend only on the saved data, edits, settings, and the local calendar day, so tests pin the day and time zone. A market day is always a New York trading session (`session`).

**Secrets stay in Keychain.** Credentials and tokens live only in macOS Keychain. Logs, diagnostics, fixtures, and tests never contain them, raw provider messages, or real financial data. Exporting or restoring a backup needs macOS authentication.

**Billed calls are opt-in.** A Plaid product that costs money never runs on an ordinary or background refresh unless it was turned on (`providers::plaid::sync::billed_endpoints`).

**Nothing is stored where it can't be seen.** Every saved field appears in some record's raw view or in a table the Storage page opens, and the `objects` tests check this.

**Settings that change results** are saved in the finance database (`FinanceSettings`), never in the browser's storage.

**The on-device model has no authority.** Apple Intelligence can quote only what read-only Rust queries returned, and every figure in its answer is checked against them. In the investment summary it only chooses which sentence of a source to quote. Brief never states a cause in its own words or predicts a price: reasons are quoted from their publishers, and forecasts are labeled as someone else's.

**Outside its own files, Brief writes only to its own calendar.** In macOS Calendar it writes only to its "Brief" calendar, and only to events it made there (`event_calendar`). Other calendar events are read for the Calendar page and trips, and never saved.

## Where data lives

| What                                                                           | Where                                                 |
| ------------------------------------------------------------------------------ | ----------------------------------------------------- |
| The change log, the snapshot, provider caches, statements, edits, and settings | `finance-state.sqlite3`                               |
| Document originals, encrypted, with their key in Keychain                      | `documents.sqlite3`                                   |
| Dated and pre-migration copies of the finance database                         | `backups/`                                            |
| Credentials and tokens                                                         | macOS Keychain                                        |
| Market prices, news and filings, places, and search embeddings                 | their own `.sqlite3` caches                           |
| Insights and Data health                                                       | computed on every read, never saved                   |
| Display preferences                                                            | the webview's local storage (`src/lib/preference.ts`) |

Only the first two databases matter. The others are caches: if one fails, finance data is unaffected and recovery ignores it.

Every write backs the database up first. Migrations run in one transaction, after a copy of the old version is kept, and a database from a newer build is refused before anything is written. A database that can't be opened is replaced, read-only, by the newest copy that can, and writes stay blocked until it is restored. A restore uses SQLite's Backup API through the open connection, so no file a connection holds is ever renamed.

## The interface

React's types are generated from Rust's (ts-rs, into `src/lib/bindings`) and never edited by hand. Data from outside Rust, such as stream messages, local storage, and files, is checked at runtime.

React never computes a total. When a page needs one, such as a spending period or a report, it asks Rust for it. Each route's loader fetches its page's data before the page opens (`src/route-data.ts`), so moving between pages never shows a loading screen. Edits go through a few narrow commands, and each returns the updated snapshot.

Tests and the browser preview read fixtures that Rust writes, so no test computes money itself. The [regen-fixtures skill](.claude/skills/regen-fixtures/SKILL.md) explains how to regenerate them.

## Where the code is

In `src-tauri/src`:

- `providers/`: one module per data source, plus the Keychain vault and shared market sockets. It depends on nothing above it.
- `financial_engine/`: turns provider records into typed accounts, holdings, and history.
- `storage/` and `database/`: the change log, commits, validation, backups, recovery, and the schema.
- `transaction_policy`, `merchants`, `places`, `quality`: what each transaction means, computed on every read.
- `insights/`: everything else computed on read, including reports, benefits, taxes, and the assistant's tools and planner.
- `statements/`, `documents/`: imported statements and documents, read on the Mac.
- `evidence/`, `objects/`, `search/`: rules stored as data and the engine that proves them, the object graph, and search.
- `card_drafts/`, `research`: drafting card and bank rules from the open catalog, published terms, and the card's own credits. `card_drafts/apply` applies clear catalog matches by itself and never changes the user's rules.
- `market/`, `summary/`, `session`: live valuation, the investment summary, and trading days.
- `commands/`: the Tauri commands. `native/` holds the Swift bridges to Apple's frameworks.

In `src`: `lib/api.ts` (IPC), `lib/bindings/` (generated types), `router.tsx` and `route-data.ts`, `pages/` (one per route), `components/`, and `lib/` for view logic.

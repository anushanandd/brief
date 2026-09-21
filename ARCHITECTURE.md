# Architecture

Brief is a single-user macOS app with no server or cloud-sync layer. Rust owns provider access, credentials, the committed financial snapshot, and durable finance state. React owns presentation, URL state, view-only derivations, and local display preferences.

```text
Plaid / SnapTrade / Alpaca / Alpha Vantage
                      │
                      ▼
             Rust provider boundary
        credentials, retries, pagination
                      │
                      ▼
             staged provider caches
                      │
                      ▼
             Rust financial engine
       typed parsing, Decimal calculations,
       classification, reconciliation
                      │
                      ▼
       validation + one SQLite transaction
                      │ Tauri IPC
                      ▼
          Zod validation + React Query
                      │
                      ▼
                   routes
```

Live market valuation and selected-security chart history are separate from the committed refresh path. They may overlay or accompany a snapshot, but they never rewrite it.

## Invariants

- Provider credentials and access tokens stay in macOS Keychain.
- Rust is the authority for the committed snapshot and shared transaction classification.
- Refresh work is staged against a revision, validated, and committed atomically. Failure leaves the prior snapshot and provider caches usable.
- Plaid pagination mutates temporary per-Item state. A cursor and its transactions become eligible for publication only after pagination and the corresponding balance request succeed.
- Missing, unsupported, or foreign-currency values remain unavailable; they are not converted to zero.
- `FinanceProvider` owns committed data. `LiveMarketProvider` may apply only a revision- and timestamp-compatible Rust market projection.
- The portfolio market session and the selected Holdings chart have separate request owners. Selecting or leaving a chart must not replace the portfolio subscriptions.
- Spending and Platinum benefits resolve the saved spending account by ID.
- Apple Intelligence receives bounded evidence and returns ephemeral text. It is not a calculation authority and has no finance-state mutation path.

## State ownership

| State                                                                                                      | Owner                                        | Persistence                  |
| ---------------------------------------------------------------------------------------------------------- | -------------------------------------------- | ---------------------------- |
| Committed snapshot and provider caches                                                                     | Rust storage                                 | `finance-state.sqlite3`      |
| Transaction annotations and confirmed account links                                                        | Rust storage                                 | `finance-state.sqlite3`      |
| Redacted sync runs                                                                                         | Rust storage                                 | `finance-state.sqlite3`      |
| Provider credentials and access tokens                                                                     | Rust providers                               | macOS Keychain               |
| React Query snapshot and route state                                                                       | React                                        | Memory                       |
| Display names, chart settings, page order, spending account, benefit visibility, and external-logo consent | Renderer preference modules                  | WebView local storage        |
| Current portfolio valuation projection                                                                     | Rust market service and `LiveMarketProvider` | Memory only                  |
| Saved startup market observations                                                                          | Rust startup-market cache                    | `startup-market.sqlite3`     |
| Verified Holdings history                                                                                  | Rust holding-market pipeline                 | `market-prices.sqlite3`      |
| Alpha Vantage news and earnings cache                                                                      | Rust news provider                           | `news.sqlite3`               |
| Apple Intelligence answer                                                                                  | Command menu                                 | Memory only                  |
| Browser preview                                                                                            | Renderer                                     | `src/data/seed.json` fixture |

SQLite files contain financial data and are not encrypted by Keychain.

## Startup and recovery

`Storage::new` acquires `finance-state.lock` so two Brief processes cannot write the finance store concurrently. It then opens and validates `finance-state.sqlite3`. If no database exists, validated legacy JSON is imported once and left in place.

If the primary database is missing, unreadable, or newer than the app supports, Brief loads a valid retained backup into an in-memory, read-only recovery state when possible. Writes remain blocked until the user restores that state or explicitly starts a new snapshot. Replaced primary and backup files are retained before recovery changes are installed.

The renderer imports legacy annotations before requesting the annotation-aware snapshot, validates native data with Zod, and loads integration status. A compatible saved market frame may be applied after the committed snapshot is available. The native window is initially hidden and is revealed after these local startup reads settle; a three-second native watchdog prevents it from remaining hidden indefinitely. Network readiness is not part of the reveal boundary.

When configured providers exist, the renderer checks snapshot age on startup, focus, visibility changes, and a one-minute timer. A visible app silently refreshes when the committed snapshot is at least one hour old, with attempts limited to once per hour in the running process.

## Provider refresh and commit

`refresh_finance_snapshot` is one native operation:

1. Acquire the single-refresh lock and establish a 120-second operation timeout.
2. Read credentials from Keychain and copy the committed revision and provider caches.
3. Refresh providers into temporary state. A provider failure may retain its last usable cache and add a warning; a source with no safe cache cannot be silently invented.
4. Stage the candidate caches with the original revision, refresh ID, and a 120-second lease.
5. Run `financial_engine::project`. Provider compatibility JSON is decoded into typed records; malformed provider records are excluded with warnings. Monetary aggregation uses `rust_decimal::Decimal` and rounds at output boundaries.
6. Validate the typed `Snapshot`, identifiers, references, currencies, totals, histories, and reconciliation constraints.
7. Back up the last valid database and replace snapshot state, provider state, and the successful sync record in one SQLite transaction.
8. Update in-memory committed state only after the transaction succeeds.

Projection, timeout, validation, and commit failures record a safe failure code where possible and retain the prior committed state. Manual refresh progress events contain fixed labels and non-sensitive timing/count information, not credentials or financial records.

Rust applies saved annotations before assigning transaction classification. The renderer consumes that classification through `src/lib/transaction-kind.ts`; it does not infer a second transaction policy.

## Persistence

`database.rs` defines constrained SQLite tables for scalar snapshot fields, ordered entities, provider metadata, annotations, account links, sync runs, and compatibility workspace state. SQLite uses foreign keys, `STRICT` tables, full synchronous writes, and schema versioning through `PRAGMA user_version`.

Before committed state replacements and user edits, storage creates `finance-state.backup.sqlite3`. Annotation and account-link edits update their dedicated tables rather than replacing the full snapshot. Recovery and compatibility helpers preserve original files instead of deleting them.

The market, startup, and news databases are independent caches. Their failure cannot invalidate the committed finance database, and they are not part of finance recovery.

## Live market and Holdings data

When the native app is visible, Alpaca is configured, and supported held symbols exist, `LiveMarketProvider` starts one app-level market request. Rust owns the feed registry, snapshots, WebSocket lifecycle, periodic REST reconciliation, and temporary portfolio recalculation. React receives a narrow `MarketProjection` and applies it only when both `revision` and `updatedAt` match the committed snapshot. Transactions and durable histories retain their committed identity.

The startup-market cache stores a disposable same-snapshot observation frame so compatible saved values can appear before current market data arrives. Saved frames remain labeled as saved and do not claim a live connection.

Holdings starts a separate request-scoped chart pipeline. Rust publishes canonical OHLCV bars and keeps latest trade or indicative quote observations separate. Verified slices are cached in `market-prices.sqlite3`; failed refreshes leave prior cache entries intact. Chart selection updates the chart request but does not change the held-symbol portfolio subscriptions. History downloads and warming share the native two-request semaphore.

Alpha Vantage news and earnings are fetched outside the finance refresh transaction. Their cache and failures cannot alter the committed snapshot.

## Renderer boundary

`src/lib/api.ts` is the IPC client and browser fallback. Native financial snapshots and market, news, and earnings payloads are parsed by schemas in `src/lib/schema.ts` before use. A synthetic contract fixture is checked from both Rust and TypeScript to catch native/renderer field drift.

React may derive date ranges, chart geometry, filters, analytics buckets, spending periods, and presentation totals from the validated snapshot. Those values are view state and are never sent back as a replacement financial snapshot. Durable edits use narrow commands for annotations and account links.

The command menu may send a bounded local evidence object to Apple's on-device Foundation Model. Its native prompt permits repeating supplied values, forbids new financial calculations and advice, and returns text only. Requests are serialized, cancellable, limited to 12,000 characters, and time out after 45 seconds.

## Code boundaries

- `src-tauri/src/providers.rs`: provider APIs, Keychain access, linking, pagination, retries, market snapshots, news, and earnings
- `src-tauri/src/providers/market_stream.rs`: shared Alpaca socket registry
- `src-tauri/src/providers/holding_market.rs`: selected-security history and market-price cache
- `src-tauri/src/financial_engine.rs`: canonical projection and live valuation calculations
- `src-tauri/src/transaction_policy.rs`: shared transaction classification
- `src-tauri/src/finance_contract.rs`: serializable native output types
- `src-tauri/src/storage.rs`: staging, validation, recovery, and commit policy
- `src-tauri/src/database.rs`: SQLite schema and transactions
- `src-tauri/src/lib.rs`: Tauri commands and orchestration
- `src/lib/api.ts`: renderer IPC client and browser seed fallback
- `src/lib/schema.ts`: renderer trust-boundary validation
- `src/hooks/finance-provider.tsx`: committed renderer state
- `src/hooks/live-market-provider.tsx`: compatible live valuation overlay
- `src/lib/spending.ts`: view-only spending and Platinum rules
- `src/lib/analytics.ts`: view-only analytics projections

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
- A successful provider fetch does not by itself prove that brokerage balances are current. Confirmed daily net-worth observations require a recent holdings sync; unaligned totals remain provisional.

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
| On-demand Plaid recurring report                                                                           | Rust provider boundary and Settings          | Memory only                  |
| Data Health report                                                                                         | Rust deterministic projection                | Derived on request           |
| Sensitive-value screen mask                                                                                | React AppShell                               | Memory only                  |
| Browser preview                                                                                            | Renderer                                     | `src/data/seed.json` fixture |

SQLite files contain financial data and are not encrypted by Keychain. Brief restricts its app-data directory to mode `0700` and local database files to `0600` on Unix platforms.

## Startup and recovery

`Storage::new` restricts the app-data directory, acquires `finance-state.lock` so two Brief processes cannot write the finance store concurrently, then opens and validates `finance-state.sqlite3`. If no database exists, validated legacy JSON is imported once. After both the canonical database and its SQLite backup are durable, the migrated plaintext JSON sources are removed.

If the primary database is missing, unreadable, or newer than the app supports, Brief loads a valid retained backup into an in-memory, read-only recovery state when possible. Writes remain blocked until the user restores that state or explicitly starts a new snapshot. Replaced primary and backup files are retained before recovery changes are installed.

The renderer imports legacy annotations before requesting the annotation-aware snapshot, validates native data with Zod, and loads integration status. A compatible saved market frame may be applied after the committed snapshot is available. The native window is initially hidden and is revealed after these local startup reads settle; a three-second native watchdog prevents it from remaining hidden indefinitely. Network readiness is not part of the reveal boundary.

When configured providers exist, the renderer checks snapshot age on startup, focus, visibility changes, and a one-minute timer. A visible app silently refreshes when the committed snapshot is at least one hour old, with attempts limited to once per hour in the running process.

## Provider refresh and commit

`refresh_finance_snapshot` is one native operation:

1. Acquire the single-refresh lock and establish one 120-second deadline for provider work and projection. A projection that finishes after the deadline has no commit path.
2. Read credentials from Keychain and copy the committed revision and provider caches.
3. Refresh providers into temporary state. Response bodies are read incrementally and rejected above 32 MiB. Safe Plaid reads may make at most two retries with capped jitter after network or server failures. Mutations are single-attempt, and institution-level throttling is deferred to a later refresh. A provider failure may retain its last usable cache and add a warning; a source with no safe cache cannot be silently invented.
4. Stage the candidate caches with the original revision, refresh ID, bounded provider diagnostics, and a 120-second lease.
5. Run `financial_engine::project`. Provider compatibility JSON is decoded into typed records; malformed provider records are excluded with warnings. Monetary aggregation uses `rust_decimal::Decimal` and rounds at output boundaries.
6. Validate the typed `Snapshot`, identifiers, references, currencies, totals, histories, and reconciliation constraints.
7. Back up the last valid database and replace snapshot and provider state in one SQLite transaction.
8. Record the bounded sync diagnostic separately on a best-effort basis, so diagnostic persistence can never reject an otherwise valid snapshot commit, then update in-memory committed state.

Provider errors retain structured status, type, code, retryability, safe endpoint, attempt count, retry deadline, and a one-way request reference until the Tauri boundary; raw provider messages are not surfaced. Projection, timeout, validation, and commit failures record a safe phase and error code where possible and retain the prior committed state. Diagnostics never store credentials, payloads, query strings, account identifiers, financial values, or raw provider messages. Manual refresh progress events contain fixed labels and non-sensitive timing/count information.

Rust applies saved annotations before assigning transaction classification. The renderer consumes that classification through `src/lib/transaction-kind.ts`; it does not infer a second transaction policy. Transfer descriptions and account-number fragments never establish income by themselves.

For supported USD SnapTrade accounts, Rust values the account from the cash balance and every position, excluding positions already counted in cash, and exposes the same native calculation as separate current cash and invested values. Missing or unsupported cash or position data makes the current account value and breakdown unavailable and the combined total incomplete; the provider-reported total is comparison evidence only. Individual brokerage graphs show that reported total with its last holdings sync date (or fetch date when sync time is unavailable). Combined graphs omit it. Historical brokerage charts use provider history. A brokerage holdings sync older than 30 minutes prevents a new daily net-worth observation and keeps the current combined chart provisional.

## Persistence

`database.rs` defines constrained SQLite tables for scalar snapshot fields, ordered entities, provider metadata, annotations, account links, sync runs, and compatibility workspace state. Schema version 4 added bounded `details_json`; version 5 clears raw provider messages that older builds may have retained in diagnostic fields. SQLite uses foreign keys, `STRICT` tables, full synchronous writes, and schema versioning through `PRAGMA user_version`.

Before committed state replacements and user edits, storage creates `finance-state.backup.sqlite3`. Annotation and account-link edits update their dedicated tables rather than replacing the full snapshot. Recovery and compatibility helpers preserve original files instead of deleting them.

Portable backup uses SQLite's live Backup API and exports the complete canonical finance database, including provider caches, annotations, account links, sync history, and compatibility workspace state. It excludes Keychain credentials, renderer display preferences, and the disposable market, startup, and news databases. Exported files are mode `0600` on macOS but are not encrypted. Restore first copies the selected database through SQLite's Backup API into the app-data directory, checks the Brief application ID, supported schema version, SQLite integrity, foreign keys, and typed finance state, then retains the current database. In normal operation SQLite's Backup API installs the validated source through the existing primary connection; recovery mode may rename only when no connection has the primary open. The restored revision is advanced so stale live projections cannot overlay it.

The calculation-version upgrade removes legacy dated history corrections and rebuilds history from cached provider data.

The market, startup, and news databases are independent caches. Their failure cannot invalidate the committed finance database, and they are not part of finance recovery.

## Data Health

Data Health is a read-only Rust projection over the committed snapshot and redacted sync-run history. It reports recovery state, incomplete valuations, possible duplicate accounts, provider and quote freshness, limited investment history, cost-basis coverage, and unresolved classifications. Findings have deterministic IDs and navigation targets; the renderer does not independently reimplement check policy or mutate financial state to clear an issue.

## Live market and Holdings data

When the native app is visible, Alpaca is configured, and supported held symbols exist, `LiveMarketProvider` starts one app-level market request. Rust owns the feed registry, snapshots, WebSocket lifecycle, periodic REST reconciliation, and temporary portfolio recalculation. The calculation keeps the committed cash value fixed while repricing eligible positions. The initial request fetches the prior 24 hours of one-minute bars even when the market is closed; React receives narrow `MarketProjection` updates and applies them only when both `revision` and `updatedAt` match the committed snapshot, retaining the first observation and five-minute portfolio points for the recent chart segment. Transactions and durable histories retain their committed identity.

The startup-market cache stores a disposable same-snapshot observation frame so compatible saved values can appear before current market data arrives. Saved frames remain labeled as saved and do not claim a live connection.

Holdings starts a separate request-scoped chart pipeline. Rust publishes canonical OHLCV bars and keeps latest trade or indicative quote observations separate. Verified slices are cached in `market-prices.sqlite3`; failed refreshes leave prior cache entries intact. Chart selection updates the chart request but does not change the held-symbol portfolio subscriptions. History downloads and warming share the native two-request semaphore.

Alpha Vantage news and earnings are fetched outside the finance refresh transaction. Their cache and failures cannot alter the committed snapshot.
News reads are cache-only and do not require credentials or consume provider quota; only the explicit Refresh news action sends a news request. Saved stories remain visible while disconnected or rate-limited. The shared news cache counts requests before sending them and reserves five of the free-tier allowance for earnings and credential checks. A daily-limit response with unused local capacity permits one short recheck before a full-day provider cooldown.

## Renderer boundary

`src/lib/api.ts` is the IPC client and browser fallback. Native financial snapshots and market, news, and earnings payloads are parsed by schemas in `src/lib/schema.ts` before use. A synthetic contract fixture is checked from both Rust and TypeScript to catch native/renderer field drift.

React may derive date ranges, chart geometry, filters, analytics buckets, spending periods, and presentation totals from the validated snapshot. Those values are view state and are never sent back as a replacement financial snapshot. Rust persists the local calendar date used by each projection so annotation reprojection cannot change month or year after a timezone change. Durable edits use narrow commands for annotations and account links.

Expected activity is a deterministic, non-persistent renderer projection over posted transactions. It groups by account, direction, and stable merchant identity; recognized Platinum credits instead use the existing benefit ID so issuer-level website enrichment cannot merge separate credits. It suppresses habitual spending, separates fixed-amount series before cadence detection, uses detailed descriptions for generic banking streams, and retains negative loan or card payments even though native classification correctly treats them as transfers. Variable debt payments use their observed range, and recent posting days determine the calendar anchor. Pending transactions are only unambiguous matches, never recurrence evidence. Predictions use observed credit amounts rather than benefit allowances and account for weekends and U.S. federal holidays for bank-scheduled activity. Stale snapshots, stale Plaid accounts, ambiguous pending matches, and unsupported series produce no invented state.

Settings may request Plaid's `/transactions/recurring/get` endpoint for connected banking Items. Rust reads Item tokens from Keychain, validates the provider response into a narrow stream report, and returns no credentials. The renderer validates the IPC result and displays provider status, cadence, dates, amounts, and evidence count for manual comparison. Reports remain in memory, are never committed, and do not affect Expected activity or refresh success.

The native macOS app menu sends a `toggle-sensitive-values` event for ⌘H. `AppShell` owns the temporary renderer mask, which obscures personal numbers and balance-history charts while leaving public security prices, market changes, and price histories visible. It never changes the financial snapshot or exports.

The native projection retains Plaid's readable transaction location fields and payment channel when supplied, trims empty values, and keeps them separate from transaction and posting dates. Activity displays location and channel in separate columns, with a street address available from the location icon. Filtered CSV exports include both fields. Older committed snapshots rebuild them from the saved provider cache through the calculation-version upgrade; missing provider fields remain unavailable.

The command menu may send a bounded question and a separate local evidence object to Apple's on-device Foundation Model. The native prompt identifies the question as the user request and the evidence as untrusted JSON data, permits repeating supplied values, forbids new financial calculations and advice, and returns text only. Requests are serialized, cancellable, limited to 12,000 combined characters, and time out after 45 seconds.

## Code boundaries

- `src-tauri/src/providers/mod.rs`: shared provider contracts and response parsing
- `src-tauri/src/providers/http.rs`: structured provider errors, retry decisions, and redacted diagnostics
- `src-tauri/src/providers/{plaid,snaptrade,alpaca,alpha_vantage,vault}.rs`: provider-specific access, linking, and credential boundaries
- `src-tauri/src/providers/market_stream.rs`: shared Alpaca socket registry
- `src-tauri/src/providers/holding_market.rs`: selected-security history and market-price cache
- `src-tauri/src/financial_engine.rs`: canonical projection and live valuation calculations
- `src-tauri/src/health.rs`: deterministic committed-data diagnostics
- `src-tauri/src/transaction_policy.rs`: shared transaction classification
- `src-tauri/src/finance_contract.rs`: serializable native output types
- `src-tauri/src/storage.rs`: staging, validation, recovery, and commit policy
- `src-tauri/src/database.rs`: SQLite schema and transactions
- `src-tauri/src/lib.rs`: Tauri app state and command registration
- `src-tauri/src/refresh.rs`: staged provider refresh and commit orchestration
- `src-tauri/src/market.rs`: live portfolio and selected-security orchestration
- `src/lib/api.ts`: renderer IPC client and browser seed fallback
- `src/lib/schema.ts`: renderer trust-boundary validation
- `src/hooks/finance-provider.tsx`: committed renderer state
- `src/hooks/live-market-provider.tsx`: compatible live valuation overlay
- `src/lib/spending.ts`: view-only spending and Platinum rules
- `src/lib/analytics.ts`: view-only analytics projections

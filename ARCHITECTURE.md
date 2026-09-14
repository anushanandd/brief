# Architecture

Brief is a single-user, local-first macOS desktop app with no server. Rust is the only financial authority. React owns presentation, interaction, runtime validation of view data, and local UI preferences.

```text
Plaid / SnapTrade / Alpaca
             │
             ▼
Typed Rust provider boundary
  ├─ Keychain credentials read on demand
  ├─ staged provider caches and cursor/page state
  └─ malformed optional records quarantined
             │
             ▼
Canonical Rust financial engine
  ├─ Decimal calculations and explicit rounding boundaries
  ├─ account identity and ambiguity candidates
  ├─ dates, freshness, currency, coverage, and provenance
  └─ reconciled immutable ViewSnapshot
             │
             ▼
One SQLite transaction
  ├─ normalized snapshot entities
  ├─ provider caches and Plaid cursors
  ├─ annotations and confirmed account links
  └─ bounded redacted sync runs
             │ Tauri IPC
             ▼
React routes / FinanceProvider
  └─ Home-only LiveMarketProvider requests a Rust-calculated
     ephemeral valuation projection
```

## State ownership

| State                                                                                                                       | Owner                                  | Persistence             |
| --------------------------------------------------------------------------------------------------------------------------- | -------------------------------------- | ----------------------- |
| Canonical snapshot and calculations                                                                                         | Rust financial engine                  | Normalized SQLite rows  |
| Provider caches, Plaid Item cursors, and refresh metadata                                                                   | Rust providers/storage                 | SQLite metadata row     |
| Transaction annotations and confirmed account links                                                                         | Rust storage                           | Dedicated SQLite tables |
| Redacted committed/failed sync runs                                                                                         | Rust storage                           | Bounded SQLite table    |
| Credentials and provider access tokens                                                                                      | Rust providers                         | macOS Keychain          |
| Stable renderer query state                                                                                                 | `FinanceProvider` / React Query        | Memory                  |
| Live market projection and intraday chart points                                                                            | Rust projection / `LiveMarketProvider` | Memory only             |
| Chart order/visibility, display names, account start dates, spending account, benefit visibility, and external-logo consent | Frontend preference modules            | Local storage           |
| Browser preview data                                                                                                        | `src/data/seed.json`                   | Repository fixture      |

Keychain protection applies to credentials and tokens, not `finance-state.sqlite3`. SQLite is the local application file and transaction mechanism; it does not add a service, multi-user system, or synchronization layer.

## Data flows

### Startup and recovery

1. Rust acquires `finance-state.lock`, preventing two Brief processes from writing the store.
2. Rust opens `finance-state.sqlite3`, loads the normalized state, and validates the reconstructed snapshot and schema version.
3. When no SQLite database exists, Rust imports the current legacy JSON state (or older individual JSON files) once, creates SQLite, and leaves the source files untouched.
4. If the primary is missing, unreadable, or incompatible, Rust loads a validated SQLite or legacy JSON backup into an in-memory read-only recovery state. Refreshes, annotations, and account links remain blocked until explicit recovery.
5. Before returning a saved snapshot, Rust rebuilds an older calculation version from the committed provider cache in one local transaction; this does not fetch providers or invent a new observation. Rust then overlays durable annotations and recalculates the annotation-aware monthly projection. React obtains `get_finance_snapshot`, validates the immutable view contract with Zod, and applies display-only preferences. It also keeps annotation query state for immediate interaction feedback. Browser development uses the deterministic seed instead.
6. After rendering the saved snapshot, the native app silently starts the normal refresh operation when connected providers exist and the committed snapshot is at least one hour old. The same age check runs when the app becomes visible or focused, with failed or successful automatic attempts limited to once per hour in the current process. Recovery mode and browser development never auto-refresh.

Recovery retains existing primary and backup database bytes under unique names before replacement. It does not modify Keychain credentials.

### Provider refresh and commit

`refresh_finance_snapshot` is one renderer command and one native operation:

1. Rust takes the single-refresh mutex, reads credentials from Keychain, and snapshots the committed revision and caches.
2. Plaid, SnapTrade, and scheduled Alpaca history work run against temporary state. Individual provider failures retain the last usable committed cache and produce a warning; a refresh that lacks any safe source cache fails without committing.
3. Rust stages the candidate caches with a UUID, base revision, and 120-second lease. This stage never crosses IPC.
4. `financial_engine::project` parses typed provider DTOs, quarantines malformed optional records, applies confirmed account links, performs exact decimal calculations, constructs histories and spending inputs, and attaches calculation/source provenance.
5. Storage validates identifiers, references, currencies, timestamps, ordering, uniqueness, totals, histories, spending reconciliation, and calculation version.
6. The last valid database is backed up. A single SQLite transaction replaces the normalized projection and provider state and records the successful sync run. Only then does in-memory committed state change.
7. Rust returns the committed immutable view snapshot to React.

During a manual refresh, Rust emits presentation-only `finance-refresh-progress` events for connected-account provider work. Events contain fixed task labels plus non-sensitive durations, transaction-request counts, and retry counts—never credentials, provider identifiers, or financial records—and React uses them for an indeterminate progress toast. Automatic refreshes follow the same native path without showing the toast.

Timeouts and provider/projection failures leave the prior snapshot and caches usable. Failures are recorded with safe error codes; warning details in the durable ledger are reduced to counts.

Plaid applies `/transactions/sync` pages only to a temporary per-Item cache. Items refresh with bounded concurrency; within a cash or card Item, transaction pagination and the real-time balance request also run concurrently. Results merge in stable Item order, and each Item's staged cache publishes only after both operations succeed. If Plaid reports a mutation during pagination, Brief restarts from the original committed cursor. The balances request asks supported institutions for recent data, but `sourceEffectiveAt` remains null when Plaid supplies no effective timestamp; Brief records its own fetch and commit times separately.

SnapTrade positions are required for a successful provider update. Balance history and activity can fall back independently to their prior account caches. Activity pagination fixes the run's end time, deduplicates stable activity IDs/content, rejects no-progress pages, and has a bounded page count. Rust projects supported dividend, interest, transfer, reinvestment, fee, and tax records into the shared activity contract while keeping trades separate. Its beta balance-history data remains `provider-estimated` and never receives a “reported return” label.

Safe read-like provider requests retry once for transient transport, 429, and 5xx failures with a short bounded backoff. Mutating link/exchange requests are not automatically replayed.

### Canonical financial projection

Provider JSON is retained only as a compatibility cache. It is parsed into typed Rust records at the financial-engine boundary; one bad optional record is skipped with a non-sensitive warning rather than poisoning every usable record.

All aggregation uses `rust_decimal::Decimal`. Rounding occurs when the cents-scale view contract is emitted. Non-USD balances are isolated and make totals incomplete instead of being silently converted. Nullable prices, quantities, and balances remain unavailable. Rust also calculates known position cost basis, unrealized P/L, imported year-to-date investment income, and year-to-date sale proceeds, attaching complete, partial, or unavailable cost-basis coverage. When complete SnapTrade activity contains usable executions and FIFO inventory reconciles to the current share count and provider-reported remaining basis, Rust emits per-sale matched basis, gain, and gain percentage plus an explicitly estimated account realized P/L by consuming prior purchase lots. Dividend activities identified as reinvestments contribute acquisition lots rather than income activity, with a narrow reconciliation tolerance for SnapTrade's differing activity and position quantity precision. Missing purchases, transfers, corporate actions, incomplete executions, or a position mismatch suppress the estimate; broker tax results may also differ when the user selected specific lots. The renderer uses returned values for display and does not recalculate durable net worth or account performance.

Account identity is explicit. Provider institution/mask agreement or matching portfolio composition plus a close provider balance may create `possibleDuplicateAccounts`, but heuristics never delete either account. Cash-only pairs require matching balances and compatible empty/cash position evidence. A user-confirmed Plaid-to-SnapTrade link is stored in `account_links`; only that durable mapping permits the linked duplicate source to be excluded on the next projection.

Plaid transactions preserve:

- `occurredOn`: authorization date when available, otherwise posting date
- `postedOn`: provider accounting/posting date
- `date`: current product display choice

Plaid-resolved merchant logo and website metadata may cross the native view boundary, but the renderer ignores remote artwork by default. Enabling actual logos is an explicit local preference and permits only Logo.dev and Plaid's merchant-logo host; the setting explains that displaying a mark discloses its ticker or merchant-domain identifier to that service.

Spending and benefit periods use `postedOn`. Rust reapplies durable transaction annotations whenever it builds a saved or live view and recalculates its monthly spending projection. Interactive period, merchant, benefit, and possible-subscription views then filter those canonical transactions for the saved spending account. Subscription identification is a conservative renderer-local classification of similar posted charges at regular monthly, quarterly, or annual intervals; it does not mutate or extend canonical provider data. The Spending workspace recognizes the recurring Amex autopay description and requires at least two consistent monthly payments before inferring statement closes with the card's documented 15-day close-to-autopay offset. Observed payments establish exact historical closes; the median inferred closing day fills missing cycles. The current statement series is anchored to the committed provider balance and reconstructed backward through its posted activity. Historical statement openings use the pay-in-full AutoPay inside the cycle when available. Absent or irregular evidence falls back to the local calendar. Benefit rules remain calendar-based and carry an effective version and contractual timezone; current hotel-credit boundaries use `America/Chicago`. The renderer automatically counts positive, posted merchant-rule matches toward the applicable benefit cap.

### History, performance, and provenance

Complete, fully refreshed current totals become observed net-worth points. Existing observations are retained and deduplicated by date. Brief also builds an explicitly transaction-derived series for each supported Plaid cash or credit account by walking its current provider balance backward through fetched posted transactions, stopping at that account's imported coverage boundary. These per-account histories cross the immutable view boundary for cash and card charts. For the all-accounts chart, Brief combines them with SnapTrade balance history or evidence-gated stock-plan reconstruction and starts at the latest common coverage date. Known stock-plan dates before the first reconciled acquisition contribute zero rather than shortening the whole portfolio's history. Recorded observations override estimates on matching dates; incomplete or unavailable account history suppresses the combined reconstruction rather than silently omitting an account.

Brokerage series distinguish what the source supports:

- SnapTrade balance history: `provider-estimated`; `value-with-comparisons` when balance-history and activity coverage are complete, otherwise `value-only`
- Stock-plan history reconstructed from quantities and split-adjusted Alpaca prices: `estimated`; acquisition evidence supplies comparison flows when reconstruction is complete
- Missing or incompatible coverage: `unavailable`

A `value-with-comparisons` chart shows the opening observed value plus known external cash flows and a VOO total-return comparison subjected to the same known flows. Rust also emits each interval’s value change excluding those known flows, with an explicitly estimated percentage. Plain and directional cash-transfer activity types are included; a flow between valuation dates is applied at the next observation. These are labeled chart references, not calculated portfolio returns. React deterministically selects cash graph markers from immutable transaction evidence using materiality thresholds and does not amount-threshold imported buy or sell trades. Money moving in and stock purchases use labeled green dots; money moving out and stock sales use labeled red dots. A sale label includes its Rust-supplied P/L percentage when available. Price changes are not marked. Matching cross-account transfer pairs are collapsed, and temporal proximity between a sale and withdrawal is never treated as proof of causation. A `value-only` chart omits them. The schema separately reserves evidence-gated `time-weighted` and `modified-dietz` methods, but the current provider evidence is intentionally insufficient, so neither is emitted. XIRR is not conflated with portfolio performance.

Alpaca security bars use split adjustment; benchmark bars use all corporate-action adjustments for total-return comparison semantics. Each projection records source and adjustment mode. Yahoo history is not a silent fallback. When adjusted history is unavailable, Brief keeps the previous compatible cache or marks history unavailable.

### SQLite persistence

`database.rs` creates constrained `STRICT` tables:

- `state_meta`: schema/revision and provider compatibility caches
- `snapshot_scalar`: scalar view fields and projection metadata
- `snapshot_entity`: ordered, uniquely identified accounts, holdings, transactions, trades, movements, histories, performance, and duplicate candidates
- `annotations`: durable user classifications/review state
- `account_links`: confirmed canonical identity mappings
- `sync_runs`: bounded, redacted committed and failed runs

Refresh replaces related state in one transaction. Annotation and account-link edits update only their dedicated tables after retaining a backup; they do not rewrite the full financial history. The database uses an explicit `user_version` migration boundary, full synchronous writes, uniqueness and shape constraints, JSON validity checks for compatibility payloads, and SQLite's backup API.

### Live market data

`FinanceProvider` holds committed state. Home mounts `LiveMarketProvider`; other routes use only committed data. Home asks Rust for supported USD equity/ETF/ADR/closed-end-fund quotes. For each rolling seven-day change, Rust uses Alpaca's close from seven calendar days earlier, or the latest preceding market close when that date is not a trading day. References are fetched through a paginated multi-symbol operation, selected by bar timestamp rather than response order, and cached in memory by ticker and reference date; newly seen tickers fetch only their missing references, while failed or incomplete attempts are throttled for five minutes. Rust rejects a quote older than a holding's known price time, but never treats an account-level position-refresh timestamp as a price timestamp. It calculates daily and seven-day price changes, applies decimal valuation to a cloned snapshot, and returns that temporary projection with each chosen reference close/date and the feed/session metadata. Home exposes the reference in a tooltip. Neither quotes nor the projection are persisted.

Polling stops when Home is not active, the window is hidden, or the authoritative market session says polling is unnecessary. Logs observes existing quote state and does not trigger polling.

### On-device explanations

Home always displays one “What changed” card with Key changes, Daily summary, and Weekly summary sections. Daily and rolling seven-day activity totals, income sources, and leading holding moves are deterministic. Material evidence for Key changes is also deterministic: a complete latest net-worth change of at least $500 or 0.25%; a rolling seven-day holding move of at least 8% regardless of position size, or at least 4% with an estimated portfolio effect of $100 or 0.1% of net worth; a card payment of at least $250; or another weekly transaction of at least $500 or 10% of monthly spending. With no qualifying evidence, Home shows a static quiet-state message and does not invoke Apple Intelligence. Otherwise, deterministic code constructs the displayed summary and a compact evidence packet; significant holdings may add up to five recent Alpaca news summaries as possible contributors. Apple Intelligence generation is gated by model availability and keyed by the qualifying evidence within its Monday-starting week. Its optional context bullet appears in the Weekly summary and is discarded if it contains digits, a refusal, or the explicit `NO_CONTEXT` sentinel, so rendered financial values remain deterministic. It cannot access credentials/provider caches, and its output remains outside financial calculation and persistence paths.

### Provider linking and credentials

Settings opens Plaid or SnapTrade linking in the system browser and polls an in-memory native session. A session has a five-minute deadline, cancellation signal, credential generation, and concurrent-poll guard. Results are checked before local mutation. Each accepted Plaid token is persisted atomically before optional metadata work.

Credential operations serialize through one mutex but do not retain a decrypted vault for the application lifetime. Each operation reads the current Keychain value, mutates if authorized, persists it, and drops the in-memory value. Replacing credentials invalidates stale link/refresh results. Local forgetting removes only saved credentials and does not call a provider revocation endpoint.

## Boundaries

- `src-tauri/src/providers.rs`: external APIs, signatures, credentials, pagination, retries, and compatibility caches
- `src-tauri/src/financial_engine.rs`: typed provider parsing and all canonical financial calculations
- `src-tauri/src/storage.rs`: staged revision/lease control, recovery, validation, and commit policy
- `src-tauri/src/database.rs`: SQLite schema, migrations from in-memory state, transactions, backups, and sync ledger
- `src-tauri/src/lib.rs`: Tauri commands and refresh orchestration
- `src/lib/api.ts`: narrow IPC client and deterministic browser fallback
- `src/lib/schema.ts`: renderer runtime view contract
- `src/hooks/`: committed view state and Home-scoped live projection state
- `src/lib/spending.ts`: display filtering, effective-dated benefit matching, and annotation-aware local UI totals; it does not produce the durable canonical snapshot

Update this file when data flow, state ownership, persistence, IPC, provider behavior, or polling changes.

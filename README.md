# Brief

Brief is a private, local-first personal finance app for macOS. React renders the interface; Tauri and Rust connect read-only providers, calculate the canonical financial projection, and store it in a local SQLite application file. Brief has no hosted backend or cloud synchronization.

Plaid and SnapTrade credentials and access tokens stay in macOS Keychain and are read on demand rather than retained in a process-wide credential cache. The finance database contains sensitive account history and is not encrypted by Keychain. Use normal macOS account protection and FileVault; database encryption remains a separate threat-model decision.

## Financial behavior

Rust parses provider records into typed inputs, quarantines malformed optional records, performs decimal arithmetic, reconciles the projection, and commits it. The WebView receives a narrow, immutable view snapshot and never sends a calculated finance snapshot back for persistence.

- Missing and foreign-currency values are not converted to zero. Known USD values form the displayed total, which is marked incomplete when necessary.
- Credit availability is never counted as an asset. Unsupported or unpriced positions remain visible without invented valuations.
- Plaid transactions retain both authorization/occurrence dates and accounting/posting dates.
- All-accounts history preserves real refresh observations and supplements earlier dates with an explicitly estimated series. Cash and credit balances are reconstructed backward through fetched posted Plaid transactions, while investments use their existing provider-backed or evidence-gated value histories.
- SnapTrade balance history is labeled provider-estimated and shown as value history, not as a calculated investment return. When history and activity coverage are complete, its chart can also show known net deposits and an adjusted VOO comparison.
- Security reconstruction uses split-adjusted Alpaca history; the VOO benchmark cache uses total-return-adjusted Alpaca history. Brief does not silently replace either with Yahoo data.
- Possible cross-provider duplicates are suggested from provider identity or matching portfolio-and-balance evidence, and remain visible until the user confirms a Plaid-to-SnapTrade account link in Settings.
- Live quotes and rolling seven-day holding changes remain Home-scoped and ephemeral. Hovering a 7 Days value shows the exact Alpaca reference close and date. Rust calculates the temporary valuation projection; it never changes the committed snapshot.
- Spending uses the saved spending account, local calendar boundaries, integer-minor-unit aggregation in the view layer, and effective-dated benefit rules. Benefit matches are evidence for review, not proof that Amex issued a credit.
- Financial and merchant logos use private local marks by default. Settings can opt in to actual remote artwork; when enabled, Logo.dev or Plaid receives the relevant ticker or merchant-domain identifier as each mark is displayed.

Home combines accounts, holdings, source-labeled value history with one-week, one-month, three-month, and all-time views, live market context, and a three-part “What changed” overview: threshold-driven key changes, today's imported activity, and a rolling seven-day summary. Performance graphs mark material imported income, expenses, transfers, sales, current holding moves, and flow-adjusted portfolio movements; estimated events remain labeled, matching internal transfers are collapsed, and nearby sales and withdrawals are shown without claiming causation. Command-Control-Left/Right cycles through the chart's date views. Apple Intelligence can add optional on-device context when the verified evidence supports something useful. Deterministic code renders every amount, comparison, and threshold; Apple Intelligence can add a short context sentence from supplied evidence but cannot calculate or mutate finance data.

Accounts provides overview, investment, and cash-and-card workspaces. Individual account pages adapt to the account type and present balances, performance, positions, income and sales, spending, and activity as one card-based statement. Investment details include provider-backed cost basis, unrealized P/L, positions, income, sales proceeds, and brokerage activity with explicit coverage labels. Brief breaks down realized sales by execution, proceeds, matched FIFO basis, and estimated P/L only when complete imported executions—including dividend reinvestments—reconcile to the current position; otherwise it keeps the sale visible without inventing profit. Broker tax results may differ when specific lots were selected.

Activity separates the unified timeline, selected-account spending analysis, possible recurring subscriptions, brokerage trades, durable balance changes, and Platinum benefits into focused views. Subscription identification conservatively flags similar posted charges at regular monthly, quarterly, or annual intervals and always labels them as possible. Spending compares the current period with an equally elapsed prior period and ranks categories and merchants from the saved spending account. Transactions with a valid HTTPS merchant website open that site from their activity row. Only user-confirmed reimbursements reduce configured benefit caps, and eligibility must still be verified with the issuer.

Logs separates provider checks from effective/source, fetched, and committed timestamps. Unknown source times stay unknown. It also shows a bounded, redacted durable sync-run ledger so failed projection or provider runs do not disappear when the app closes.

## Run

Install dependencies and start the browser preview with deterministic sample data:

```sh
pnpm install
pnpm dev
```

Start the native app for real provider connections:

```sh
pnpm tauri dev
```

Brief shows the saved snapshot immediately, then silently refreshes connected providers when the last committed sync is at least one hour old, including when returning to the app. Manual refresh remains available.

Configure Plaid, SnapTrade, or Alpaca in Brief → Settings. Settings groups account connections, Home and display preferences, activity and benefit rules, and privacy and diagnostics. It also supports local display names, Plaid repair, local forgetting, explicit duplicate-account linking, chart visibility/order, and spending preferences. Display names and links affect only Brief. Forgetting a connection does not revoke provider access.

## Local persistence and recovery

The primary store is `finance-state.sqlite3`. A transaction commits provider cursors/caches, the reconciled snapshot, annotations, account links, and a redacted sync result. Before a durable mutation, Brief copies the last valid database to `finance-state.backup.sqlite3`.

Existing `finance-state.json` data is imported once when no SQLite store exists; the JSON source is left untouched. If the primary database is missing, corrupt, or from an unsupported schema, Brief exposes a validated backup in read-only recovery mode. Restore or explicitly start fresh before writes resume. Replaced database files are retained with a unique `.retained-….sqlite3` name.

## Verify

```sh
pnpm check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml
```

## Documentation

- [AGENTS.md](AGENTS.md): repository rules and verification guidance
- [ARCHITECTURE.md](ARCHITECTURE.md): system boundaries, state ownership, persistence, and data flows

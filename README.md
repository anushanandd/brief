# Brief

Brief is a private, local-first personal finance app for macOS. React renders the interface; Tauri and Rust connect read-only providers, calculate the canonical financial projection, and store it in a local SQLite application file. Brief has no hosted backend or cloud synchronization.

Plaid and SnapTrade credentials and access tokens stay in macOS Keychain and are read on demand rather than retained in a process-wide credential cache. The finance database contains sensitive account history and is not encrypted by Keychain. Use normal macOS account protection and FileVault; database encryption remains a separate threat-model decision.

## Financial behavior

Rust parses provider records into typed inputs, quarantines malformed optional records, performs decimal arithmetic, reconciles the projection, and commits it. The WebView receives a narrow, immutable view snapshot and never sends a calculated finance snapshot back for persistence.

- Missing and foreign-currency values are not converted to zero. Known USD values form the displayed total, which is marked incomplete when necessary.
- Credit availability is never counted as an asset. Unsupported or unpriced positions remain visible without invented valuations.
- Plaid transactions retain both authorization/occurrence dates and accounting/posting dates.
- All-accounts history preserves real refresh observations and supplements earlier dates with an explicitly estimated series. Cash and credit balances are reconstructed backward through fetched posted Plaid transactions and exposed as transaction-derived account histories, while investments use their existing provider-backed or evidence-gated value histories.
- SnapTrade balance history is labeled provider-estimated and shown as value history, not as a calculated investment return. When history and activity coverage are complete, its chart can also show known net deposits and an adjusted VOO comparison.
- Security reconstruction uses split-adjusted Alpaca history; the VOO benchmark cache uses total-return-adjusted Alpaca history. Brief does not silently replace either with Yahoo data.
- Possible cross-provider duplicates are suggested from provider identity or matching portfolio-and-balance evidence, and remain visible until the user confirms a Plaid-to-SnapTrade account link in Settings.
- Live quotes and rolling seven-day holding changes remain Home-scoped and ephemeral. Position-refresh timestamps are kept separate from actual price timestamps so they do not suppress valid market quotes. Hovering a 7 Days value shows the exact Alpaca reference close and date. Rust calculates the temporary valuation projection; it never changes the committed snapshot.
- Spending uses the saved spending account, recurring Amex autopay dates and their known 15-day statement-close lead for inferred statement boundaries, local calendar boundaries as a fallback, integer-minor-unit aggregation in the view layer, and effective-dated benefit rules. Posted positive transactions matched by those local rules count toward benefit caps automatically.
- Financial and merchant logos use private local marks by default. Settings can opt in to actual remote artwork; when enabled, Logo.dev or Plaid receives the relevant ticker or merchant-domain identifier as each mark is displayed.

Home combines accounts, holdings, value history with inferred or locally edited account start dates and one-week, one-month, three-month, and all-time views, live market context, and a three-part “What changed” overview: threshold-driven key changes, today's imported activity, and a rolling seven-day summary. All-time value graphs begin at the selected account's saved or inferred start date. Dots beneath the graph switch accounts, while Command-Left/Right cycles through the same choices. Its account summary shows each current value with a seven-day value change when complete history or holding references support it. Its Accounts, Holdings, and Latest activities headings open their corresponding primary workspaces. Performance graphs use labeled green dots for threshold-selected money moving in and imported stock purchases, and labeled red dots for threshold-selected money moving out and imported stock sales; stock trades have no minimum marker amount. Sale labels include P/L percentages only when complete imported execution evidence supports them. Price changes are not marked, matching internal transfers are collapsed, and nearby sales and withdrawals are shown without claiming causation. Bare number keys 1–6 open Home, Accounts, Spending, Holdings, Activity, and Settings, while `?` shows the shortcut reference. W/M/Q/A switches week, month, quarter, and all-time views from anywhere on a page with a range chart. Command-Control-Left/Right cycles through the chart's date views; Command-[ and Command-] move backward and forward through page history. Apple Intelligence can add optional on-device context when the verified evidence supports something useful. Deterministic code renders every amount, comparison, and threshold; Apple Intelligence can add a short context bullet from supplied evidence but cannot calculate or mutate finance data.

Accounts combines account navigation and activity into one account-switching overview, with All accounts first and non-spending accounts ordered from highest to lowest current value. All accounts is the default, and each selection has its own URL so browser history works. Choosing an account updates a compact first-page set of cards for its balance, chart, account details with account-appropriate income metrics, latest activity, and—for investment accounts—holdings with prices and P/L plus realized-sale estimates. Holdings and Activity are primary navigation workspaces with complete searchable ledgers; Activity paginates by the rows that fit in the window and supports arrow buttons or arrow keys. `/` focuses searchable ledgers, Escape clears or leaves search, and J/K moves through rows with Enter opening an available row destination. Account previews open either ledger with the originating account already selected, and both ledgers support category filters. Cash charts reconstruct balances backward from the current provider value through fetched posted transactions, label the result transaction-derived, and stop at the imported coverage boundary. Credit accounts instead chart card spending by month so changes in a liability balance are not presented as spending. Focused investment, cash, and individual account routes remain available. Brief breaks down realized sales by execution, proceeds, matched FIFO basis, and estimated P/L only when complete imported executions—including dividend reinvestments—reconcile to the current position; otherwise it keeps the sale visible without inventing profit. Broker tax results may differ when specific lots were selected.

Spending is the dedicated workspace for the saved spending account, which is omitted from the Accounts workspace but remains visible in Home’s account summary. Its full-width overview card places the selected-period total above cumulative card activity, with two supporting metrics above category spending in a fixed companion column. Current statement balance and the activity line are anchored to the provider's current card balance; historical statement openings use the following pay-in-full AutoPay amount. Purchases raise the line and have red transaction markers; payments and credits lower it and have green markers. Hovering or focusing a marker shows its transaction date, merchant, and amount. Multiple transactions posted on one date receive deterministic positions within that day because provider evidence does not include an intraday time. When at least two recognized Amex autopays establish a consistent monthly cadence, Brief infers each statement close 15 days before the posted payment; M/Q/Y use one, three, or twelve such statement cycles. The payment therefore lowers activity inside the following statement rather than defining its boundary. Without sufficient evidence, the controls retain calendar periods. A compact M/Q/Y/A indicator shows the active keyboard-selected range. It then stacks every benefit enabled in Settings and all matched credits by benefit on the left while Latest activity spans both rows on the right, expands to match their content, and uses the app-wide activity row with category and right-aligned date. M/Q/Y/A select the range, while Left/Right moves the reference cycle backward or forward. Focused transaction and subscription history remains available for deeper inspection. Subscription identification conservatively flags similar posted charges at regular monthly, quarterly, or annual intervals and always labels them as possible. Transactions with a valid HTTPS merchant website open that site from their activity row. Posted positive benefit-rule matches reduce configured caps automatically.

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

Brief shows the saved snapshot immediately, then silently refreshes connected providers when the last committed sync is at least one hour old, including when returning to the app. Manual refresh reports connected-account provider status and timing while the prior snapshot remains available.

Configure Plaid, SnapTrade, or Alpaca in Brief → Settings. Settings groups account connections, Home and display preferences, spending and benefit rules, and privacy and diagnostics. It also supports local display names and account start dates, Plaid repair, local forgetting, explicit duplicate-account linking, chart visibility/order, and spending preferences. Display names and links affect only Brief. Forgetting a connection does not revoke provider access.

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

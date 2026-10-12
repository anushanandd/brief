# Architecture

Brief is a single-user macOS app built with Tauri. Rust owns everything that touches money or secrets: provider access, credentials, calculations, and durable state. React owns presentation and view state. Nothing runs on a server.

```text
Plaid · SnapTrade · Alpaca · SEC EDGAR · Alpha Vantage · CNBC RSS
                 │  read-only, retried, bounded
                 ▼
      providers ──► staged provider caches
                 │
                 ▼
      financial_engine: typed records, Decimal math
                 │
                 ▼
      storage: validate, then commit in one SQLite transaction
                 │  on every read
                 ▼
      annotations + rules + settings ──► insights
                 │  Tauri IPC
                 ▼
      Zod validation ──► React Query ──► routes
```

Live market prices travel a separate path and may overlay a committed snapshot, but never rewrite it.

## Invariants

- **Secrets:** credentials and provider tokens live only in macOS Keychain. Logs, diagnostics, fixtures, and tests never contain credentials, tokens, raw provider messages, or real financial data.
- **Atomic refresh:** a refresh is staged, bounded by one 120-second deadline, validated, and committed in a single transaction. Any failure leaves the previous snapshot and caches usable.
- **Plaid paging:** cursor and page changes stay temporary until pagination and the matching balance request both succeed.
- **No invented values:** missing, unsupported, or non-USD values stay unavailable. They are never converted to zero, and an incomplete total says so.
- **One policy, in Rust:** transaction classification, forecasts, statement cycles, benefit rules, attention items, period changes, and Data health are computed in Rust. React formats them and never reimplements them.
- **Determinism:** results depend only on the snapshot, annotations, rules, settings, and the local calendar day. Each projection stores its calendar date so a timezone change can't move a month boundary. The one exception is the investment summary, which also reads the market sessions saved for its period: a closed session's layers stay as they were computed.
- **Settings that change results** (spending account, account names and start dates, benefit visibility, the billed Plaid products, background refresh, notifications, the card product each extra card is tracked as, the tax profile, and account tax treatments) live in the finance database, never in renderer storage.
- **Spending account by ID:** statements and Platinum benefits use the saved account ID, never provider ordering. Spending may cover one account or every USD account, and never treats another card as the saved one.
- **Billed calls are opt-in:** Plaid live balances, Liabilities, Investments Transactions, and Recurring Transactions never run on an ordinary or background refresh unless the user chose them. Each is its own Plaid charge, and its Settings row says so.
- **Apple Intelligence has no authority:** in answers it may quote only results from read-only Rust tools. It cannot calculate or change finance data, and it writes nothing in the investment summary. Each answer is returned with the tool queries that ran, so its figures can be checked. A query pinned to Today is rerun by Rust from saved data, without the model.
- **Brief never infers a cause:** the investment summary's reasons are publishers' headlines, quoted as published with the publisher, time, and link, and shown only when a headline names a holding that led the day and says it moved that way, or is a market wrap saying the market moved its way.

## State

| State                                                                                                                  | Owner                                | Stored in                |
| ---------------------------------------------------------------------------------------------------------------------- | ------------------------------------ | ------------------------ |
| Snapshot, provider caches, imported statement files, annotations, rules, finance settings, account links, sync history | Rust `storage`                       | `finance-state.sqlite3`  |
| Credentials and provider tokens                                                                                        | Rust `providers::vault`              | macOS Keychain           |
| Insights and Data health                                                                                               | Rust `insights`, `health`            | Derived on each read     |
| Live portfolio valuation                                                                                               | Rust `market` + `LiveMarketProvider` | Memory                   |
| Saved market frame for fast startup                                                                                    | Rust `startup_market`                | `startup-market.sqlite3` |
| Verified security history                                                                                              | Rust `providers::holding_market`     | `market-prices.sqlite3`  |
| Market sessions (layers, fits, groups, quiet state, calibration counts, events)                                        | Rust `summary`, `storage`            | `finance-state.sqlite3`  |
| News, SEC filings, insider trades, reported quarters, period summaries, and earnings                                   | Rust `providers::news_cache`         | `news.sqlite3`           |
| Display preferences (theme, page order, masking defaults, logo consent, pinned queries)                                | React                                | WebView local storage    |
| Browser preview data                                                                                                   | React                                | `src/data/seed.json`     |

Only `finance-state.sqlite3` is authoritative. The other databases are disposable caches. Their failure never affects finance data, and recovery ignores them. The app-data directory is `0700` and database files are `0600`, but SQLite files are not encrypted.

## Refresh and commit

`refresh_finance_snapshot` runs once at a time:

1. Take the refresh lock and start the 120-second deadline.
2. Read credentials from Keychain and copy the committed revision and caches.
3. Fetch each provider into temporary state. A provider failure keeps its last usable cache and adds a notice. A provider with no cache can't be invented.
4. Stage the candidate caches against the original revision.
5. Project with `financial_engine::project`. Malformed provider records are skipped with a notice.
6. Validate the snapshot: identifiers, references, currencies, totals, and histories.
7. Back up the database, then replace the snapshot and caches in one SQLite transaction.
8. Record a redacted sync diagnostic separately, so diagnostics can never block a valid commit.

Provider specifics:

- **Plaid** balances come from Plaid's cached `/accounts/get`. The billed `/accounts/balance/get` runs only on an explicit live-balance refresh. Reconstructed balance history claims the requested 730-day window only after Plaid reports its historical pull complete; until then it starts at the earliest transaction received. Transactions use Plaid's v2 category taxonomy, and an Item cached under another taxonomy is rebuilt once from a fresh cursor. When enabled, `/liabilities/get` supplies statement dates; its failure is a notice, never a failed refresh. Response bodies are capped at 32 MiB, and safe reads retry at most twice.
- **Plaid transactions** are requested with the institution's original descriptor. A cache saved in an older request format (`PLAID_TRANSACTION_FORMAT`) is rebuilt once from a fresh cursor, as a taxonomy change is.
- **Plaid Investments Transactions** (opt-in) reads each investment Item's last 24 months of trades, dividends (qualified or not, when the institution says), fund distributions, fees, and contributions. Its failure is a notice; holdings still commit. Gains are estimated only for accounts whose whole available history was read.
- **Plaid Recurring Transactions** (opt-in) saves Plaid's streams. A series every covering stream reports inactive and tombstoned is no longer forecast; Brief's own policy still decides everything else.
- **Alpaca corporate actions** (splits and cash dividends for every held or traded symbol) are fetched once a day and committed with the refresh, because splits change lots and gains. A position's pre-split trades are restated only when the restated history explains the shares held and the recorded one doesn't, so a broker's already-adjusted history is never adjusted twice.
- **Institution logos** are looked up on Logo.dev by exact domain, from the website Plaid (`/institutions/get_by_id` with optional metadata, once per connection) or SnapTrade (each brokerage login) reports, only when logos are enabled. Without one, an account shows its type's icon.
- **SnapTrade** accounts are valued from cash plus positions. The provider-reported total is evidence only. A holdings sync older than 30 minutes keeps the combined history provisional. Its balance history reaches back at most a year and skips days, so a skipped day carries the prior balance plus known deposits and withdrawals. Shares a stock plan allocates arrive with no cash amount; they count as a deposit worth that day's Alpaca close, and without a close they are left out.

Net-worth history counts a transfer between two tracked accounts once, even while its legs post on different days. When a provider files one leg of such a transfer as income or spending, an equal and opposite transfer leg in another tracked account within five days makes it a transfer too, unless its category was chosen by hand or by rule. A total's history (net worth, the combined brokerages, an account's holdings) starts at the earliest history among its parts, and each part counts from its own first point; a part known only by today's value leaves the total without history, since it would enter as a jump.

**Imported statements** (CSV or XLSX exports, `statements/`) fill an account's history from before its provider's first reported activity; from that day on the provider is the source of truth. The saved file is authoritative and is parsed again on every projection, so which rows count follows the provider's current history. Rust finds the table using `statements/vocabulary.json`, checks the rows against the statement's printed total and running balances, and refuses a file whose total disagrees. A row a provider reports a few days later is matched and not added; rows two overlapping statements share count once, while identical rows within one statement all count. Rows for a Plaid cash or card account are projected as Plaid records, so they share Plaid's categories, the transaction policy, and the reconstructed balance history, which then reaches back past Plaid's window. An import or removal re-projects saved provider data without the network and commits like a refresh.

Refresh triggers: a visible app refreshes silently when the snapshot is at least an hour old. Opt-in background refresh (`background.rs`) keeps the app alive with its window hidden and refreshes when the snapshot is at least 20 hours old. While background refresh is on, a menu bar item reopens, refreshes, or quits the hidden app. With notifications also on, a background refresh posts at most three notifications for new needs (payment due, cash shortfall, credit expiring, a card closing its statement with a high share of its limit in use, a possible duplicate charge), once each per run; they can appear on a locked screen, so they never contain an amount.

## Transactions and insights

Provider categories come from `financial_engine/categories.json`, a versioned map from Plaid's primary and detailed codes to about twenty top-level categories, an optional `subcategory` label (shown only when Plaid's confidence is not `UNKNOWN`), and a transaction kind. When a provider category applies, that kind decides the transaction's meaning; the name and text policy covers refunds, unknown codes, and manual or rule categories. A positive issuer statement-credit descriptor (the benefit rules' credit descriptors, or Amex/Platinum "… Credit"/"… Reimbursement") is always a credit, because Plaid files these under the benefit's spending category, a transfer, or a fee.

Annotations also carry tags, a split, a refund link, and whether the transaction is someone else's money. A split divides one transaction's amount between categories: its parts share the amount's sign and sum to it exactly in cents, every category total counts the parts, and a split saved for another amount is ignored. A refund linked to a purchase takes that purchase's category and reduces spending. Someone else's money (kind `owed`), set by hand, is neither spending nor income: a purchase made for someone, or what they paid back, Zelle included. A refund of such a purchase is someone else's too, and so is a card credit the benefit rules pair with it.

Category at read time, in order: a manual annotation, then the most specific category rule, then the provider category. A manual or rule category drops the provider subcategory. A posted transaction inherits the annotation of the pending transaction it replaced (`pendingTransactionId`). Outgoing payment-app transfers (Plaid's `TRANSFER_OUT_TRANSFER_OUT_FROM_APPS`, or Zelle) become "Payments To People" and count as spending, mirroring incoming Zelle as income. The committed snapshot keeps provider categories, and React reads classification through `src/lib/transaction-kind.ts`.

`insights::derive` computes these from the typed snapshot, once edits are applied, and never persists anything. A read parses the committed snapshot once into the typed contract; the policy, insights, and Rust's own readers work on that, and it is serialized once for the renderer. `Storage` reuses a derived view for at most 30 seconds while saved data, edits, and the calendar day are unchanged:

- `forecast`: recurring bills and income across accounts
- `statement`: card statement dates, from Plaid Liabilities when available, otherwise inferred
- `benefits`: Amex Platinum credits from the rule data in `insights/amex-platinum.json` (see [docs/platinum-benefits.md](docs/platinum-benefits.md)), with posted credits set against the annual fee since its last charge. Another credit card is tracked only when the user assigns it a card product; the spending card needs no assignment. Each closed window carries its verdict (missed when the card used the benefit, unclaimed when it never did, settling, or unknown), and the windows with allowance left come in the order to use them
- `refundSuggestions`: unlinked refunds from the last 90 days with exactly one purchase they could repay; the transaction inspector offers the link
- `plan`: month-to-date spending, cash coverage, and runway. Cash starts from available balances where the bank reports them, so pending holds count, and a pending bill already in that balance isn't subtracted again
- `attention`: at most three payment-due or cash-shortfall alerts
- `brief`: every need for action (the alerts above, a used card credit about to expire, cash expected to run short), which notifications report, and the expected events Today lists as coming up
- `subscriptions`: subscription series with a charge due within a year, each normalized to a monthly cost, and their monthly total. A fixed charge repriced once is still a subscription, forecast at its new price, with the change reported
- `cash`: known cash carried 90 days through the recurring activity that reaches cash accounts, with a range from the amounts each series has posted; unavailable while any cash balance is unknown. It starts from available balances, as `plan` does
- `credit`: utilization per card and across cards with a reported limit, each card's next statement close (issuer cycle for the spending card, otherwise a month after the last statement, marked estimated), and interest charged on the last statement
- `anomalies`: possible duplicates (exactly two identical posted charges from one merchant on one day, at least $10, in the last 60 days) and unusual amounts (far above a merchant's median by its robust spread, with five earlier charges). Marking a transaction reviewed dismisses either
- `trips`: runs of in-store spending away from home regions (regions with a share of in-store purchase days across several months), with the placeless travel and transport charges on their days
- `taxes`: this year's and last year's taxable interest, dividends, short- and long-term gains, and wash sales (same ticker within 30 days in any account; a tax-advantaged replacement makes the loss permanent), harvest values, and open wash-sale windows. With a tax profile, an estimate stacked on the user's other income using `insights/tax-tables.json`, a range while dividends are unclassified. A year without tables has no estimate
- `dividends`: each held security's last-year dividend pace, yield, next announced payment, and annual income
- `recurringCheck`: Plaid's mature streams Brief doesn't forecast, for Data health
- `returns`: time-weighted and money-weighted returns per brokerage account beside the benchmark, only where the history records deposits and covers the whole window
- `periods`, `exposure` (`reconcile`): the combined brokerages' statement for each chart window, the one source of the Investments page's period figures: start + deposits − withdrawals + investment gain = end, daily-linked time-weighted ("how your investments did") and money-weighted ("how your money did") returns beside the S&P 500 over the same days, annualized only from a year, a waterfall whose realized gains, dividends, interest, fees, and taxes are booked from the ledger with unrealized change as the plug, and drawdown, volatility, and beta; and how concentrated the holdings are (effective holdings, top-five weight)
- `allocation`: known USD positions by security, instrument kind, and account
- `recap`: for each chart period (the trailing week, month, three months, year, and all history), spending against the period of the same length before it, income, and the top categories; a period that starts before saved activity is marked partial
- `spending`: for each scope Spending offers (an account, or all of them) and each period of a kind (statement, rolling week, month, quarter, year) back to the start of its history, what is usual by this point by category (up to three earlier periods cut at the same day, when history covers two), the current period carried to its end at its pace once a week of it has passed, and each statement's opening balance. The renderer sums each period's activity and draws it
- `changes`: for each chart period (a week, a month, three months, a year, all history), each account's balance change from its daily history and the number of posted transactions; an account whose history starts later is left out and the period says so
- `interest`: each cash account's interest posted in each chart period, with its rate a year on today's balance, and its monthly net flow over all its posted history once that spans two months
- `lots`: open tax lots per position, estimated first-in-first-out from saved trades (each sale's gain is split into short- and long-term parts by the lots it consumed, and each lot carries a tax-if-sold estimate with a tax profile); shares no saved purchase explains make a position partial, and history that contradicts the position has no lots; a fully explained position also has a money-weighted return
- `movers`: the day's change by position from saved prices, not the live overlay
- `accountsTotal`: the total and history without the saved spending card, which Spending presents on its own. Today's totals and the Accounts "All accounts" view use it; the snapshot's `netWorth` still includes the card. Live points subtract the card balance, which is not market-driven.
- `securities`, `allocation`, `longTermSoon`: holdings merged across accounts with weight and gain, exposure with a concentration flag (one security at 20% or more), and lots turning long-term within 60 days (taxable accounts only)
- `investmentAttention`: only what calls for a decision: a concentrated holding, taxable lots turning long-term with a gain, and taxable losses large enough to harvest, plus the holdings large enough that their earnings dates matter
- `ledger`: read-only queries for Apple Intelligence, capped at 25 rows, including credit cards, taxes, trips, subscriptions, and dividend income from the insights above

`insights/golden.rs` replays results recorded from the former TypeScript implementation.

## Startup and recovery

`Storage::new` locks the app-data directory against a second Brief process, then opens and validates the finance database. Legacy JSON is imported once and deleted only after a valid SQLite backup exists.

If the database is missing, unreadable, or from a newer version, Brief opens a valid retained backup read-only. Writes stay blocked until the user restores it or starts fresh, and original files are kept either way.

The window stays hidden until local reads finish, with a three-second watchdog. Startup never waits on the network.

## Backups and erasure

- **Backups** (`.briefbackup`): SQLite Backup API copies of the whole finance database, excluding Keychain, display preferences, and caches. Export and restore require a single-use macOS authentication. Restore validates the application ID, schema version, integrity, and typed state before installing through the open connection, then advances the revision.
- **Preferences** (`.briefsettings`): a bounded JSON file of display preferences, with no financial data.
- **Erasure:** clears the finance database, retained copies, and caches, but keeps Keychain credentials. Removing credentials is a separate action.

## Live market data

When the app is visible and Alpaca is configured, one app-level market service streams held symbols, reprices positions, and sends `MarketProjection` updates. React applies an update only when its `revision` and `updatedAt` match the committed snapshot. The Holdings chart has its own request-scoped pipeline: it keeps canonical bars separate from latest trades and quotes, and never replaces the portfolio subscriptions.

Earnings (Alpha Vantage) are fetched only on request and spend a local daily budget. A holding's news refreshes when it is opened with saved stories at least 30 minutes old, at most once a minute per ticker, leaving Alpaca's shared per-minute limit to the market stream. A refresh fetches Alpaca (Benzinga) stories and, when an SEC contact email is saved, the company's current and periodic reports from EDGAR (8-K, or 6-K from foreign issuers, and 10-Q and 10-K), whose terms require that contact in every request. Filings appear among the company's stories, so a holding without recent filings shows nothing extra. Rust drops generated filler and analyst ratings and, each time stories are read, tags each story's scope and events from its headline and scores its relevance to the company from 0 to 100 (how few tickers it names, and whether its headline names the ticker or the company's EDGAR name); a story below 40 only mentions the company, as broker notes on peers and roundups do, and never counts as evidence. On launch, and at most every ten minutes, `refresh_portfolio_news` saves the past week's stories for every held stock from one paged Alpaca request and checks EDGAR for the holdings whose filings are oldest, so summaries are ready before a holding is opened. It also reads CNBC's market-section RSS feeds (`providers::feeds`), whose headlines name the causes of market, macro, and theme moves that may name no holding (a report on a private company, say); their tickers are read from the companies a headline names. Each holding's recent Form 4s are read once each (open-market purchases and sales only, with whether a sale was under a 10b5-1 plan). Moves are compared with SPY and a sector fund chosen from the company's SEC industry code (an approximation). The Investments overview reuses this: `get_portfolio_today` prices held positions, SPY, and known sector funds in one Alpaca snapshot to rank each position's dollar effect, `get_portfolio_sectors` groups held stocks by sector, asking EDGAR once about stocks it has never classified. The summary's period report splits the period among the holdings: each day's start-of-day weight times the security's daily return from closes adjusted for splits and dividends, with cash and anything unpriced as one more row so a day's parts sum to its return, Carino-smoothed so the period's parts sum to the period's return. Dollars rank the return drivers and each security's own return the top movers; a holding whose saved trades don't explain its shares is listed as unpriced, never guessed.

`get_investment_summary` describes a returns window for the portfolio or one holding; Rust writes every sentence, and the renderer only styles them. News volume tracks company size, not importance, so nothing ranks by how much was written. One model of "unusual" serves today, the period, and a holding's page: each session's move in layers (below), whose own part is measured in the holding's own typical day (a robust standard deviation), so a volatile stock's ordinary month is ordinary and a steady stock's real move stands out. Each session's record (`summary::sessions`) holds every holding's market, group, and own parts in percent and, when its shares are known, dollars, with the fit behind them (fund, betas, R², window) and the groups that moved; a closed session's record is saved in the finance database, past sessions are filled from a year of closes before the period's first, and today's stays provisional until the close. A period's layers are its sessions' summed, a holding's period kind follows them (own when its own part was at least a typical period, with its group only when the group's part was the larger), and an event's reaction is the holding's own part that session and the next. A beta from under 120 days of history is shrunk toward its prior (the industry fund's beta to the market; for a group, one for the industry fund and zero for another). Each record also counts the session's fund groups, loose groups, and unusual own moves, and Diagnostics shows the rates, so thresholds are measured before they are tuned. Prices on this path are adjusted for splits and dividends. Every day here is a New York trading session (`session`, with NYSE holidays and early closes computed by rule): a story or filing is priced by the first session that closes after it, and a period runs through the latest session that has opened. While that session trades, the latest price stands in for its close; before the open and after the close, today is its close, never a pre-market or after-hours price, and the summary says which. Events come from an event log (`summary::eventlog`) kept with each closed session in six families, each event with its provenance (sourced, observed, or reported), its own time, its source's, and Brief's first save: the ledger's trades and cash activity; corporate actions from the refresh's daily Alpaca read; EDGAR reports by accession number, an amendment folding into what it amends, and chains (an earnings report and its quarterly filing, a deal and its completion) of which only the ends are listed; market-wide days from a reviewed calendar (`summary/eventlog/macro-calendar.json`, Federal Reserve decisions; BLS dates are added only from the published schedule) and published rules (NYSE early closes, S&P 500 rebalances); stories, clustered per company per day with the first report anchoring each; and observed flags (a holding's own move of at least 2.5 typical days, volume twice its median, a 52-week extreme, a fall from the high crossing 5, 10, or 20%). A fact reported twice is listed once: the ledger's credit over the action that expected it, an action or filing over news about it, and a flag only as evidence for the event it measured. Events rank by visible components compared in order (dollars on the portfolio, the holding's own move, the kind's typical effect, whether it was the first report), never a combined score, and on the portfolio each stock supplies at most one. A minor event the stock plainly ignored is left out. A session is quiet for a holding with no sourced event, its own move under 1.5 typical days, and ordinary volume, and for the portfolio when nothing was flagged and its dollars were within one of its typical days. Half an hour after each close, or on the next launch, a close job (`close.rs`) refreshes the portfolio's stories and filings and records that session and any missing in the month before, once per session; it never calls Plaid. The portfolio also names the stocks whose period was unusual for them, whatever their size. Your trades and dividends come from the snapshot; a dividend counts toward a holding only when its activity names the ticker. A company's latest quarter comes from EDGAR's XBRL facts (two requests a week per ticker, only with a saved SEC contact): revenue, the same calendar quarter a year earlier, and net margin, each only when the company tagged it. Each session's layers (`summary::layers`) are each holding's beta times the S&P 500, its beta times its group fund's move beyond the market, and the rest, its own, in dollars. A holding's group fund is whichever sector or theme fund (chips and software apart from Technology) best fits its moves beyond the market in both halves of the past year; its SEC industry code's fund keeps the place unless another fits clearly better. A group moved when its fund's day beyond the market was noticeable for the fund and most members went along; holdings that moved the same way beyond the market together, whose own moves tend to go together, form a group too. The day itself is told in a few lines (`summary::brief`): the portfolio's (or the holding's) move in percent and dollars beside the S&P 500's, then the holdings that led it (up to three moving its way, each at least 15% of its dollars); a day under 0.25% is one line. Up to two headlines (`summary::headlines`) follow, from saved stories between the previous session's close and the next open: for each leading holding, the newest whose headline names it and says it moved its way (or, about one or two companies, says no way), never one that points the other way or only restates the move; then, when the S&P 500 moved at least 0.3%, the newest market wrap saying it moved its way. A quiet day gets none. The renderer warms the portfolio's summary in the background on launch. Each run is logged in memory with its stages' timings and the provider failures it saw (Diagnostics); a part that couldn't load and a year of prices the request's page limit cut short are listed as unavailable with their symbols. Data health warns when saved stories are over two days old or the latest summary run failed, and notes a calibration rate out of range (after a month of sessions, a fund flagged more than three times a month).

## Renderer boundary

`src/lib/api.ts` is the only IPC client. Every native payload is parsed by Zod in `src/lib/schema.ts`. A synthetic fixture (`src/data/fixtures/native-finance.json`) is checked from both Rust and TypeScript to catch contract drift.

React may derive view state (date ranges, filters, chart geometry, range totals in cents) but never sends a snapshot back. Edits go through narrow commands: annotations, settings, rules, and account links. Each returns the updated snapshot.

## Codemap

**Rust (`src-tauri/src`)**

- `lib.rs`: app state and startup; `commands/`: the Tauri commands by area (finance, backups, integrations, assistant, investments); `window.rs`: the main window and `brief://` links; `files.rs`: user-chosen files, read within a size bound and written only as new private files
- `refresh.rs`: refresh orchestration
- `providers/`: one module per provider, plus `http` (errors, retries, diagnostics), `vault` (Keychain), `market_stream` (shared sockets), `holding_market` (security history), `news` (stories and their relevance), `feeds` (CNBC RSS), `sec` (EDGAR), `insiders` (Form 4 trades), `portfolio` (today's move and sectors), `corporate_actions` (Alpaca's splits and dividends), `closes` (daily closes for the summary), `fundamentals` (EDGAR's reported quarters), and `news_cache` (one connection to `news.sqlite3`, shared by news, EDGAR, and the earnings budget)
- `summary/`: a returns window described, from saved data and prices: `sessions` in layers, `eventlog`, and the day's `brief` with its `headlines`
- `financial_engine/`: the projection, one phase per module. `records` (typed provider data), `accounts`, `holdings`, `transactions` (Plaid), `activities` (SnapTrade), `imported` (statement rows), `investments`, `gains` (split restatement and first-in-first-out gains), `history`, `performance`, `values`
- `statements/`: reading, checking, and planning imported statement files
- `storage/`: staging and commit. `annotations` (edits to transactions), `backup` (backups, restore, recovery), `legacy` (pre-database files), `validate` (checks every snapshot)
- `database.rs`: the finance database; `database/migrations`: its schema, migrated forward in place
- `transaction_policy.rs`: read-time classification
- `insights/`: forecasts, statements, benefits, plan, attention, cash, returns, allocation, recap, lots, period changes, refunds, and ledger
- `health.rs`: Data health checks
- `market.rs`, `startup_market.rs`: live valuation and the saved startup frame
- `numeric.rs`: cents and medians, rounded one way everywhere
- `close.rs`: the market close job
- `session.rs`: New York trading sessions, the one definition of a day for market data
- `settings.rs`, `rules.rs`, `background.rs`, `notify.rs` (opt-in notifications and the menu bar item), `native_menu.rs`, `authentication.rs`, `foundation_model.rs`

**React (`src`)**

- `lib/api.ts`, `lib/schema.ts` (a barrel over `lib/schema/`): IPC and validation
- `lib/preference.ts`: one stored display preference; every preference module builds on it
- `hooks/finance-provider.tsx`, `hooks/live-market-provider.tsx`: committed data and the live overlay
- `pages/`: one file per route
- `components/settings/`: the Settings dialog's sections
- `components/`: shared UI
- `lib/`: view-only derivations (spending periods, analytics, filters, formatting)
- `styles.css`: imports the partials in `styles/` in cascade order

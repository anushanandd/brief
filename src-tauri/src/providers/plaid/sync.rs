//! One refresh of every Plaid Item. Connections sync concurrently and independently; each
//! Item's rows merge by account, so a connection that fails keeps the accounts it synced
//! last time while the others update. Billed products (live balances, Liabilities, Recurring,
//! Investments Transactions) are requested only when `PlaidSyncOptions` asks for them.

use super::*;

/// Plaid holds up to 24 months of investment transactions.
const INVESTMENT_HISTORY_DAYS: i64 = 730;
const INVESTMENT_HISTORY_PAGES: usize = 20;
/// Pages of `/transactions/sync` one refresh may walk.
const SYNC_PAGES: usize = 100;
/// Attempts at a cursor walk when Plaid reports the ledger changed under it.
const SYNC_ATTEMPTS: usize = 3;
const MUTATED_DURING_PAGINATION: &str = "TRANSACTIONS_SYNC_MUTATION_DURING_PAGINATION";

impl Providers {
    pub async fn sync_plaid(
        &self,
        cache: &mut PlaidCache,
        previous: Option<&PlaidData>,
        options: PlaidSyncOptions,
        progress: &(dyn Fn(PlaidRefreshProgress) + Sync),
    ) -> Result<PlaidData, ProviderError> {
        let Some(credentials) = self
            .read_secret::<PlaidCredentials>(PLAID_CREDENTIALS_KEY)
            .map_err(|_| ProviderError::invalid_response("Plaid", "/credentials", 1))?
        else {
            return Ok(PlaidData::default());
        };
        let items = self
            .read_secret::<Vec<PlaidItem>>(PLAID_ITEMS_KEY)
            .map_err(|_| ProviderError::invalid_response("Plaid", "/credentials", 1))?
            .unwrap_or_default();
        cache
            .items
            .retain(|id, _| items.iter().any(|item| &item.item_id == id));
        let count = items.len();
        let work = items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let staged = cache.items.get(&item.item_id).cloned().unwrap_or_default();
                (index, item, staged)
            })
            .collect::<Vec<_>>();
        let mut results = stream::iter(work.into_iter().map(|(index, item, mut staged)| {
            let credentials = &credentials;
            async move {
                let sync = ItemSync {
                    providers: self,
                    credentials,
                    item: &item,
                    options,
                    index,
                    count,
                    progress,
                };
                let result = sync.run(&mut staged).await;
                (index, item, staged, result)
            }
        }))
        .buffer_unordered(PLAID_ITEM_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
        results.sort_by_key(|(index, _, _, _)| *index);

        let mut data = PlaidData::default();
        let mut legacy_failure = false;
        let mut first_error = None;
        for (_, item, mut staged, result) in results {
            match result {
                Ok(mut fresh) => {
                    // Notices belong to this refresh only, never to the reusable item cache.
                    data.warnings.append(&mut fresh.warnings);
                    data.refreshed_account_ids.extend(
                        account_ids(fresh.accounts.iter().chain(&fresh.investment_accounts))
                            .into_iter()
                            .map(|id| format!("plaid:{id}")),
                    );
                    staged.data = Some(fresh.clone());
                    staged.error = None;
                    cache.items.insert(item.item_id.clone(), staged);
                    merge_plaid_data(&mut data, fresh);
                }
                Err(error) => {
                    let message = error.to_string();
                    data.diagnostics.push(error.diagnostic());
                    first_error.get_or_insert(error);
                    let cached = cache.items.entry(item.item_id.clone()).or_default();
                    cached.error = Some(message.clone());
                    let institution = item
                        .institution_name
                        .as_deref()
                        .unwrap_or("Plaid connection");
                    data.warnings.push(format!("{institution}: {message}"));
                    match &cached.data {
                        Some(saved) => merge_plaid_data(&mut data, saved.clone()),
                        None => legacy_failure = true,
                    }
                }
            }
        }
        // Older caches have no Item/account mapping. Preserve their aggregate until each
        // connection has been refreshed once; overlay only identified successful accounts.
        if legacy_failure {
            if let Some(previous) = previous {
                let mut retained = previous.clone();
                retained.warnings = data.warnings.clone();
                retained.refreshed_account_ids = data.refreshed_account_ids.clone();
                merge_plaid_data(&mut retained, data);
                return Ok(retained);
            }
            return Err(first_error
                .unwrap_or_else(|| ProviderError::invalid_response("Plaid", "/refresh", 1)));
        }
        Ok(data)
    }

    /// Every investment transaction Plaid holds for an Item (up to 24 months), with the
    /// securities they reference. Pages are bounded so a refresh can't run away.
    pub(super) async fn plaid_investment_transactions(
        &self,
        credentials: &PlaidCredentials,
        access_token: &str,
    ) -> Result<(Vec<Value>, Vec<Value>), ProviderError> {
        const ENDPOINT: &str = "/investments/transactions/get";
        const PAGE: usize = 500;
        let invalid = || ProviderError::invalid_response("Plaid", ENDPOINT, 1);
        let today = Utc::now().date_naive();
        let start = today - Duration::days(INVESTMENT_HISTORY_DAYS);
        let (mut transactions, mut securities) = (Vec::new(), Vec::new());
        for page in 0..INVESTMENT_HISTORY_PAGES {
            let (response, _) = self
                .plaid_read(
                    credentials,
                    ENDPOINT,
                    json!({
                        "access_token": access_token,
                        "start_date": start.to_string(),
                        "end_date": today.to_string(),
                        "options": { "count": PAGE, "offset": page * PAGE }
                    }),
                )
                .await?;
            let batch =
                required_array(&response, "investment_transactions").map_err(|_| invalid())?;
            securities.extend(required_array(&response, "securities").map_err(|_| invalid())?);
            let received = batch.len();
            transactions.extend(batch);
            let total = response
                .get("total_investment_transactions")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            if received < PAGE || transactions.len() >= total {
                return Ok((transactions, securities));
            }
        }
        Err(invalid())
    }

    /// The institution's website, so its logo can be looked up by exact domain, and whether it
    /// supports Statements, asked of Plaid once per connection. A failed lookup is retried on the
    /// next refresh.
    async fn ensure_institution_url(
        &self,
        credentials: &PlaidCredentials,
        item: &PlaidItem,
        cache: &mut PlaidItemCache,
    ) {
        if cache.institution_url_checked && cache.statements_supported.is_some() {
            return;
        }
        let Some(id) = item.institution_id.as_deref() else {
            cache.institution_url_checked = true;
            cache.statements_supported = Some(false);
            return;
        };
        if let Ok((value, _)) = self
            .plaid_read(
                credentials,
                "/institutions/get_by_id",
                json!({
                    "institution_id": id,
                    "country_codes": ["US"],
                    "options": { "include_optional_metadata": true }
                }),
            )
            .await
        {
            cache.institution_url = value
                .pointer("/institution/url")
                .and_then(Value::as_str)
                .map(str::to_owned);
            cache.institution_url_checked = true;
            cache.statements_supported = Some(institution_supports(&value, "statements"));
        }
    }
}

/// One connection's refresh: the Item, its credentials, what to fetch, and where to report.
struct ItemSync<'a> {
    providers: &'a Providers,
    credentials: &'a PlaidCredentials,
    item: &'a PlaidItem,
    options: PlaidSyncOptions,
    index: usize,
    count: usize,
    progress: &'a (dyn Fn(PlaidRefreshProgress) + Sync),
}

/// A finished walk of `/transactions/sync`: the ledger as Plaid now has it.
struct SyncedTransactions {
    cursor: String,
    transactions: BTreeMap<String, Value>,
    update_status: Option<String>,
    requests: usize,
    retries: usize,
}

/// The Item's cash and card accounts, with the ids the other products may ask about.
#[derive(Default)]
struct CashAccounts {
    accounts: Vec<Value>,
    ids: Vec<String>,
    credit_ids: Vec<String>,
}

impl ItemSync<'_> {
    /// Everything this Item contributes to the refresh. An investments Item carries holdings;
    /// a transactions Item carries cash and card accounts with their ledger and, when asked,
    /// recurring streams and card statement details.
    async fn run(&self, cache: &mut PlaidItemCache) -> Result<PlaidData, ProviderError> {
        self.providers
            .ensure_institution_url(self.credentials, self.item, cache)
            .await;
        let website = cache.institution_url.clone();
        if self.item.investments {
            return self.sync_investments(website.as_deref()).await;
        }
        let ((), (balances, fetched_at)) =
            tokio::try_join!(self.sync_transactions(cache), self.fetch_balances())?;
        let accounts = required_array(&balances, "accounts")
            .map_err(|_| self.invalid(self.balance_endpoint()))?;
        let cash = self.cash_accounts(accounts, website.as_deref(), &fetched_at);
        let mut data = PlaidData {
            transactions: cache
                .transactions
                .iter()
                .filter(|transaction| {
                    account_id(transaction).is_some_and(|id| cash.ids.contains(&id))
                })
                .cloned()
                .collect(),
            accounts: cash.accounts,
            ..PlaidData::default()
        };
        if !cache.history_complete && !cash.ids.is_empty() {
            data.warnings.push(format!(
                "{}: older transactions are still loading",
                self.institution("A connection")
            ));
        }
        if let Some(start) = &cache.transaction_history_start {
            data.transaction_history_start
                .extend(cash.ids.iter().map(|id| (id.clone(), start.clone())));
        }
        if self.options.recurring && !cash.ids.is_empty() {
            self.sync_recurring(&mut data).await;
        }
        if self.options.liabilities && !cash.credit_ids.is_empty() {
            self.sync_liabilities(&mut data, &cash.credit_ids).await;
        }
        Ok(data)
    }

    /// Holdings, their securities, and the accounts that hold them. Investment activity is a
    /// billed product that fails on its own: holdings still commit without it.
    async fn sync_investments(&self, website: Option<&str>) -> Result<PlaidData, ProviderError> {
        const ENDPOINT: &str = "/investments/holdings/get";
        let investments = self
            .measured(
                "holdings",
                ENDPOINT,
                json!({ "access_token": &self.item.access_token }),
            )
            .await?;
        let fetched_at = Utc::now().to_rfc3339();
        let invalid = || self.invalid(ENDPOINT);
        let mut data = PlaidData::default();
        for mut account in required_array(&investments, "accounts").map_err(|_| invalid())? {
            self.describe_account(&mut account, website, &fetched_at, None);
            data.investment_accounts.push(account);
        }
        data.holdings = required_array(&investments, "holdings").map_err(|_| invalid())?;
        data.securities = required_array(&investments, "securities").map_err(|_| invalid())?;
        if self.options.investment_transactions {
            match self
                .providers
                .plaid_investment_transactions(self.credentials, &self.item.access_token)
                .await
            {
                Ok((transactions, securities)) => {
                    data.investment_history_accounts = account_ids(data.investment_accounts.iter());
                    data.investment_transactions = transactions;
                    data.securities.extend(securities);
                }
                Err(error) => self.warn(&mut data, "investment activity not updated", error),
            }
        }
        Ok(data)
    }

    /// Brings the Item's transaction cache up to Plaid's cursor and records how far its history
    /// reaches. The cache changes only when the whole walk succeeds.
    async fn sync_transactions(&self, cache: &mut PlaidItemCache) -> Result<(), ProviderError> {
        self.report("transactions", "Checking".into(), "active");
        let started = Instant::now();
        let result = self.page_transactions(cache).await;
        let synced = self.finish("transactions", started, result, |synced| {
            let requests = match synced.requests {
                1 => "1 request".to_string(),
                requests => format!("{requests} requests"),
            };
            let retries = match synced.retries {
                0 => String::new(),
                1 => " · 1 retry".into(),
                retries => format!(" · {retries} retries"),
            };
            format!("Complete · {requests}{retries}")
        })?;
        cache.cursor = Some(synced.cursor);
        cache.transactions = synced.transactions.into_values().collect();
        cache.taxonomy = Some(PLAID_TRANSACTION_FORMAT.into());
        cache.history_complete =
            synced.update_status.as_deref() == Some("HISTORICAL_UPDATE_COMPLETE");
        cache.transaction_history_start = plaid_history_start(
            cache.history_complete,
            cache.transaction_history_start.take(),
            Utc::now().date_naive(),
        );
        Ok(())
    }

    /// Walks the cursor from the cache, starting over (a bounded number of times) when Plaid
    /// reports the ledger changed under the walk. Requests and retries count across attempts.
    async fn page_transactions(
        &self,
        cache: &PlaidItemCache,
    ) -> Result<SyncedTransactions, ProviderError> {
        // Mixing taxonomies would split categories, and a cache fetched without a field would
        // lack it on older rows, so a cache in another format is rebuilt from a fresh cursor
        // instead of being extended.
        let current_format = cache.taxonomy.as_deref() == Some(PLAID_TRANSACTION_FORMAT);
        let saved_cursor = cache.cursor.clone().filter(|_| current_format);
        let saved: BTreeMap<String, Value> = cache
            .transactions
            .iter()
            .filter(|_| current_format)
            .filter_map(|transaction| Some((transaction_id(transaction)?, transaction.clone())))
            .collect();
        let (mut requests, mut retries) = (0, 0);
        for attempt in 1..=SYNC_ATTEMPTS {
            match self
                .walk_cursor(
                    saved_cursor.clone(),
                    saved.clone(),
                    &mut requests,
                    &mut retries,
                )
                .await
            {
                Err(error)
                    if attempt < SYNC_ATTEMPTS
                        && error.code() == Some(MUTATED_DURING_PAGINATION) =>
                {
                    continue;
                }
                result => return result,
            }
        }
        unreachable!("the last attempt returns")
    }

    /// One pass over `/transactions/sync` from `cursor`, applying added, modified, and removed
    /// rows to `transactions` until Plaid has no more.
    async fn walk_cursor(
        &self,
        mut cursor: Option<String>,
        mut transactions: BTreeMap<String, Value>,
        requests: &mut usize,
        retries: &mut usize,
    ) -> Result<SyncedTransactions, ProviderError> {
        const ENDPOINT: &str = "/transactions/sync";
        let invalid = || self.invalid(ENDPOINT);
        let mut seen: BTreeSet<String> = cursor.iter().cloned().collect();
        for page in 0..SYNC_PAGES {
            let mut request = json!({
                "access_token": &self.item.access_token,
                "count": 500,
                "options": {
                    "personal_finance_category_version": PLAID_CATEGORY_TAXONOMY,
                    "include_original_description": true
                }
            });
            if let Some(value) = cursor.as_deref() {
                request["cursor"] = Value::String(value.into());
            }
            *requests += 1;
            let (sync, request_retries) = self.read(ENDPOINT, request).await?;
            *retries += request_retries;
            let update_status = sync
                .get("transactions_update_status")
                .and_then(Value::as_str)
                .map(str::to_owned);
            for transaction in
                required_arrays(&sync, &["added", "modified"]).map_err(|_| invalid())?
            {
                if let Some(id) = transaction_id(&transaction) {
                    transactions.insert(id, transaction);
                }
            }
            for removed in required_array(&sync, "removed").map_err(|_| invalid())? {
                if let Some(id) = transaction_id(&removed) {
                    transactions.remove(&id);
                }
            }
            let next_cursor = string_field(&sync, "next_cursor").map_err(|_| invalid())?;
            let has_more = bool_field(&sync, "has_more").map_err(|_| invalid())?;
            validate_plaid_page(&next_cursor, has_more, page, &mut seen).map_err(|_| invalid())?;
            if !has_more {
                return Ok(SyncedTransactions {
                    cursor: next_cursor,
                    transactions,
                    update_status,
                    requests: *requests,
                    retries: *retries,
                });
            }
            cursor = Some(next_cursor);
        }
        Err(invalid())
    }

    /// Cached balances come free with Transactions and update when Plaid syncs the bank; a
    /// real-time Balance request is billed per call and only made when asked for. Returns the
    /// response with the time it was fetched.
    async fn fetch_balances(&self) -> Result<(Value, String), ProviderError> {
        let mut request = json!({ "access_token": self.item.access_token });
        if self.options.live_balances {
            request["options"] = json!({
                "min_last_updated_datetime": (Utc::now() - Duration::minutes(15)).to_rfc3339()
            });
        }
        let balances = self
            .measured("balances", self.balance_endpoint(), request)
            .await?;
        Ok((balances, Utc::now().to_rfc3339()))
    }

    fn balance_endpoint(&self) -> &'static str {
        if self.options.live_balances {
            "/accounts/balance/get"
        } else {
            "/accounts/get"
        }
    }

    /// The cash and card accounts in a balances response, described for the projection.
    fn cash_accounts(
        &self,
        accounts: Vec<Value>,
        website: Option<&str>,
        fetched_at: &str,
    ) -> CashAccounts {
        let balance_mode = if self.options.live_balances {
            "live"
        } else {
            "cached"
        };
        let mut cash = CashAccounts::default();
        for mut account in accounts {
            if !allowed_plaid_account(&account) {
                continue;
            }
            self.describe_account(&mut account, website, fetched_at, Some(balance_mode));
            if let Some(id) = account_id(&account) {
                if account.get("type").and_then(Value::as_str) == Some("credit") {
                    cash.credit_ids.push(id.clone());
                }
                cash.ids.push(id);
            }
            cash.accounts.push(account);
        }
        cash
    }

    /// Recurring Transactions, a billed product: a failure warns and the rest still commits.
    async fn sync_recurring(&self, data: &mut PlaidData) {
        match self
            .read(
                "/transactions/recurring/get",
                json!({ "access_token": &self.item.access_token }),
            )
            .await
        {
            Ok((response, _)) => {
                for (key, direction) in [("inflow_streams", "in"), ("outflow_streams", "out")] {
                    for mut stream in array(&response, key) {
                        if let Some(object) = stream.as_object_mut() {
                            object.insert("direction".into(), direction.into());
                        }
                        data.recurring_streams.push(stream);
                    }
                }
            }
            Err(error) => self.warn(data, "recurring streams not updated", error),
        }
    }

    /// Liabilities, a billed product: card statement details are optional, so balances and
    /// transactions still commit when it fails.
    async fn sync_liabilities(&self, data: &mut PlaidData, credit_ids: &[String]) {
        match self
            .read(
                "/liabilities/get",
                json!({
                    "access_token": &self.item.access_token,
                    "options": { "account_ids": credit_ids }
                }),
            )
            .await
        {
            Ok((response, _)) => data.liabilities.extend(
                response
                    .pointer("/liabilities/credit")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            ),
            Err(error) => {
                let what = match error.code() {
                    Some("ADDITIONAL_CONSENT_REQUIRED") => {
                        "repair in Settings to get statement dates"
                    }
                    Some("PRODUCTS_NOT_SUPPORTED" | "NO_LIABILITY_ACCOUNTS") => {
                        "statement dates estimated"
                    }
                    _ => "statement details not updated",
                };
                self.warn(data, what, error);
            }
        }
    }

    /// Names the institution on an account and stamps when its balance was fetched and how.
    fn describe_account(
        &self,
        account: &mut Value,
        website: Option<&str>,
        fetched_at: &str,
        balance_mode: Option<&str>,
    ) {
        let Some(object) = account.as_object_mut() else {
            return;
        };
        object.insert(
            "institution_name".into(),
            self.institution("Unknown institution").into(),
        );
        if let Some(website) = website {
            object.insert("institution_url".into(), website.into());
        }
        object.insert("balance_fetched_at".into(), fetched_at.into());
        if let Some(mode) = balance_mode {
            object.insert("balance_mode".into(), mode.into());
        }
    }

    fn institution(&self, fallback: &'static str) -> &str {
        self.item.institution_name.as_deref().unwrap_or(fallback)
    }

    fn invalid(&self, endpoint: &str) -> ProviderError {
        ProviderError::invalid_response("Plaid", endpoint, 1)
    }

    /// Records a product that failed on its own, by institution, without failing the Item.
    fn warn(&self, data: &mut PlaidData, what: &str, error: ProviderError) {
        data.warnings
            .push(format!("{}: {what}", self.institution("A connection")));
        data.diagnostics.push(error.diagnostic());
    }

    async fn read(&self, path: &str, body: Value) -> Result<(Value, usize), ProviderError> {
        self.providers
            .plaid_read(self.credentials, path, body)
            .await
    }

    /// One request reported under `task` from start to finish.
    async fn measured(
        &self,
        task: &'static str,
        path: &str,
        body: Value,
    ) -> Result<Value, ProviderError> {
        self.report(task, "Checking".into(), "active");
        let started = Instant::now();
        let result = self.read(path, body).await;
        self.finish(task, started, result, |(_, retries)| {
            completion_status(*retries)
        })
        .map(|(value, _)| value)
    }

    /// Reports how `task` ended, with `detail` describing a success, and hands the result on.
    fn finish<T, E>(
        &self,
        task: &'static str,
        started: Instant,
        result: Result<T, E>,
        detail: impl FnOnce(&T) -> String,
    ) -> Result<T, E> {
        let status = match &result {
            Ok(value) => measured_status(started, detail(value)),
            Err(_) => measured_status(started, "Failed"),
        };
        self.report(
            task,
            status,
            if result.is_ok() {
                "complete"
            } else {
                "warning"
            },
        );
        result
    }

    fn report(&self, task: &'static str, status: String, state: &'static str) {
        report_plaid_progress(self.progress, self.index, self.count, task, status, state);
    }
}

fn account_id(value: &Value) -> Option<String> {
    value
        .get("account_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn account_ids<'a>(values: impl Iterator<Item = &'a Value>) -> Vec<String> {
    values.filter_map(account_id).collect()
}

fn transaction_id(transaction: &Value) -> Option<String> {
    transaction
        .get("transaction_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub(super) fn report_plaid_progress(
    progress: &(dyn Fn(PlaidRefreshProgress) + Sync),
    item_index: usize,
    item_count: usize,
    task: &'static str,
    status: String,
    state: &'static str,
) {
    let connection = if item_count == 1 {
        "Plaid".into()
    } else {
        format!("Plaid {}", item_index + 1)
    };
    progress(PlaidRefreshProgress {
        id: format!("plaid-{item_index}-{}", task.to_lowercase()),
        label: format!("{connection} {task}"),
        status,
        state,
    });
}

pub(super) fn measured_status(started: Instant, detail: impl AsRef<str>) -> String {
    format!(
        "{} · {:.1}s",
        detail.as_ref(),
        started.elapsed().as_secs_f64()
    )
}

pub(super) fn completion_status(retries: usize) -> String {
    match retries {
        0 => "Complete".into(),
        1 => "Complete · 1 retry".into(),
        retries => format!("Complete · {retries} retries"),
    }
}

pub(super) fn validate_plaid_page(
    cursor: &str,
    has_more: bool,
    page: usize,
    seen: &mut BTreeSet<String>,
) -> Result<(), String> {
    if has_more && (page >= SYNC_PAGES - 1 || !seen.insert(cursor.into())) {
        return Err(
            "Plaid transaction pagination made no progress or exceeded its safe page limit".into(),
        );
    }
    Ok(())
}

/// Plaid sends about 30 days first and the rest of the requested window later. Until it
/// reports the historical pull complete, balance history starts at the earliest transaction
/// it has sent rather than claiming the whole window.
pub(super) fn plaid_history_start(
    complete: bool,
    saved: Option<String>,
    today: NaiveDate,
) -> Option<String> {
    if !complete {
        return None;
    }
    saved.or_else(|| Some((today - Duration::days(729)).to_string()))
}

/// Overlays `fresh` on `target` by account: every row of an account `fresh` carries is
/// replaced, rows of other accounts stay, and securities dedupe by id.
pub(super) fn merge_plaid_data(target: &mut PlaidData, mut fresh: PlaidData) {
    target.diagnostics.append(&mut fresh.diagnostics);
    let ids = account_ids(fresh.accounts.iter().chain(&fresh.investment_accounts))
        .into_iter()
        .collect::<BTreeSet<_>>();
    let retained = |value: &Value| !account_id(value).is_some_and(|id| ids.contains(&id));
    target.accounts.retain(retained);
    target.investment_accounts.retain(retained);
    target.transactions.retain(retained);
    target.holdings.retain(retained);
    target.liabilities.retain(retained);
    target.investment_transactions.retain(retained);
    target.recurring_streams.retain(retained);
    target
        .investment_history_accounts
        .retain(|id| !ids.contains(id));
    target
        .transaction_history_start
        .retain(|id, _| !ids.contains(id));
    target
        .transaction_history_start
        .extend(fresh.transaction_history_start);
    target.accounts.extend(fresh.accounts);
    target.investment_accounts.extend(fresh.investment_accounts);
    target.transactions.extend(fresh.transactions);
    target.holdings.extend(fresh.holdings);
    target.liabilities.extend(fresh.liabilities);
    target
        .investment_transactions
        .extend(fresh.investment_transactions);
    target
        .investment_history_accounts
        .extend(fresh.investment_history_accounts);
    target.recurring_streams.extend(fresh.recurring_streams);
    let securities: BTreeMap<String, Value> = target
        .securities
        .drain(..)
        .chain(fresh.securities)
        .filter_map(|security| Some((security.get("security_id")?.as_str()?.to_owned(), security)))
        .collect();
    target.securities = securities.into_values().collect();
}

/// Transactions Items carry cash and card accounts only: the projection values
/// every other account as cash, so a loan would count as an asset.
pub(super) fn allowed_plaid_account(account: &Value) -> bool {
    matches!(
        account.get("type").and_then(Value::as_str),
        Some("depository" | "credit")
    )
}

/// Whether an `/institutions/get_by_id` response lists `product` among the institution's.
pub(super) fn institution_supports(response: &Value, product: &str) -> bool {
    response
        .pointer("/institution/products")
        .and_then(Value::as_array)
        .is_some_and(|products| products.iter().any(|value| value.as_str() == Some(product)))
}

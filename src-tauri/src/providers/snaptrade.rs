use super::*;

impl Providers {
    pub async fn sync_snaptrade(
        &self,
        cached: Option<&SnapTradeData>,
        refresh_history: bool,
        backfill_activity: bool,
    ) -> Result<SnapTradeData, String> {
        let Some(credentials) =
            self.read_secret::<SnapTradeCredentials>(SNAPTRADE_CREDENTIALS_KEY)?
        else {
            return Ok(SnapTradeData::default());
        };
        let (all_accounts, connections) = tokio::join!(
            self.snaptrade_get(&credentials, "/accounts"),
            self.snaptrade_get(&credentials, "/authorizations"),
        );
        let all_accounts = all_accounts?
            .as_array()
            .cloned()
            .ok_or_else(|| "SnapTrade response did not include an account list".to_string())?;
        let refresh = AccountRefresh {
            credentials: &credentials,
            cached,
            refresh_history,
            backfill_activity,
            balance_fetched_at: Utc::now().to_rfc3339(),
        };
        let results = stream::iter(
            all_accounts
                .into_iter()
                .filter(supported_snaptrade_account)
                .map(|account| self.sync_snaptrade_account(account, &refresh)),
        )
        .buffered(3)
        .try_collect::<Vec<_>>()
        .await?;
        let mut data = SnapTradeData::complete();
        for account in results {
            data.absorb(account);
        }
        let websites = connections
            .as_ref()
            .map(snaptrade_websites)
            .unwrap_or_default();
        for account in &mut data.accounts {
            let authorization = account
                .get("brokerage_authorization")
                .and_then(|value| value.as_str().or_else(|| value.get("id")?.as_str()))
                .map(str::to_owned);
            if let Some(website) = authorization.and_then(|id| websites.get(&id)) {
                account["institution_url"] = Value::String(website.clone());
            }
        }
        data.connections = match connections.and_then(|value| snaptrade_connections(&value)) {
            Ok(connections) => connections,
            Err(error) => {
                data.warnings
                    .push(format!("SnapTrade connection status: {error}"));
                cached
                    .map(|data| data.connections.clone())
                    .unwrap_or_default()
            }
        };
        for connection in data.connections.iter().filter(|c| c.disabled) {
            data.warnings.push(format!(
                "SnapTrade {}: needs repair in Settings; showing saved data",
                connection.name
            ));
        }
        Ok(data)
    }

    /// One account's positions, cash, balance history, and activity, fetched together. Cash and
    /// history are bounded so a slow answer can't hold the refresh; any part that fails keeps
    /// its saved copy with a warning, except positions, which fail the account.
    async fn sync_snaptrade_account(
        &self,
        mut account: Value,
        refresh: &AccountRefresh<'_>,
    ) -> Result<SnapTradeData, String> {
        let AccountRefresh {
            credentials,
            cached,
            refresh_history,
            backfill_activity,
            ..
        } = *refresh;
        let mut data = SnapTradeData::complete();
        let id = account
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        // `/accounts` only returns brokerage accounts. `account_category` is optional and is
        // commonly null for valid accounts, including E*Trade Individual and Roth IRA
        // accounts, so it cannot be used as an inclusion filter.
        let label = account
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .map_or_else(
                || "SnapTrade account".to_owned(),
                |name| format!("SnapTrade {name}"),
            );
        account["balance_fetched_at"] = Value::String(refresh.balance_fetched_at.clone());
        let positions_path = format!("/accounts/{id}/positions/all");
        let cash_path = format!("/accounts/{id}/balances");
        let history_path = format!("/accounts/{id}/balanceHistory");
        let activities_path = format!("/accounts/{id}/activities");
        let cached_history = cached.and_then(|data| data.balance_history.get(&id));
        let cached_activities = cached.and_then(|data| data.activities.get(&id));
        let recent_only = !backfill_activity && cached_activities.is_some();
        let since = (Utc::now() - Duration::days(if recent_only { 30 } else { 730 }))
            .format("%Y-%m-%d")
            .to_string();
        let (positions, cash_balances, balance_history, activities) = tokio::join!(
            self.snaptrade_get(credentials, &positions_path),
            self.snaptrade_get_within(credentials, &cash_path, EXTRA_TIMEOUT),
            async {
                if !refresh_history {
                    if let Some(history) = cached_history {
                        return Ok(history.clone());
                    }
                }
                let response = self
                    .snaptrade_get_within(credentials, &history_path, EXTRA_TIMEOUT)
                    .await?;
                required_array(&response, "history")
            },
            async {
                self.snaptrade_activities(credentials, &activities_path, &since)
                    .await
                    .map(|fresh| {
                        if recent_only {
                            merge_recent_activities(
                                cached_activities.map(Vec::as_slice).unwrap_or(&[]),
                                fresh,
                                &since,
                            )
                        } else {
                            fresh
                        }
                    })
            },
        );
        let positions = positions?;
        data.positions_as_of.insert(
            id.clone(),
            positions
                .pointer("/data_freshness/as_of")
                .and_then(Value::as_str)
                .map(str::to_owned),
        );
        data.positions
            .insert(id.clone(), required_array(&positions, "results")?);
        match cash_balances.and_then(|value| {
            value
                .as_array()
                .cloned()
                .ok_or_else(|| "Cash balance response is not a list".into())
        }) {
            Ok(balances) => {
                data.cash_balances.insert(id.clone(), balances);
            }
            Err(_) => data
                .warnings
                .push(format!("{label}: cash unavailable; using reported total")),
        }
        data.history_complete &= balance_history.is_ok();
        data.activity_complete &= activities.is_ok();
        let balance_history = history_or_cached(
            balance_history,
            cached_history,
            &format!("{label} balance history"),
            &mut data.warnings,
        );
        data.balance_history.insert(id.clone(), balance_history);
        if activities.is_err() {
            account["sync_status"]["transactions"] = cached
                .and_then(|data| {
                    data.accounts
                        .iter()
                        .find(|account| account.get("id").and_then(Value::as_str) == Some(&id))
                })
                .and_then(|account| account.pointer("/sync_status/transactions"))
                .cloned()
                .unwrap_or(Value::Null);
        }
        data.accounts.push(account);
        let activities = history_or_cached(
            activities,
            cached_activities,
            &format!("{label} activities"),
            &mut data.warnings,
        );
        data.activities.insert(id, activities);
        Ok(data)
    }

    /// Asks SnapTrade directly, so a connection can be repaired even while refreshes fail.
    pub async fn snaptrade_connections_live(&self) -> Result<Vec<ProviderConnection>, String> {
        let Some(credentials) =
            self.read_secret::<SnapTradeCredentials>(SNAPTRADE_CREDENTIALS_KEY)?
        else {
            return Ok(Vec::new());
        };
        let response = tokio::time::timeout(
            StdDuration::from_secs(10),
            self.snaptrade_get(&credentials, "/authorizations"),
        )
        .await
        .map_err(|_| "SnapTrade did not respond in time (connections)".to_string())??;
        Ok(snaptrade_connections(&response)?
            .iter()
            .map(ProviderConnection::from)
            .collect())
    }

    pub(super) async fn snaptrade_activities(
        &self,
        credentials: &SnapTradeCredentials,
        path: &str,
        since: &str,
    ) -> Result<Vec<Value>, String> {
        const PAGE_SIZE: usize = 1000;
        const MAX_PAGES: usize = 100;

        let mut activities = Vec::new();
        let mut seen = BTreeSet::new();
        let mut offset = 0;
        let end = Utc::now().format("%Y-%m-%d").to_string();
        for _ in 0..MAX_PAGES {
            let response = self
                .snaptrade_request(
                    credentials,
                    Method::GET,
                    path,
                    vec![
                        ("startDate".into(), since.into()),
                        ("endDate".into(), end.clone()),
                        ("limit".into(), PAGE_SIZE.to_string()),
                        ("offset".into(), offset.to_string()),
                    ],
                    None,
                )
                .await?;
            let page = response
                .get("data")
                .and_then(Value::as_array)
                .cloned()
                .ok_or_else(|| "SnapTrade response did not include activity data".to_string())?;
            let page_len = page.len();
            offset += page_len;
            let before = activities.len();
            for activity in page {
                let key = activity
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| activity.to_string());
                if seen.insert(key) {
                    activities.push(activity);
                }
            }
            let total = response
                .pointer("/pagination/total")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok());

            if page_len == 0 || total.is_some_and(|value| offset >= value) {
                return Ok(activities);
            }
            // Older API responses may omit pagination metadata. A short page is still a reliable
            // end condition, while a full page warrants one more request.
            if total.is_none() && page_len < PAGE_SIZE {
                return Ok(activities);
            }
            if activities.len() == before {
                return Err("SnapTrade activity pagination made no progress".into());
            }
        }
        Err("SnapTrade activity pagination exceeded its safe page limit".into())
    }

    pub(super) async fn snaptrade_get(
        &self,
        credentials: &SnapTradeCredentials,
        path: &str,
    ) -> Result<Value, String> {
        self.snaptrade_request(credentials, Method::GET, path, vec![], None)
            .await
    }

    /// A GET that gives up after `limit`, for parts of a refresh it can do without.
    async fn snaptrade_get_within(
        &self,
        credentials: &SnapTradeCredentials,
        path: &str,
        limit: StdDuration,
    ) -> Result<Value, String> {
        tokio::time::timeout(limit, self.snaptrade_get(credentials, path))
            .await
            .map_err(|_| "request timed out".to_string())?
    }

    pub(super) async fn snaptrade_request(
        &self,
        credentials: &SnapTradeCredentials,
        method: Method,
        path: &str,
        parameters: Vec<(String, String)>,
        body: Option<Value>,
    ) -> Result<Value, String> {
        self.snaptrade_send(credentials, method, path, parameters, body)
            .await
            .map_err(|error| format!("{error} ({})", snaptrade_step(path)))
    }

    async fn snaptrade_send(
        &self,
        credentials: &SnapTradeCredentials,
        method: Method,
        path: &str,
        parameters: Vec<(String, String)>,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let mut url = Url::parse(&format!("https://api.snaptrade.com{path}"))
            .map_err(|error| error.to_string())?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("clientId", &credentials.client_id);
            query.append_pair("timestamp", &Utc::now().timestamp().to_string());
            for (key, value) in parameters {
                query.append_pair(&key, &value);
            }
        }
        let signature_payload = json!({
            "content": body.clone().filter(|value| !value.as_object().is_some_and(|v| v.is_empty())).unwrap_or(Value::Null),
            "path": url.path(),
            "query": url.query().unwrap_or_default()
        });
        let signature_content =
            serde_json::to_string(&signature_payload).map_err(|error| error.to_string())?;
        let mut mac = Hmac::<Sha256>::new_from_slice(credentials.consumer_key.as_bytes())
            .map_err(|error| error.to_string())?;
        mac.update(signature_content.as_bytes());
        let signature = BASE64.encode(mac.finalize().into_bytes());
        let mut request = self
            .http
            .request(method.clone(), url)
            .header("Signature", signature)
            .header("Accept", "application/json");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let retry = (method == Method::GET)
            .then(|| request.try_clone())
            .flatten();
        let response = send_with_retry(request, retry)
            .await
            .map_err(|(error, _)| snaptrade_send_error(error))?;
        provider_response("SnapTrade", path, response).await
    }
}

/// Each brokerage login's website, by authorization ID, for its logo.
pub(super) fn snaptrade_websites(value: &Value) -> BTreeMap<String, String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|connection| {
            let id = connection.get("id")?.as_str()?.to_owned();
            let url = connection.pointer("/brokerage/url")?.as_str()?;
            Some((id, url.to_owned()))
        })
        .collect()
}

pub(super) fn snaptrade_connections(value: &Value) -> Result<Vec<SnapTradeConnection>, String> {
    value
        .as_array()
        .ok_or("SnapTrade response did not include a connection list")?
        .iter()
        .map(|connection| {
            let name = ["/brokerage/display_name", "/brokerage/name", "/name"]
                .into_iter()
                .filter_map(|pointer| connection.pointer(pointer).and_then(Value::as_str))
                .find(|name| !name.trim().is_empty())
                .unwrap_or("SnapTrade connection")
                .to_owned();
            Ok(SnapTradeConnection {
                id: string_field(connection, "id")?,
                name,
                disabled: connection
                    .get("disabled")
                    .and_then(Value::as_bool)
                    .ok_or("SnapTrade connection did not include its disabled status")?,
            })
        })
        .collect()
}

// Only enabled connections are in the baseline, so repairing one is observable
// even when SnapTrade preserves its connection and account IDs.
pub(super) fn snaptrade_active_connection_ids(value: &Value) -> Result<BTreeSet<String>, String> {
    Ok(snaptrade_connections(value)?
        .into_iter()
        .filter(|connection| !connection.disabled)
        .map(|connection| connection.id)
        .collect())
}

/// Names the step without the account IDs that SnapTrade paths contain.
fn snaptrade_step(path: &str) -> &'static str {
    if path.ends_with("/positions/all") {
        "positions"
    } else if path.ends_with("/balances") {
        "cash"
    } else if path.ends_with("/balanceHistory") {
        "balance history"
    } else if path.ends_with("/activities") {
        "activity"
    } else if path == "/accounts" {
        "accounts"
    } else if path == "/authorizations" {
        "connections"
    } else if path == "/snapTrade/login" {
        "login"
    } else {
        "request"
    }
}

/// A send failure names its kind only; the transport message is not shown or recorded.
fn snaptrade_send_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "SnapTrade did not respond in time".into()
    } else {
        "Could not reach SnapTrade".into()
    }
}

pub(super) fn snaptrade_link_status(
    response: &Value,
    baseline: &BTreeSet<String>,
    browser_completed: bool,
) -> Result<LinkStatus, String> {
    let active = snaptrade_active_connection_ids(response)?;
    if browser_completed && active.is_empty() {
        return Err(
            "No active SnapTrade connection found. Finish connecting in your browser and try again"
                .into(),
        );
    }
    Ok(LinkStatus::of(
        browser_completed || !active.is_subset(baseline),
    ))
}

pub(super) fn merge_recent_activities(
    cached: &[Value],
    fresh: Vec<Value>,
    since: &str,
) -> Vec<Value> {
    let fresh_ids: BTreeSet<&str> = fresh
        .iter()
        .filter_map(|activity| activity.get("id").and_then(Value::as_str))
        .collect();
    let mut result: Vec<Value> = cached
        .iter()
        .filter(|activity| {
            let date = activity
                .get("trade_date")
                .and_then(Value::as_str)
                .or_else(|| activity.get("settlement_date").and_then(Value::as_str))
                .unwrap_or("");
            !date.is_empty()
                && date < since
                && !activity
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| fresh_ids.contains(id))
        })
        .cloned()
        .collect();
    result.extend(fresh);
    result
}

pub(super) fn supported_snaptrade_account(account: &Value) -> bool {
    account
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty())
}

/// How long cash and balance history may take before an account's refresh goes on without them.
const EXTRA_TIMEOUT: StdDuration = StdDuration::from_secs(5);

/// What each account's refresh shares with the others.
struct AccountRefresh<'a> {
    credentials: &'a SnapTradeCredentials,
    cached: Option<&'a SnapTradeData>,
    refresh_history: bool,
    backfill_activity: bool,
    balance_fetched_at: String,
}

impl SnapTradeData {
    /// Nothing yet, and nothing missing.
    fn complete() -> Self {
        Self {
            history_complete: true,
            activity_complete: true,
            ..Default::default()
        }
    }

    /// Adds another account's data; the result is complete only if both were.
    fn absorb(&mut self, other: Self) {
        self.accounts.extend(other.accounts);
        self.positions.extend(other.positions);
        self.cash_balances.extend(other.cash_balances);
        self.positions_as_of.extend(other.positions_as_of);
        self.activities.extend(other.activities);
        self.balance_history.extend(other.balance_history);
        self.warnings.extend(other.warnings);
        self.history_complete &= other.history_complete;
        self.activity_complete &= other.activity_complete;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaptrade_link_detects_repairs_and_verifies_explicit_browser_completion() {
        let before = json!([
            {"id": "existing", "disabled": false},
            {"id": "repair", "disabled": true}
        ]);
        let baseline = snaptrade_active_connection_ids(&before).unwrap();
        assert_eq!(
            snaptrade_link_status(&before, &baseline, false)
                .unwrap()
                .status,
            "pending"
        );
        let repaired = json!([
            {"id": "existing", "disabled": false},
            {"id": "repair", "disabled": false}
        ]);
        assert_eq!(
            snaptrade_link_status(&repaired, &baseline, false)
                .unwrap()
                .status,
            "connected"
        );
        let added = json!([{"id": "new", "disabled": false}]);
        assert_eq!(
            snaptrade_link_status(&added, &baseline, false)
                .unwrap()
                .status,
            "connected"
        );
        // Reopening a healthy connection may leave every provider field unchanged.
        assert_eq!(
            snaptrade_link_status(&before, &baseline, true)
                .unwrap()
                .status,
            "connected"
        );
        for unavailable in [json!([]), json!([{"id": "repair", "disabled": true}])] {
            assert_eq!(
                snaptrade_link_status(&unavailable, &baseline, false)
                    .unwrap()
                    .status,
                "pending"
            );
            assert!(snaptrade_link_status(&unavailable, &baseline, true).is_err());
        }
        for malformed in [
            json!({}),
            json!([{"id": "existing"}]),
            json!([{"disabled": false}]),
        ] {
            assert!(snaptrade_link_status(&malformed, &baseline, true).is_err());
        }
    }

    #[test]
    fn snaptrade_websites_map_each_login_to_its_brokerage_site() {
        let websites = snaptrade_websites(&json!([
            { "id": "a", "brokerage": { "url": "https://us.etrade.com" } },
            { "id": "b", "brokerage": { "url": null } }
        ]));
        assert_eq!(
            websites.get("a").map(String::as_str),
            Some("https://us.etrade.com")
        );
        assert!(!websites.contains_key("b"));
    }

    #[test]
    fn snaptrade_connections_name_the_brokerage_and_keep_disabled_status() {
        let connections = snaptrade_connections(&json!([
            {"id": "one", "disabled": true, "name": "Connection-1",
             "brokerage": {"display_name": "Broker One", "name": "Broker One Inc."}},
            {"id": "two", "disabled": false, "name": "Connection-2", "brokerage": {"name": "Broker Two"}},
            {"id": "three", "disabled": false, "brokerage": {"display_name": " "}}
        ]))
        .unwrap();
        assert_eq!(
            connections,
            vec![
                SnapTradeConnection {
                    id: "one".into(),
                    name: "Broker One".into(),
                    disabled: true
                },
                SnapTradeConnection {
                    id: "two".into(),
                    name: "Broker Two".into(),
                    disabled: false
                },
                SnapTradeConnection {
                    id: "three".into(),
                    name: "SnapTrade connection".into(),
                    disabled: false
                },
            ]
        );
        assert!(snaptrade_connections(&json!([{"id": "one", "name": "No status"}])).is_err());
    }

    #[test]
    fn snaptrade_errors_name_the_step_without_account_ids() {
        for (path, step) in [
            ("/accounts", "accounts"),
            ("/authorizations", "connections"),
            ("/accounts/account-id/positions/all", "positions"),
            ("/accounts/account-id/balances", "cash"),
            ("/accounts/account-id/balanceHistory", "balance history"),
            ("/accounts/account-id/activities", "activity"),
        ] {
            assert_eq!(snaptrade_step(path), step);
        }
    }

    #[test]
    fn recent_activity_refresh_replaces_recent_records_but_retains_backfill() {
        let cached = vec![
            json!({"id": "old", "trade_date": "2026-01-01"}),
            json!({"id": "deleted", "trade_date": "2026-09-01"}),
            json!({"id": "moved", "trade_date": "2026-01-01"}),
        ];
        let result = merge_recent_activities(
            &cached,
            vec![
                json!({"id": "new", "trade_date": "2026-09-02"}),
                json!({"id": "moved", "trade_date": "2026-09-03"}),
            ],
            "2026-08-01",
        );
        assert_eq!(
            result
                .iter()
                .map(|value| value["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["old", "new", "moved"]
        );
    }

    #[test]
    fn snaptrade_accepts_accounts_without_an_account_category() {
        let account = json!({
            "id": "account-id",
            "name": "E*Trade Roth IRA",
            "raw_type": "ROTHIRA",
            "account_category": null
        });

        assert!(supported_snaptrade_account(&account));
    }
}

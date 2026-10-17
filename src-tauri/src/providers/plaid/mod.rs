use super::snaptrade::{
    snaptrade_active_connection_ids, snaptrade_connections, snaptrade_link_status,
};
use super::vault::{update_plaid_items, upsert_plaid_item};
use super::*;

impl Providers {
    /// Reads local state only; `snaptrade_connections_live` asks SnapTrade separately.
    pub fn connections(
        &self,
        cache: &PlaidCache,
        snaptrade: &[SnapTradeConnection],
    ) -> Result<Vec<ProviderConnection>, String> {
        let mut connections = self
            .read_secret::<Vec<PlaidItem>>(PLAID_ITEMS_KEY)?
            .unwrap_or_default()
            .into_iter()
            .map(|item| ProviderConnection {
                error: cache
                    .items
                    .get(&item.item_id)
                    .and_then(|cached| cached.error.clone()),
                statements: statements_state(&item, cache.items.get(&item.item_id)),
                item_id: item.item_id,
                name: item
                    .institution_name
                    .unwrap_or_else(|| "Plaid connection".into()),
                provider: if item.investments {
                    super::ConnectionProvider::PlaidInvestments
                } else {
                    super::ConnectionProvider::Plaid
                },
            })
            .collect::<Vec<_>>();
        connections.extend(snaptrade.iter().map(ProviderConnection::from));
        Ok(connections)
    }

    pub fn forget_connection(&self, item_id: &str) -> Result<(), String> {
        self.mutate_vault(None, true, |vault| {
            update_plaid_items(vault, |items| items.retain(|item| item.item_id != item_id))
        })
    }

    pub async fn begin_link(
        &self,
        provider: &str,
        item_id: Option<&str>,
    ) -> Result<LinkSession, String> {
        let generation = self.credential_generation.load(Ordering::SeqCst);
        let session_id = Uuid::new_v4().to_string();
        let (url, pending) = match provider {
            "plaid" => self.begin_plaid_link(generation, false, item_id).await?,
            "plaid-investments" => self.begin_plaid_link(generation, true, item_id).await?,
            "plaid-statements" => {
                let item_id = item_id.ok_or("Choose a Plaid connection for statements")?;
                self.begin_plaid_statements_link(generation, item_id)
                    .await?
            }
            "snaptrade" => self.begin_snaptrade_link(item_id).await?,
            _ => return Err("Unknown provider".into()),
        };
        let mut links = self
            .links
            .lock()
            .map_err(|_| "Provider link state is unavailable".to_string())?;
        if generation != self.credential_generation.load(Ordering::SeqCst) {
            return Err("Credentials changed; start a new connection attempt".into());
        }
        links.retain(|_, session| session.expires > Instant::now());
        // One browser flow at a time makes the before/after connection baseline unambiguous.
        for session in links.values() {
            session.cancel.send_replace(true);
        }
        links.clear();
        links.insert(
            session_id.clone(),
            PendingSession {
                link: pending,
                generation,
                expires: Instant::now() + StdDuration::from_secs(300),
                polling: false,
                cancel: tokio::sync::watch::channel(false).0,
            },
        );
        Ok(LinkSession {
            provider: provider.into(),
            session_id,
            url,
        })
    }

    /// A hosted Plaid Link for a new login, or for repairing the saved item `item_id`.
    async fn begin_plaid_link(
        &self,
        generation: u64,
        investments: bool,
        item_id: Option<&str>,
    ) -> Result<(String, PendingLink), String> {
        let credentials = self.plaid_credentials()?;
        let user = self.ensure_plaid_user(&credentials, generation).await?;
        let (user_field, user_identifier) = plaid_user_identifier(&user)?;
        // Repair reopens the saved item; its products and accounts are already chosen.
        let repair_token = match item_id {
            Some(id) => Some(
                self.read_secret::<Vec<PlaidItem>>(PLAID_ITEMS_KEY)?
                    .unwrap_or_default()
                    .into_iter()
                    .find(|item| item.item_id == id && item.investments == investments)
                    .ok_or("Unknown Plaid connection")?
                    .access_token,
            ),
            None => None,
        };
        let mut request = plaid_link_request(investments, repair_token);
        request[user_field] = Value::String(user_identifier.into());
        let response = self
            .plaid_once(&credentials, "/link/token/create", request)
            .await
            .map_err(|error| error.to_string())?;
        let link_token = string_field(&response, "link_token")?;
        let url = string_field(&response, "hosted_link_url")?;
        Ok((
            url,
            PendingLink::Plaid {
                link_token,
                exchanged_public_tokens: Vec::new(),
                investments,
                repairing: item_id.is_some(),
                statements_item: None,
            },
        ))
    }

    /// A hosted Plaid Link that adds statements to the saved bank item `item_id`: two years of
    /// them, through update mode, so the item and its accounts stay as they are.
    async fn begin_plaid_statements_link(
        &self,
        generation: u64,
        item_id: &str,
    ) -> Result<(String, PendingLink), String> {
        let credentials = self.plaid_credentials()?;
        let user = self.ensure_plaid_user(&credentials, generation).await?;
        let (user_field, user_identifier) = plaid_user_identifier(&user)?;
        let item = self
            .read_secret::<Vec<PlaidItem>>(PLAID_ITEMS_KEY)?
            .unwrap_or_default()
            .into_iter()
            .find(|item| item.item_id == item_id && !item.investments)
            .ok_or("Unknown Plaid bank connection")?;
        let today = Utc::now().date_naive();
        let mut request = plaid_statements_link_request(item.access_token, today);
        request[user_field] = Value::String(user_identifier.into());
        let response = self
            .plaid_once(&credentials, "/link/token/create", request)
            .await
            .map_err(|error| error.to_string())?;
        Ok((
            string_field(&response, "hosted_link_url")?,
            PendingLink::Plaid {
                link_token: string_field(&response, "link_token")?,
                exchanged_public_tokens: Vec::new(),
                investments: false,
                repairing: true,
                statements_item: Some(item.item_id),
            },
        ))
    }

    /// A SnapTrade connection portal for a new login, or for reconnecting `item_id`, which
    /// keeps the connection and account IDs a new login would not.
    async fn begin_snaptrade_link(
        &self,
        item_id: Option<&str>,
    ) -> Result<(String, PendingLink), String> {
        let credentials = self.snaptrade_credentials()?;
        let connections = self.snaptrade_get(&credentials, "/authorizations").await?;
        let existing_connection_ids = snaptrade_active_connection_ids(&connections)?;
        let mut request = json!({
            "connectionType": "read",
            "showCloseButton": true
        });
        if let Some(id) = item_id {
            if !snaptrade_connections(&connections)?
                .iter()
                .any(|connection| connection.id == id)
            {
                return Err("Unknown SnapTrade connection".into());
            }
            request["reconnect"] = Value::String(id.into());
        }
        let response = self
            .snaptrade_request(
                &credentials,
                Method::POST,
                "/snapTrade/login",
                vec![],
                Some(request),
            )
            .await?;
        Ok((
            string_field(&response, "redirectURI")?,
            PendingLink::SnapTrade {
                existing_connection_ids,
            },
        ))
    }

    pub async fn poll_link(
        &self,
        provider: &str,
        session_id: &str,
        browser_completed: bool,
    ) -> Result<LinkStatus, String> {
        let session = {
            let mut links = self
                .links
                .lock()
                .map_err(|_| "Provider link state is unavailable")?;
            let session = links
                .get_mut(session_id)
                .ok_or("Unknown or expired provider link session")?;
            validate_link_session(session, self.credential_generation.load(Ordering::SeqCst))?;
            if session.polling {
                return Err("This connection is already being checked".into());
            }
            session.polling = true;
            session.clone()
        };
        let result = cancellable_link_poll(
            session.cancel.subscribe(),
            session.expires,
            self.poll_link_inner(provider, session_id, session.link, browser_completed),
        )
        .await;
        let mut links = self
            .links
            .lock()
            .map_err(|_| "Provider link state is unavailable")?;
        let current = links
            .get_mut(session_id)
            .ok_or("Connection attempt cancelled")?;
        validate_link_session(current, self.credential_generation.load(Ordering::SeqCst))?;
        current.polling = false;
        if result
            .as_ref()
            .is_ok_and(|status| status.status == "connected")
        {
            links.remove(session_id);
        }
        result
    }

    pub(super) fn mutate_link_vault(
        &self,
        session_id: &str,
        change: impl FnOnce(&mut CredentialVault) -> Result<(), String>,
    ) -> Result<(), String> {
        let links = self
            .links
            .lock()
            .map_err(|_| "Provider link state is unavailable")?;
        let session = links
            .get(session_id)
            .ok_or("Connection attempt cancelled")?;
        validate_link_session(session, self.credential_generation.load(Ordering::SeqCst))?;
        self.mutate_vault(Some(session.generation), false, |vault| {
            validate_link_session(session, self.credential_generation.load(Ordering::SeqCst))?;
            change(vault)
        })
    }

    /// Trades a one-time public token for the item's access token, saving it at once, then
    /// names the item's institution when Plaid can say.
    async fn exchange_public_token(
        &self,
        credentials: &PlaidCredentials,
        session_id: &str,
        public_token: &str,
        investments: bool,
    ) -> Result<(), String> {
        let exchange = self
            .plaid_once(
                credentials,
                "/item/public_token/exchange",
                json!({ "public_token": public_token }),
            )
            .await
            .map_err(|error| error.to_string())?;
        let item_id = string_field(&exchange, "item_id")?;
        let access_token = string_field(&exchange, "access_token")?;
        let new_item = PlaidItem {
            item_id: item_id.clone(),
            access_token: access_token.clone(),
            institution_id: None,
            institution_name: None,
            investments,
            statements: false,
        };
        // Public tokens are one-time credentials. Persist each access token before
        // any optional metadata request or the next exchange can fail.
        self.mutate_link_vault(session_id, |vault| upsert_plaid_item(vault, new_item))?;

        let item = self
            .plaid_read(
                credentials,
                "/item/get",
                json!({ "access_token": &access_token }),
            )
            .await
            .ok()
            .map(|(value, _)| value);
        let institution_id = item
            .as_ref()
            .and_then(|value| value.pointer("/item/institution_id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let institution_name = if let Some(id) = institution_id.as_deref() {
            self.plaid_read(
                credentials,
                "/institutions/get_by_id",
                json!({ "institution_id": id, "country_codes": ["US"] }),
            )
            .await
            .ok()
            .map(|(value, _)| value)
            .as_ref()
            .and_then(|value| value.pointer("/institution/name"))
            .and_then(Value::as_str)
            .map(str::to_owned)
        } else {
            None
        };
        if institution_id.is_some() || institution_name.is_some() {
            self.mutate_link_vault(session_id, |vault| {
                update_plaid_items(vault, |items| {
                    if let Some(stored) = items.iter_mut().find(|item| item.item_id == item_id) {
                        stored.institution_id = institution_id;
                        stored.institution_name = institution_name;
                    }
                })
            })?;
        }
        Ok(())
    }

    pub(super) async fn poll_link_inner(
        &self,
        provider: &str,
        session_id: &str,
        pending: PendingLink,
        browser_completed: bool,
    ) -> Result<LinkStatus, String> {
        match (provider, pending) {
            (
                linked_provider @ ("plaid" | "plaid-investments" | "plaid-statements"),
                PendingLink::Plaid {
                    link_token,
                    exchanged_public_tokens,
                    investments,
                    repairing,
                    statements_item,
                },
            ) if investments == (linked_provider == "plaid-investments")
                && statements_item.is_some() == (linked_provider == "plaid-statements") =>
            {
                let credentials = self.plaid_credentials()?;
                let response = self
                    .plaid_read(
                        &credentials,
                        "/link/token/get",
                        json!({ "link_token": link_token }),
                    )
                    .await
                    .map(|(value, _)| value)
                    .map_err(|error| error.to_string())?;
                let public_tokens = plaid_public_tokens(&response);
                if public_tokens.is_empty() && !plaid_link_completed(&response) {
                    if let Some(message) = plaid_link_exit(&response) {
                        return Err(message);
                    }
                }
                if repairing {
                    let completed = plaid_link_completed(&response);
                    if let Some(item_id) = statements_item.filter(|_| completed) {
                        self.mutate_link_vault(session_id, |vault| {
                            update_plaid_items(vault, |items| {
                                if let Some(item) =
                                    items.iter_mut().find(|item| item.item_id == item_id)
                                {
                                    item.statements = true;
                                }
                            })
                        })?;
                    }
                    return Ok(LinkStatus::of(completed));
                }
                if public_tokens.is_empty() {
                    return Ok(LinkStatus::of(false));
                }

                let mut exchanged = exchanged_public_tokens;
                for public_token in public_tokens {
                    if exchanged.contains(&public_token) {
                        continue;
                    }
                    self.exchange_public_token(
                        &credentials,
                        session_id,
                        &public_token,
                        investments,
                    )
                    .await?;
                    exchanged.push(public_token);
                    if let Some(PendingSession {
                        link:
                            PendingLink::Plaid {
                                exchanged_public_tokens,
                                ..
                            },
                        ..
                    }) = self
                        .links
                        .lock()
                        .map_err(|_| "Provider link state is unavailable".to_string())?
                        .get_mut(session_id)
                    {
                        *exchanged_public_tokens = exchanged.clone();
                    }
                }
                Ok(LinkStatus::of(plaid_link_completed(&response)))
            }
            (
                "snaptrade",
                PendingLink::SnapTrade {
                    existing_connection_ids,
                },
            ) => {
                let credentials = self.snaptrade_credentials()?;
                let response = self.snaptrade_get(&credentials, "/authorizations").await?;
                snaptrade_link_status(&response, &existing_connection_ids, browser_completed)
            }
            _ => Err("Provider does not match this link session".into()),
        }
    }

    pub fn finish_link(&self, session_id: &str) -> Result<(), String> {
        if let Some(session) = self
            .links
            .lock()
            .map_err(|_| "Provider link state is unavailable".to_string())?
            .remove(session_id)
        {
            session.cancel.send_replace(true);
        }
        Ok(())
    }
    pub(super) async fn ensure_plaid_user(
        &self,
        credentials: &PlaidCredentials,
        generation: u64,
    ) -> Result<PlaidUser, String> {
        if let Some(user) = self.read_secret::<PlaidUser>(PLAID_USER_KEY)? {
            if user.client_id == credentials.client_id && plaid_user_identifier(&user).is_ok() {
                return Ok(user);
            }
        }

        let response = self
            .plaid_once(
                credentials,
                "/user/create",
                json!({ "client_user_id": "brief-local-owner" }),
            )
            .await
            .map_err(|error| error.to_string())?;
        let user = PlaidUser {
            client_id: credentials.client_id.clone(),
            user_id: response
                .get("user_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            user_token: response
                .get("user_token")
                .and_then(Value::as_str)
                .map(str::to_owned),
        };
        plaid_user_identifier(&user)?;
        self.mutate_vault(Some(generation), false, |vault| {
            vault.entries.insert(
                PLAID_USER_KEY.into(),
                serde_json::to_value(&user).map_err(|error| error.to_string())?,
            );
            Ok(())
        })?;
        Ok(user)
    }

    pub(super) async fn plaid_once(
        &self,
        credentials: &PlaidCredentials,
        path: &str,
        body: Value,
    ) -> Result<Value, ProviderError> {
        self.plaid_request(credentials, path, body, false)
            .await
            .map(|(value, _)| value)
    }

    pub(super) async fn plaid_read(
        &self,
        credentials: &PlaidCredentials,
        path: &str,
        body: Value,
    ) -> Result<(Value, usize), ProviderError> {
        self.plaid_request(credentials, path, body, true).await
    }

    pub(super) async fn plaid_request(
        &self,
        credentials: &PlaidCredentials,
        path: &str,
        body: Value,
        safe_read: bool,
    ) -> Result<(Value, usize), ProviderError> {
        const MAX_ATTEMPTS: usize = 3;
        let max_attempts = if safe_read { MAX_ATTEMPTS } else { 1 };
        for attempt in 1..=max_attempts {
            crate::providers::http::count_request("Plaid", attempt > 1);
            let response = self
                .http
                .post(format!("https://production.plaid.com{path}"))
                .header("PLAID-CLIENT-ID", &credentials.client_id)
                .header("PLAID-SECRET", &credentials.secret)
                .json(&body)
                .send()
                .await;
            let result = match response {
                Ok(response) => {
                    ProviderError::from_response("Plaid", path, response, attempt).await
                }
                Err(_) => Err(ProviderError::network("Plaid", path, attempt)),
            };
            match result {
                Ok(value) => return Ok((value, attempt - 1)),
                Err(error) if error.should_retry_now(safe_read, attempt, max_attempts) => {
                    tokio::time::sleep(error.retry_delay(attempt)).await;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    }
}

/// A hosted Link token request: a new login for transactions or investments, or a repair of
/// the item with `repair_token`.
fn plaid_link_request(investments: bool, repair_token: Option<String>) -> Value {
    let mut request = json!({
        "client_name": "Brief",
        "country_codes": ["US"],
        "hosted_link": { "url_lifetime_seconds": 1800 },
        "language": "en",
    });
    match repair_token {
        Some(token) => request["access_token"] = Value::String(token),
        None => {
            request["enable_multi_item_link"] = Value::Bool(true);
            if investments {
                request["products"] = json!(["investments"]);
                request["account_filters"] =
                    json!({ "investment": { "account_subtypes": ["all"] } });
            } else {
                request["products"] = json!(["transactions"]);
                request["account_filters"] = json!({
                    "credit": { "account_subtypes": ["all"] },
                    "depository": { "account_subtypes": ["all"] }
                });
                request["transactions"] = json!({ "days_requested": 730 });
            }
        }
    }
    if !investments {
        // Consent only: Liabilities is not fetched or billed until the user enables card
        // statement details in Settings.
        request["additional_consented_products"] = json!(["liabilities"]);
    }
    request
}

/// For a bank connection: `Some(true)` once statements were added, `Some(false)` when they
/// can be, which needs the institution to support Statements and the item to hold a checking or
/// savings account, and `None` otherwise, so Settings offers only what Plaid can deliver.
fn statements_state(item: &PlaidItem, cached: Option<&PlaidItemCache>) -> Option<bool> {
    if item.investments {
        return None;
    }
    if item.statements {
        return Some(true);
    }
    let cached = cached?;
    let has_bank_account = cached.data.as_ref().is_some_and(|data| {
        data.accounts
            .iter()
            .any(|account| account.get("type").and_then(Value::as_str) == Some("depository"))
    });
    (cached.statements_supported == Some(true) && has_bank_account).then_some(false)
}

/// The statements Plaid may extract: up to two years before `today`.
pub(super) const STATEMENT_DAYS: i64 = 730;

/// An update-mode Link for the item with `access_token` that adds statements from the last two
/// years. Plaid asks only for consent when the item was linked within that time.
pub(super) fn plaid_statements_link_request(access_token: String, today: NaiveDate) -> Value {
    json!({
        "client_name": "Brief",
        "country_codes": ["US"],
        "hosted_link": { "url_lifetime_seconds": 1800 },
        "language": "en",
        "access_token": access_token,
        "products": ["statements"],
        "statements": {
            "start_date": (today - Duration::days(STATEMENT_DAYS)).to_string(),
            "end_date": today.to_string(),
        },
    })
}

pub(super) fn plaid_user_identifier(user: &PlaidUser) -> Result<(&'static str, &str), String> {
    if let Some(user_token) = user.user_token.as_deref() {
        Ok(("user_token", user_token))
    } else if let Some(user_id) = user.user_id.as_deref() {
        Ok(("user_id", user_id))
    } else {
        Err("Plaid did not return a user ID for Multi-Item Link".into())
    }
}

pub(super) fn plaid_public_tokens(value: &Value) -> Vec<String> {
    let mut tokens = Vec::new();
    for session in array(value, "link_sessions") {
        if let Some(token) = session
            .pointer("/on_success/public_token")
            .and_then(Value::as_str)
        {
            tokens.push(token.into());
        }
        if let Some(results) = session
            .pointer("/results/item_add_results")
            .and_then(Value::as_array)
        {
            tokens.extend(
                results
                    .iter()
                    .filter_map(|item| item.get("public_token").and_then(Value::as_str))
                    .map(str::to_owned),
            );
        }
    }
    tokens.sort();
    tokens.dedup();
    tokens
}

pub(super) fn plaid_link_completed(value: &Value) -> bool {
    array(value, "link_sessions").iter().any(|session| {
        session.get("finished_at").and_then(Value::as_str).is_some()
            && (session.get("on_success").is_some_and(Value::is_object)
                || session
                    .pointer("/results/item_add_results")
                    .and_then(Value::as_array)
                    .is_some_and(|items| !items.is_empty())
                || array(session, "events").iter().any(|event| {
                    event.get("event_name").and_then(Value::as_str) == Some("HANDOFF")
                }))
    })
}

/// A Link that ended without an Item reports Plaid's error code, never its free-text
/// message, so a failed attempt stops polling instead of waiting out the session.
pub(super) fn plaid_link_exit(value: &Value) -> Option<String> {
    let sessions = array(value, "link_sessions");
    if sessions.is_empty()
        || sessions
            .iter()
            .any(|session| session.get("finished_at").and_then(Value::as_str).is_none())
    {
        return None;
    }
    let exits = sessions
        .iter()
        .filter_map(|session| {
            session
                .get("exit")
                .or_else(|| session.get("on_exit"))
                .filter(|exit| exit.is_object())
        })
        .collect::<Vec<_>>();
    if exits.is_empty() {
        return None;
    }
    Some(
        match exits
            .iter()
            .find_map(|exit| http::safe_metadata(exit.pointer("/error/error_code")))
        {
            Some(code) => format!("Plaid could not connect ({code})"),
            None => "Plaid Link closed before connecting".into(),
        },
    )
}

mod statements;
mod sync;
#[cfg(test)]
mod tests;

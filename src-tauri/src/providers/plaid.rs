use super::*;

impl Providers {
    pub fn connections(&self, cache: &PlaidCache) -> Result<Vec<ProviderConnection>, String> {
        Ok(self
            .read_secret::<Vec<PlaidItem>>(PLAID_ITEMS_KEY)?
            .unwrap_or_default()
            .into_iter()
            .map(|item| ProviderConnection {
                error: cache
                    .items
                    .get(&item.item_id)
                    .and_then(|cached| cached.error.clone()),
                item_id: item.item_id,
                name: item
                    .institution_name
                    .unwrap_or_else(|| "Plaid connection".into()),
                provider: if item.investments {
                    "plaid-investments"
                } else {
                    "plaid"
                }
                .into(),
            })
            .collect())
    }

    pub async fn plaid_recurring_report(&self) -> Result<PlaidRecurringReport, String> {
        let credentials = self.plaid_credentials()?;
        let items = self
            .read_secret::<Vec<PlaidItem>>(PLAID_ITEMS_KEY)?
            .unwrap_or_default()
            .into_iter()
            .filter(|item| !item.investments)
            .collect::<Vec<_>>();
        if items.is_empty() {
            return Err("Connect a Plaid bank or credit card account first".into());
        }
        let mut connections = Vec::with_capacity(items.len());
        for item in items {
            let result = self
                .plaid_read(
                    &credentials,
                    "/transactions/recurring/get",
                    json!({ "access_token": &item.access_token }),
                )
                .await
                .map(|(value, _)| value)
                .map_err(|error| error.to_string())
                .and_then(parse_plaid_recurring_streams);
            let (streams, error) = match result {
                Ok(streams) => (streams, None),
                Err(error) => (Vec::new(), Some(error)),
            };
            connections.push(PlaidRecurringConnection {
                item_id: item.item_id,
                name: item
                    .institution_name
                    .unwrap_or_else(|| "Plaid connection".into()),
                streams,
                error,
            });
        }
        Ok(PlaidRecurringReport {
            fetched_at: Utc::now().to_rfc3339(),
            connections,
        })
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
            "plaid" | "plaid-investments" => {
                let credentials = self.plaid_credentials()?;
                let user = self.ensure_plaid_user(&credentials, generation).await?;
                let (user_field, user_identifier) = plaid_user_identifier(&user)?;
                let investments = provider == "plaid-investments";
                let mut request = json!({
                    "client_name": "Brief",
                    "country_codes": ["US"],
                    "enable_multi_item_link": true,
                    "hosted_link": { "url_lifetime_seconds": 1800 },
                    "language": "en",
                    "products": [if investments { "investments" } else { "transactions" }]
                });
                request["account_filters"] = if investments {
                    json!({ "investment": { "account_subtypes": ["stock plan"] } })
                } else {
                    json!({
                        "credit": { "account_subtypes": ["credit card"] },
                        "depository": { "account_subtypes": ["checking", "savings"] }
                    })
                };
                if !investments {
                    request["transactions"] = json!({ "days_requested": 730 });
                }
                request[user_field] = Value::String(user_identifier.into());
                if let Some(id) = item_id {
                    let item = self
                        .read_secret::<Vec<PlaidItem>>(PLAID_ITEMS_KEY)?
                        .unwrap_or_default()
                        .into_iter()
                        .find(|item| item.item_id == id && item.investments == investments)
                        .ok_or("Unknown Plaid connection")?;
                    let object = request.as_object_mut().ok_or("Invalid link request")?;
                    for field in [
                        "products",
                        "enable_multi_item_link",
                        "account_filters",
                        "transactions",
                    ] {
                        object.remove(field);
                    }
                    object.insert("access_token".into(), Value::String(item.access_token));
                }
                let response = self
                    .plaid_once(&credentials, "/link/token/create", request)
                    .await
                    .map_err(|error| error.to_string())?;
                let link_token = string_field(&response, "link_token")?;
                let url = string_field(&response, "hosted_link_url")?;
                (
                    url,
                    PendingLink::Plaid {
                        link_token,
                        exchanged_public_tokens: Vec::new(),
                        investments,
                        repairing: item_id.is_some(),
                    },
                )
            }
            "snaptrade" => {
                let credentials = self.snaptrade_credentials()?;
                let connections = self
                    .snaptrade_request(&credentials, Method::GET, "/authorizations", vec![], None)
                    .await?;
                let existing_connection_ids = snaptrade_active_connection_ids(&connections)?;
                let response = self
                    .snaptrade_request(
                        &credentials,
                        Method::POST,
                        "/snapTrade/login",
                        vec![],
                        Some(json!({
                            "connectionType": "read",
                            "showCloseButton": true
                        })),
                    )
                    .await?;
                (
                    string_field(&response, "redirectURI")?,
                    PendingLink::SnapTrade {
                        existing_connection_ids,
                    },
                )
            }
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

    pub(super) async fn poll_link_inner(
        &self,
        provider: &str,
        session_id: &str,
        pending: PendingLink,
        browser_completed: bool,
    ) -> Result<LinkStatus, String> {
        match (provider, pending) {
            (
                linked_provider @ ("plaid" | "plaid-investments"),
                PendingLink::Plaid {
                    link_token,
                    exchanged_public_tokens,
                    investments,
                    repairing,
                },
            ) if investments == (linked_provider == "plaid-investments") => {
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
                if repairing {
                    return Ok(if plaid_link_completed(&response) {
                        LinkStatus {
                            status: "connected".into(),
                        }
                    } else {
                        pending_status()
                    });
                }
                if public_tokens.is_empty() {
                    return Ok(pending_status());
                }

                let mut exchanged = exchanged_public_tokens;
                for public_token in public_tokens {
                    if exchanged.contains(&public_token) {
                        continue;
                    }
                    let exchange = self
                        .plaid_once(
                            &credentials,
                            "/item/public_token/exchange",
                            json!({ "public_token": public_token }),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    let item_id = string_field(&exchange, "item_id")?;
                    let access_token = string_field(&exchange, "access_token")?;
                    {
                        let new_item = PlaidItem {
                            item_id: item_id.clone(),
                            access_token: access_token.clone(),
                            institution_id: None,
                            institution_name: None,
                            investments,
                        };
                        // Public tokens are one-time credentials. Persist each access token before
                        // any optional metadata request or the next exchange can fail.
                        self.mutate_link_vault(session_id, |vault| {
                            upsert_plaid_item(vault, new_item)
                        })?;

                        let item = self
                            .plaid_read(
                                &credentials,
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
                                &credentials,
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
                                    if let Some(stored) =
                                        items.iter_mut().find(|item| item.item_id == item_id)
                                    {
                                        stored.institution_id = institution_id;
                                        stored.institution_name = institution_name;
                                    }
                                })
                            })?;
                        }
                    }
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
                Ok(if plaid_link_completed(&response) {
                    LinkStatus {
                        status: "connected".into(),
                    }
                } else {
                    pending_status()
                })
            }
            (
                "snaptrade",
                PendingLink::SnapTrade {
                    existing_connection_ids,
                },
            ) => {
                let credentials = self.snaptrade_credentials()?;
                let response = self
                    .snaptrade_request(&credentials, Method::GET, "/authorizations", vec![], None)
                    .await?;
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

    pub async fn sync_plaid(
        &self,
        cache: &mut PlaidCache,
        previous: Option<&PlaidData>,
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
        let mut data = PlaidData::default();
        cache
            .items
            .retain(|id, _| items.iter().any(|item| &item.item_id == id));
        let item_count = items.len();
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
                let result = self
                    .sync_plaid_item(credentials, &item, &mut staged, index, item_count, progress)
                    .await;
                (index, item, staged, result)
            }
        }))
        .buffer_unordered(PLAID_ITEM_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
        results.sort_by_key(|(index, _, _, _)| *index);

        let mut legacy_failure = false;
        let mut first_error = None;
        for (_, item, mut staged, result) in results {
            match result {
                Ok(fresh) => {
                    data.refreshed_account_ids.extend(
                        fresh
                            .accounts
                            .iter()
                            .chain(&fresh.investment_accounts)
                            .filter_map(|account| account.get("account_id").and_then(Value::as_str))
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
                    data.warnings.push(format!("Plaid connection: {message}. Repair or forget this connection in Settings."));
                    if let Some(saved) = &cached.data {
                        merge_plaid_data(&mut data, saved.clone());
                    } else {
                        legacy_failure = true;
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

    pub(super) async fn sync_plaid_item(
        &self,
        credentials: &PlaidCredentials,
        item: &PlaidItem,
        item_cache: &mut PlaidItemCache,
        item_index: usize,
        item_count: usize,
        progress: &(dyn Fn(PlaidRefreshProgress) + Sync),
    ) -> Result<PlaidData, ProviderError> {
        let mut data = PlaidData::default();
        if item.investments {
            report_plaid_progress(
                progress,
                item_index,
                item_count,
                "holdings",
                "Checking".into(),
                "active",
            );
            let started = Instant::now();
            let result = self
                .plaid_read(
                    credentials,
                    "/investments/holdings/get",
                    json!({ "access_token": &item.access_token }),
                )
                .await;
            let status = match &result {
                Ok((_, retries)) => measured_status(started, completion_status(*retries)),
                Err(_) => measured_status(started, "Failed"),
            };
            report_plaid_progress(
                progress,
                item_index,
                item_count,
                "holdings",
                status,
                if result.is_ok() {
                    "complete"
                } else {
                    "warning"
                },
            );
            let (investments, _) = result?;
            let balance_fetched_at = Utc::now().to_rfc3339();
            let institution = item
                .institution_name
                .as_deref()
                .unwrap_or("Unknown institution");
            for mut account in required_array(&investments, "accounts").map_err(|_| {
                ProviderError::invalid_response("Plaid", "/investments/holdings/get", 1)
            })? {
                if let Some(object) = account.as_object_mut() {
                    object.insert("institution_name".into(), Value::String(institution.into()));
                    object.insert(
                        "balance_fetched_at".into(),
                        Value::String(balance_fetched_at.clone()),
                    );
                }
                data.investment_accounts.push(account);
            }
            data.holdings
                .extend(required_array(&investments, "holdings").map_err(|_| {
                    ProviderError::invalid_response("Plaid", "/investments/holdings/get", 1)
                })?);
            data.securities
                .extend(required_array(&investments, "securities").map_err(|_| {
                    ProviderError::invalid_response("Plaid", "/investments/holdings/get", 1)
                })?);
            return Ok(data);
        }

        report_plaid_progress(
            progress,
            item_index,
            item_count,
            "transactions",
            "Checking".into(),
            "active",
        );
        report_plaid_progress(
            progress,
            item_index,
            item_count,
            "balances",
            "Checking".into(),
            "active",
        );
        let transaction_sync = async {
            let started = Instant::now();
            let result = async {
                let mut requests = 0;
                let mut retries = 0;
                let starting_cursor = item_cache.cursor.clone();
                let starting_transactions: BTreeMap<String, Value> = item_cache
                    .transactions
                    .iter()
                    .filter_map(|transaction| {
                        let id = transaction
                            .get("transaction_id")
                            .and_then(Value::as_str)
                            .map(str::to_owned)?;
                        Some((id, transaction.clone()))
                    })
                    .collect();
                let mut restarts = 0;
                loop {
                    let mut cursor = starting_cursor.clone();
                    let mut transactions = starting_transactions.clone();
                    let mut seen = BTreeSet::new();
                    if let Some(cursor) = &cursor {
                        seen.insert(cursor.clone());
                    }
                    let pagination: Result<(String, BTreeMap<String, Value>), ProviderError> =
                        async {
                            for page in 0..100 {
                                let mut request = json!({
                                    "access_token": &item.access_token,
                                    "count": 500
                                });
                                if let Some(value) = cursor.as_deref() {
                                    request["cursor"] = Value::String(value.into());
                                }
                                requests += 1;
                                let (sync, request_retries) = self
                                    .plaid_read(credentials, "/transactions/sync", request)
                                    .await?;
                                retries += request_retries;
                                for transaction in required_arrays(&sync, &["added", "modified"])
                                    .map_err(|_| {
                                        ProviderError::invalid_response(
                                            "Plaid",
                                            "/transactions/sync",
                                            1,
                                        )
                                    })?
                                {
                                    if let Some(id) =
                                        transaction.get("transaction_id").and_then(Value::as_str)
                                    {
                                        transactions.insert(id.to_owned(), transaction);
                                    }
                                }
                                for removed in required_array(&sync, "removed").map_err(|_| {
                                    ProviderError::invalid_response(
                                        "Plaid",
                                        "/transactions/sync",
                                        1,
                                    )
                                })? {
                                    if let Some(id) =
                                        removed.get("transaction_id").and_then(Value::as_str)
                                    {
                                        transactions.remove(id);
                                    }
                                }
                                let next_cursor =
                                    string_field(&sync, "next_cursor").map_err(|_| {
                                        ProviderError::invalid_response(
                                            "Plaid",
                                            "/transactions/sync",
                                            1,
                                        )
                                    })?;
                                cursor = Some(next_cursor.clone());
                                let has_more = bool_field(&sync, "has_more").map_err(|_| {
                                    ProviderError::invalid_response(
                                        "Plaid",
                                        "/transactions/sync",
                                        1,
                                    )
                                })?;
                                validate_plaid_page(&next_cursor, has_more, page, &mut seen)
                                    .map_err(|_| {
                                        ProviderError::invalid_response(
                                            "Plaid",
                                            "/transactions/sync",
                                            1,
                                        )
                                    })?;
                                if !has_more {
                                    return Ok((next_cursor, transactions));
                                }
                            }
                            Err(ProviderError::invalid_response(
                                "Plaid",
                                "/transactions/sync",
                                1,
                            ))
                        }
                        .await;

                    match pagination {
                        Ok((cursor, transactions)) => {
                            item_cache.cursor = Some(cursor);
                            item_cache.transactions = transactions.into_values().collect();
                            if item_cache.transaction_history_start.is_none() {
                                item_cache.transaction_history_start = if starting_cursor.is_none()
                                {
                                    Some(
                                        (Utc::now() - Duration::days(729)).date_naive().to_string(),
                                    )
                                } else {
                                    item_cache
                                        .transactions
                                        .iter()
                                        .filter_map(|transaction| {
                                            transaction["datetime"]
                                                .as_str()
                                                .or_else(|| transaction["date"].as_str())
                                                .and_then(|date| date.get(..10))
                                        })
                                        .min()
                                        .map(str::to_owned)
                                };
                            }
                            break;
                        }
                        Err(error)
                            if error.code()
                                == Some("TRANSACTIONS_SYNC_MUTATION_DURING_PAGINATION") =>
                        {
                            restarts += 1;
                            if restarts > 2 {
                                return Err(error);
                            }
                            continue;
                        }
                        Err(error) => return Err(error),
                    }
                }
                Ok::<_, ProviderError>((requests, retries))
            }
            .await;
            let status = match &result {
                Ok((requests, retries)) => {
                    let requests_label = if *requests == 1 {
                        "request"
                    } else {
                        "requests"
                    };
                    let retries_label = match retries {
                        0 => String::new(),
                        1 => " · 1 retry".into(),
                        retries => format!(" · {retries} retries"),
                    };
                    measured_status(
                        started,
                        format!("Complete · {requests} {requests_label}{retries_label}"),
                    )
                }
                Err(_) => measured_status(started, "Failed"),
            };
            report_plaid_progress(
                progress,
                item_index,
                item_count,
                "transactions",
                status,
                if result.is_ok() {
                    "complete"
                } else {
                    "warning"
                },
            );
            result.map(|_| ())
        };
        let balance_sync = async {
            let started = Instant::now();
            let result = self
                .plaid_read(
                    credentials,
                    "/accounts/balance/get",
                    json!({
                        "access_token": item.access_token,
                        "options": {
                            "min_last_updated_datetime": (Utc::now() - Duration::minutes(15)).to_rfc3339()
                        }
                    }),
                )
                .await;
            let status = match &result {
                Ok((_, retries)) => measured_status(started, completion_status(*retries)),
                Err(_) => measured_status(started, "Failed"),
            };
            report_plaid_progress(
                progress,
                item_index,
                item_count,
                "balances",
                status,
                if result.is_ok() {
                    "complete"
                } else {
                    "warning"
                },
            );
            let (balances, _) = result?;
            Ok::<_, ProviderError>((balances, Utc::now().to_rfc3339()))
        };
        let (_, (balances, balance_fetched_at)) = tokio::try_join!(transaction_sync, balance_sync)?;
        let mut allowed_ids = Vec::new();
        for mut account in required_array(&balances, "accounts")
            .map_err(|_| ProviderError::invalid_response("Plaid", "/accounts/balance/get", 1))?
        {
            let institution = item
                .institution_name
                .as_deref()
                .unwrap_or("Unknown institution");
            if allowed_plaid_account(&account) {
                if let Some(object) = account.as_object_mut() {
                    object.insert("institution_name".into(), Value::String(institution.into()));
                    object.insert(
                        "balance_fetched_at".into(),
                        Value::String(balance_fetched_at.clone()),
                    );
                }
                if let Some(id) = account.get("account_id").and_then(Value::as_str) {
                    allowed_ids.push(id.to_owned());
                }
                data.accounts.push(account);
            }
        }
        data.transactions.extend(
            item_cache
                .transactions
                .iter()
                .filter(|transaction| {
                    transaction
                        .get("account_id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| allowed_ids.iter().any(|allowed| allowed == id))
                })
                .cloned(),
        );
        if let Some(start) = &item_cache.transaction_history_start {
            data.transaction_history_start
                .extend(allowed_ids.into_iter().map(|id| (id, start.clone())));
        }
        Ok(data)
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

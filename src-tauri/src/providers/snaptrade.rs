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
        let all_accounts = self
            .snaptrade_request(&credentials, Method::GET, "/accounts", vec![], None)
            .await?
            .as_array()
            .cloned()
            .ok_or_else(|| "SnapTrade response did not include an account list".to_string())?;
        let balance_fetched_at = Utc::now().to_rfc3339();
        let results = stream::iter(
            all_accounts
                .into_iter()
                .filter(supported_snaptrade_account)
                .map(|mut account| {
                    let credentials = &credentials;
                    let balance_fetched_at = &balance_fetched_at;
                    async move {
                        let mut data = SnapTradeData {
                            history_complete: true,
                            activity_complete: true,
                            ..Default::default()
                        };

                        let id = account
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned();
                        // `/accounts` only returns brokerage accounts. `account_category` is optional and is
                        // commonly null for valid accounts, including E*Trade Individual and Roth IRA
                        // accounts, so it cannot be used as an inclusion filter.
                        account["balance_fetched_at"] = Value::String(balance_fetched_at.clone());
                        let positions_path = format!("/accounts/{id}/positions/all");
                        let cash_path = format!("/accounts/{id}/balances");
                        let history_path = format!("/accounts/{id}/balanceHistory");
                        let activities_path = format!("/accounts/{id}/activities");
                        let cached_history = cached.and_then(|data| data.balance_history.get(&id));
                        let cached_activities = cached.and_then(|data| data.activities.get(&id));
                        let recent_only = !backfill_activity && cached_activities.is_some();
                        let since = (Utc::now()
                            - Duration::days(if recent_only { 30 } else { 730 }))
                        .format("%Y-%m-%d")
                        .to_string();
                        let (positions, cash_balances, balance_history, activities) = tokio::join!(
                            self.snaptrade_request(
                                credentials,
                                Method::GET,
                                &positions_path,
                                vec![],
                                None,
                            ),
                            async {
                                tokio::time::timeout(
                                    StdDuration::from_secs(5),
                                    self.snaptrade_request(
                                        credentials,
                                        Method::GET,
                                        &cash_path,
                                        vec![],
                                        None,
                                    ),
                                )
                                .await
                                .map_err(|_| "request timed out".to_string())?
                            },
                            async {
                                if !refresh_history {
                                    if let Some(history) = cached_history {
                                        return Ok(history.clone());
                                    }
                                }
                                let response = tokio::time::timeout(
                                    StdDuration::from_secs(5),
                                    self.snaptrade_request(
                                        credentials,
                                        Method::GET,
                                        &history_path,
                                        vec![],
                                        None,
                                    ),
                                )
                                .await
                                .map_err(|_| "request timed out".to_string())??;
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
                        match cash_balances.and_then(|value| value.as_array().cloned().ok_or_else(|| "Cash balance response is not a list".into())) {
                            Ok(balances) => { data.cash_balances.insert(id.clone(), balances); }
                            Err(_) => data.warnings.push(format!("SnapTrade account {id} cash balance unavailable; using reported total")),
                        }
                        data.history_complete &= balance_history.is_ok();
                        data.activity_complete &= activities.is_ok();
                        let balance_history = history_or_cached(
                            balance_history,
                            cached_history,
                            &format!("SnapTrade account {id} balance history"),
                            &mut data.warnings,
                        );
                        data.balance_history.insert(id.clone(), balance_history);
                        if activities.is_err() {
                            account["sync_status"]["transactions"] = cached
                                .and_then(|data| {
                                    data.accounts.iter().find(|account| {
                                        account.get("id").and_then(Value::as_str) == Some(&id)
                                    })
                                })
                                .and_then(|account| account.pointer("/sync_status/transactions"))
                                .cloned()
                                .unwrap_or(Value::Null);
                        }
                        data.accounts.push(account);
                        let activities = history_or_cached(
                            activities,
                            cached_activities,
                            &format!("SnapTrade account {id} activities"),
                            &mut data.warnings,
                        );
                        data.activities.insert(id, activities);
                        Ok::<_, String>(data)
                    }
                }),
        )
        .buffered(3)
        .try_collect::<Vec<_>>()
        .await?;
        let mut data = SnapTradeData {
            history_complete: true,
            activity_complete: true,
            ..Default::default()
        };
        for account in results {
            data.accounts.extend(account.accounts);
            data.positions.extend(account.positions);
            data.cash_balances.extend(account.cash_balances);
            data.positions_as_of.extend(account.positions_as_of);
            data.activities.extend(account.activities);
            data.balance_history.extend(account.balance_history);
            data.warnings.extend(account.warnings);
            data.history_complete &= account.history_complete;
            data.activity_complete &= account.activity_complete;
        }
        Ok(data)
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

    pub(super) async fn snaptrade_request(
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
        match request.send().await {
            Ok(response) if retry.is_some() && retryable_status(response.status()) => {
                retry_pause(&response).await;
                let response = retry
                    .ok_or("SnapTrade request could not be retried")?
                    .send()
                    .await
                    .map_err(|_| "Could not reach SnapTrade".to_string())?;
                provider_response("SnapTrade", path, response).await
            }
            Ok(response) => provider_response("SnapTrade", path, response).await,
            Err(_) if retry.is_some() => {
                tokio::time::sleep(StdDuration::from_millis(250)).await;
                let response = retry
                    .ok_or("SnapTrade request could not be retried")?
                    .send()
                    .await
                    .map_err(|_| "Could not reach SnapTrade".to_string())?;
                provider_response("SnapTrade", path, response).await
            }
            Err(_) => Err("Could not reach SnapTrade".into()),
        }
    }
}

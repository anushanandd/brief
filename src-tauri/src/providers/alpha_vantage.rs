use super::*;

impl Providers {
    pub async fn earnings_calendar(
        &self,
        symbols: Vec<String>,
    ) -> Result<Vec<EarningsEvent>, String> {
        let symbols = normalize_stock_symbols(symbols)
            .into_iter()
            .collect::<BTreeSet<_>>();
        if symbols.is_empty() {
            return Ok(Vec::new());
        }
        let credentials = self.alpha_vantage_credentials()?;
        let events = self.alpha_vantage_earnings(None, &credentials).await?;
        Ok(events
            .into_iter()
            .filter(|event| symbols.contains(&event.symbol))
            .collect())
    }

    pub(super) async fn alpha_vantage_earnings(
        &self,
        symbol: Option<&str>,
        credentials: &AlphaVantageCredentials,
    ) -> Result<Vec<EarningsEvent>, String> {
        let generation = self.credential_generation.load(Ordering::SeqCst);
        // Serialize news/earnings requests to deduplicate misses and preserve one budget.
        let _news_request = self.news.lock().await;
        let mut cache = news_cache::NewsCache::new(self.news_cache_path.as_deref())?;
        let now = Utc::now().timestamp();
        if symbol.is_none() {
            if let Some(body) = cache.earnings(now)? {
                return parse_earnings_csv(&body);
            }
        }
        cache.reserve(None, now)?;
        let mut request = self.http.get("https://www.alphavantage.co/query").query(&[
            ("function", "EARNINGS_CALENDAR"),
            ("horizon", "3month"),
            ("apikey", credentials.api_key.as_str()),
        ]);
        if let Some(symbol) = symbol {
            request = request.query(&[("symbol", symbol)]);
        }
        let response = request
            .send()
            .await
            .map_err(|_| "Could not reach Alpha Vantage")?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok());
            cache.provider_throttle(retry_after, now)?;
        }
        if !response.status().is_success() {
            return Err(format!("Alpha Vantage returned {}", response.status()));
        }
        let body = String::from_utf8(
            http::response_bytes(response, http::MAX_PROVIDER_RESPONSE_BYTES)
                .await
                .map_err(|_| "Could not read Alpha Vantage earnings")?,
        )
        .map_err(|_| "Could not read Alpha Vantage earnings")?;
        if let Ok(payload) = serde_json::from_str::<Value>(&body) {
            cache.provider_limit(&payload, now)?;
            return Err(
                "Alpha Vantage earnings unavailable: quota or access limit; try again later".into(),
            );
        }
        let events = parse_earnings_csv(&body)?;
        if self.credential_generation.load(Ordering::SeqCst) != generation {
            return Err("Credentials changed; retry the request".into());
        }
        if symbol.is_none() {
            cache.save_earnings(&body, now)?;
        }
        Ok(events)
    }

    pub async fn market_news(
        &self,
        symbols: Vec<String>,
        refresh: bool,
    ) -> Result<news_cache::NewsResult, String> {
        let symbols = normalize_stock_symbols(symbols);
        if symbols.len() != 1 {
            return Err("Choose one ticker for news".into());
        }
        let symbol = &symbols[0];
        let _news_request = self.news.lock().await;
        let mut cache = news_cache::NewsCache::new(self.news_cache_path.as_deref())?;
        let now = Utc::now().timestamp();
        if !refresh {
            return cache.result(symbol, now, None);
        }
        let credentials = self.alpha_vantage_credentials()?;
        let generation = self.credential_generation.load(Ordering::SeqCst);
        if let Err(error) = cache.reserve(Some(symbol), now) {
            return cache.result(symbol, now, Some(error));
        }
        let fetched = async {
            let response = self
                .http
                .get("https://www.alphavantage.co/query")
                .timeout(StdDuration::from_secs(15))
                .query(&[
                    ("function", "NEWS_SENTIMENT"),
                    ("tickers", symbol.as_str()),
                    ("sort", "LATEST"),
                    ("limit", "25"),
                    ("apikey", credentials.api_key.as_str()),
                ])
                .send()
                .await
                .map_err(|_| "Could not reach Alpha Vantage news".to_string())?;
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse().ok());
                cache.provider_throttle(retry_after, now)?;
            }
            if !response.status().is_success() {
                return Err(format!("Alpha Vantage news returned {}", response.status()));
            }
            let payload: Value = serde_json::from_slice(
                &http::response_bytes(response, http::MAX_PROVIDER_RESPONSE_BYTES)
                    .await
                    .map_err(|_| "Could not read Alpha Vantage news".to_string())?,
            )
            .map_err(|_| "Could not read Alpha Vantage news".to_string())?;
            cache.provider_limit(&payload, now)?;
            parse_alpha_vantage_news(&payload, symbol)?;
            Ok(json!({"feed": payload["feed"]}))
        }
        .await;
        if self.credential_generation.load(Ordering::SeqCst) != generation {
            return Err("News credentials changed; retry the request".into());
        }
        let warning = match fetched {
            Ok(payload) => cache.save(symbol, &payload, now).err(),
            Err(error) => Some(error),
        };
        if let Some(ref warning) = warning {
            cache.failure(symbol, warning)?;
        }
        cache.result(symbol, now, warning)
    }
}

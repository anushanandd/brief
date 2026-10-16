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
        // Serialize earnings requests to deduplicate misses and preserve the daily budget.
        let mut cache = self.news_cache().await?;
        let now = Utc::now().timestamp();
        if symbol.is_none() {
            if let Some(body) = cache.earnings(now)? {
                return parse_earnings_csv(&body);
            }
        }
        cache.reserve(now)?;
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
            return Err("Earnings unavailable: quota reached".into());
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
}

pub(super) fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted && characters.peek() == Some(&'"') => {
                field.push('"');
                characters.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            '\r' => {}
            _ => field.push(character),
        }
    }
    fields.push(field);
    fields
}

pub(super) fn parse_earnings_csv(body: &str) -> Result<Vec<EarningsEvent>, String> {
    if body.trim_start().starts_with('{') {
        return Err("Alpha Vantage did not return an earnings calendar".into());
    }
    let mut lines = body.lines();
    let headers = csv_fields(
        lines
            .next()
            .ok_or("Alpha Vantage returned an empty calendar")?,
    );
    let column = |name: &str| {
        headers
            .iter()
            .position(|header| header.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("Alpha Vantage earnings calendar omitted {name}"))
    };
    let symbol = column("symbol")?;
    let name = column("name")?;
    let report_date = column("reportDate")?;
    let fiscal_date_ending = column("fiscalDateEnding").ok();
    let estimate = column("estimate").ok();
    let currency = column("currency").ok();
    let mut events = Vec::new();
    for line in lines.filter(|line| !line.trim().is_empty()) {
        let values = csv_fields(line);
        let Some(symbol_value) = values
            .get(symbol)
            .map(|value| value.trim().to_ascii_uppercase())
        else {
            continue;
        };
        let Some(date) = values.get(report_date).map(|value| value.trim()) else {
            continue;
        };
        if symbol_value.is_empty() || NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
            continue;
        }
        events.push(EarningsEvent {
            symbol: symbol_value,
            name: values
                .get(name)
                .map(|value| value.trim().to_owned())
                .unwrap_or_default(),
            report_date: date.to_owned(),
            fiscal_date_ending: fiscal_date_ending
                .and_then(|index| values.get(index))
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            estimate: estimate
                .and_then(|index| values.get(index))
                .and_then(|value| value.trim().parse().ok()),
            currency: currency
                .and_then(|index| values.get(index))
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
        });
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_earnings_calendar_rows() {
        let events = parse_earnings_csv(
            "symbol,name,reportDate,fiscalDateEnding,estimate,currency\nAAPL,\"Apple, Inc.\",2026-10-29,2026-09-30,1.42,USD\n",
        )
        .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].symbol, "AAPL");
        assert_eq!(events[0].name, "Apple, Inc.");
        assert_eq!(events[0].report_date, "2026-10-29");
        assert_eq!(events[0].estimate, Some(1.42));
    }
}

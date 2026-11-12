//! What others expect, labeled as theirs: analysts' estimates and how they've moved (Alpha
//! Vantage's free `EARNINGS_ESTIMATES`), the move the options market prices in around a report
//! (the at-the-money straddle in Alpaca's free indicative options feed), and, with a local
//! model, what management said about the outlook on the earnings call. Brief predicts nothing;
//! after a report it puts the outcome beside these.
use super::*;
use chrono::Datelike;
use serde_json::json;

/// A holding's estimates are fetched at most this often.
const ESTIMATES_DAYS: i64 = 7;
/// Background work leaves at least this much of Alpha Vantage's daily budget for pages.
const RESERVED_REQUESTS: usize = 10;
/// Estimates fetched per background run.
const ESTIMATES_PER_RUN: usize = 3;
/// A report this soon gets its expected move read.
const EXPECTED_MOVE_DAYS: i64 = 14;
/// An expected move is read again after this long.
const EXPECTED_MOVE_HOURS: i64 = 6;
/// A transcript's management remarks are cut here; a local model reads them whole.
const MAX_TRANSCRIPT_CHARS: usize = 80_000;
/// Sentences offered a local model when reading guidance from a call.
const CALL_CANDIDATES: usize = 120;

/// Analysts' consensus for one fiscal period now and 30 days ago, with revision counts.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EstimateTrend {
    pub fiscal_date_ending: String,
    /// `fiscal quarter` or `fiscal year`.
    pub horizon: String,
    pub eps: f64,
    pub eps_30_days_ago: Option<f64>,
    /// The consensus's change over 30 days, in percent of its size then.
    pub change_pct: Option<f64>,
    pub revisions_up: Option<u32>,
    pub revisions_down: Option<u32>,
    pub analysts: Option<u32>,
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
    .filter(|value: &f64| value.is_finite())
}

fn count(value: &Value) -> Option<u32> {
    number(value)
        .filter(|value| *value >= 0.0 && *value < 10_000.0)
        .map(|value| value.round() as u32)
}

/// The next fiscal quarter's and the current fiscal year's estimates after `today`: the
/// soonest period of each horizon ending on or after it.
pub(super) fn parse_estimates(
    payload: &Value,
    today: NaiveDate,
) -> Result<Vec<EstimateTrend>, String> {
    let estimates = payload
        .get("estimates")
        .and_then(Value::as_array)
        .ok_or("Alpha Vantage returned no estimates")?;
    let mut out: Vec<EstimateTrend> = Vec::new();
    for horizon in ["fiscal quarter", "fiscal year"] {
        let next = estimates
            .iter()
            .filter(|item| item["horizon"].as_str() == Some(horizon))
            .filter_map(|item| {
                let date = NaiveDate::parse_from_str(item["date"].as_str()?, "%Y-%m-%d").ok()?;
                (date >= today).then_some((date, item))
            })
            .min_by_key(|(date, _)| *date);
        let Some((date, item)) = next else {
            continue;
        };
        let Some(eps) = number(&item["eps_estimate_average"]) else {
            continue;
        };
        let before = number(&item["eps_estimate_average_30_days_ago"]);
        out.push(EstimateTrend {
            fiscal_date_ending: date.to_string(),
            horizon: horizon.into(),
            eps,
            eps_30_days_ago: before,
            change_pct: before
                .filter(|before| before.abs() > 0.005)
                .map(|before| ((eps - before) / before.abs() * 1_000.0).round() / 10.0),
            revisions_up: count(&item["eps_estimate_revision_up_trailing_30_days"]),
            revisions_down: count(&item["eps_estimate_revision_down_trailing_30_days"]),
            analysts: count(&item["eps_estimate_analyst_count"]),
        });
    }
    Ok(out)
}

/// One option's mid price: halfway between bid and ask when both are quoted, else the last
/// trade.
fn mid(snapshot: &Value) -> Option<f64> {
    let quote = &snapshot["latestQuote"];
    let (bid, ask) = (number(&quote["bp"]), number(&quote["ap"]));
    match (bid, ask) {
        (Some(bid), Some(ask)) if bid > 0.0 && ask >= bid => Some((bid + ask) / 2.0),
        _ => number(&snapshot["latestTrade"]["p"]).filter(|price| *price > 0.0),
    }
}

/// An OCC option symbol's expiration, type, and strike: `TEST261120C00125000`.
fn occ(symbol: &str) -> Option<(NaiveDate, char, f64)> {
    let tail = symbol.get(symbol.len().checked_sub(15)?..)?;
    let date = NaiveDate::parse_from_str(&format!("20{}", &tail[..6]), "%Y%m%d").ok()?;
    let kind = tail[6..7]
        .chars()
        .next()
        .filter(|kind| matches!(kind, 'C' | 'P'))?;
    let strike = tail[7..].parse::<u64>().ok()? as f64 / 1_000.0;
    Some((date, kind, strike))
}

/// The move the options market prices in through the first expiration that covers the report:
/// the at-the-money call and put's mid prices together, as a percent of the stock's price.
pub(super) fn expected_move(
    payload: &Value,
    price: f64,
    covers: NaiveDate,
) -> Option<(NaiveDate, f64)> {
    if price <= 0.0 {
        return None;
    }
    let options: Vec<(NaiveDate, char, f64, f64)> = payload["snapshots"]
        .as_object()?
        .iter()
        .filter_map(|(symbol, snapshot)| {
            let (date, kind, strike) = occ(symbol)?;
            Some((date, kind, strike, mid(snapshot)?))
        })
        .filter(|(date, ..)| *date >= covers)
        .collect();
    let expiry = options.iter().map(|(date, ..)| *date).min()?;
    // The strike nearest the price with both a call and a put quoted.
    let strikes: BTreeSet<u64> = options
        .iter()
        .filter(|(date, ..)| *date == expiry)
        .map(|(_, _, strike, _)| (strike * 1_000.0).round() as u64)
        .collect();
    let leg = |kind: char, strike: u64| {
        options
            .iter()
            .find(|(date, side, at, _)| {
                *date == expiry && *side == kind && (at * 1_000.0).round() as u64 == strike
            })
            .map(|(.., mid)| *mid)
    };
    let strike = strikes
        .into_iter()
        .filter(|strike| leg('C', *strike).is_some() && leg('P', *strike).is_some())
        .min_by(|a, b| {
            ((*a as f64 / 1_000.0) - price)
                .abs()
                .total_cmp(&((*b as f64 / 1_000.0) - price).abs())
        })?;
    let straddle = leg('C', strike)? + leg('P', strike)?;
    Some((expiry, (straddle / price * 1_000.0).round() / 10.0))
}

/// Management's remarks in a call transcript: the executives' turns, not the analysts'.
pub(super) fn management_remarks(payload: &Value) -> Option<String> {
    let turns = payload["transcript"].as_array()?;
    let text: String = turns
        .iter()
        .filter(|turn| {
            let title = turn["title"].as_str().unwrap_or("").to_lowercase();
            [
                "ceo",
                "cfo",
                "chief",
                "president",
                "chair",
                "officer",
                "founder",
            ]
            .iter()
            .any(|role| title.contains(role))
        })
        .filter_map(|turn| turn["content"].as_str())
        .collect::<Vec<_>>()
        .join(" ");
    (!text.trim().is_empty()).then(|| text.chars().take(MAX_TRANSCRIPT_CHARS).collect())
}

/// The calendar quarter a fiscal period ends in, as Alpha Vantage names transcripts: `2026Q3`.
pub(super) fn transcript_quarter(fiscal_date_ending: &str) -> Option<String> {
    let date = NaiveDate::parse_from_str(fiscal_date_ending, "%Y-%m-%d").ok()?;
    Some(format!("{}Q{}", date.year(), (date.month() - 1) / 3 + 1))
}

pub(crate) fn expected_key(symbol: &str, report_date: &str) -> String {
    format!("expected:{symbol}:{report_date}")
}

pub(crate) fn call_key(symbol: &str, quarter: &str) -> String {
    format!("call:{symbol}:{quarter}")
}

impl Providers {
    /// Whether background work may spend an Alpha Vantage request, keeping
    /// `RESERVED_REQUESTS` for pages.
    async fn background_budget(&self) -> bool {
        self.integration_status()
            .is_ok_and(|status| status.alpha_vantage)
            && self
                .news_cache()
                .await
                .ok()
                .and_then(|cache| cache.budget(Utc::now().timestamp()).ok())
                .is_some_and(|(remaining, cooldown)| {
                    cooldown.is_none() && remaining > RESERVED_REQUESTS
                })
    }

    async fn alpha_vantage_json(&self, query: &[(&str, &str)]) -> Result<Value, String> {
        let credentials = self.alpha_vantage_credentials()?;
        let now = Utc::now().timestamp();
        self.news_cache().await?.reserve(now)?;
        let mut parameters = query.to_vec();
        parameters.push(("apikey", credentials.api_key.as_str()));
        let response = self
            .http
            .get("https://www.alphavantage.co/query")
            .query(&parameters)
            .send()
            .await
            .map_err(|_| "Could not reach Alpha Vantage")?;
        if !response.status().is_success() {
            return Err(format!("Alpha Vantage returned {}", response.status()));
        }
        let body = http::response_bytes(response, 8 * 1024 * 1024)
            .await
            .map_err(|_| "Could not read Alpha Vantage")?;
        let payload: Value =
            serde_json::from_slice(&body).map_err(|_| "Could not read Alpha Vantage")?;
        if payload.get("Information").is_some() || payload.get("Note").is_some() {
            self.news_cache().await?.provider_limit(&payload, now)?;
            return Err("Alpha Vantage limit reached".into());
        }
        Ok(payload)
    }

    /// Reads what's expected of the holdings, a few per run: estimates weekly, the expected
    /// move before each report in the next two weeks, and, with a local model, the latest
    /// call's guidance.
    pub async fn read_expectations(
        &self,
        stocks: &[String],
        local: Option<&str>,
    ) -> Result<usize, String> {
        let now = Utc::now();
        let today = crate::session::new_york_date(now);
        let mut done = 0;
        // Estimates, oldest first.
        let mut stale: Vec<(i64, String)> = {
            let cache = self.news_cache().await?;
            stocks
                .iter()
                .filter_map(|symbol| {
                    let saved = cache
                        .saved_estimates(symbol)
                        .ok()
                        .flatten()
                        .map(|(_, at)| at);
                    saved
                        .is_none_or(|at| now.timestamp() - at >= ESTIMATES_DAYS * 86_400)
                        .then(|| (saved.unwrap_or(0), symbol.clone()))
                })
                .collect()
        };
        stale.sort();
        for (_, symbol) in stale.into_iter().take(ESTIMATES_PER_RUN) {
            if !self.background_budget().await {
                break;
            }
            let Ok(payload) = self
                .alpha_vantage_json(&[("function", "EARNINGS_ESTIMATES"), ("symbol", &symbol)])
                .await
            else {
                break;
            };
            if let Ok(estimates) = parse_estimates(&payload, today) {
                self.news_cache()
                    .await?
                    .save_estimates(&symbol, &estimates, now.timestamp())?;
                done += 1;
            }
        }
        // Expected moves before reports in the next two weeks, from the saved calendar.
        let calendar = self
            .news_cache()
            .await?
            .earnings(now.timestamp())?
            .and_then(|body| alpha_vantage::parse_earnings_csv(&body).ok())
            .unwrap_or_default();
        let upcoming: Vec<EarningsEvent> = calendar
            .into_iter()
            .filter(|event| stocks.contains(&event.symbol))
            .filter(|event| {
                NaiveDate::parse_from_str(&event.report_date, "%Y-%m-%d")
                    .is_ok_and(|day| day >= today && (day - today).num_days() <= EXPECTED_MOVE_DAYS)
            })
            .collect();
        if let (false, Ok(Some(credentials))) = (
            upcoming.is_empty(),
            self.read_secret::<AlpacaCredentials>(ALPACA_CREDENTIALS_KEY),
        ) {
            let symbols: Vec<String> = upcoming.iter().map(|event| event.symbol.clone()).collect();
            let prices = self
                .market_snapshots(symbols)
                .await
                .map(|snapshots| snapshots.snapshots)
                .unwrap_or_default();
            for event in &upcoming {
                let key = expected_key(&event.symbol, &event.report_date);
                let fresh = self
                    .news_cache()
                    .await?
                    .reading_saved(&key)?
                    .is_some_and(|at| now.timestamp() - at < EXPECTED_MOVE_HOURS * 3_600);
                let Some(price) = prices.get(&event.symbol).map(|snapshot| snapshot.price) else {
                    continue;
                };
                if fresh {
                    continue;
                }
                let report = NaiveDate::parse_from_str(&event.report_date, "%Y-%m-%d")
                    .map_err(|_| "Invalid report date")?;
                // After the close, the next day's session is the first to react.
                let covers = if event.time_of_day.as_deref() == Some("post-market") {
                    report + Duration::days(1)
                } else {
                    report
                };
                let through = (covers + Duration::days(10)).to_string();
                let from = covers.to_string();
                let path = format!("/v1beta1/options/snapshots/{}", event.symbol);
                let Ok(payload) = self
                    .alpaca_request(
                        &credentials,
                        &path,
                        &[
                            ("feed", "indicative"),
                            ("expiration_date_gte", from.as_str()),
                            ("expiration_date_lte", through.as_str()),
                            ("limit", "1000"),
                        ],
                    )
                    .await
                else {
                    continue;
                };
                if let Some((expiry, pct)) = expected_move(&payload, price, covers) {
                    self.news_cache().await?.save_reading(
                        &key,
                        &json!({ "pct": pct, "expiry": expiry.to_string(), "price": price }),
                        "options",
                        now.timestamp(),
                    )?;
                    done += 1;
                }
            }
        }
        // The latest call's guidance, only when a local model can read a whole call.
        let Some(model) = local else {
            return Ok(done);
        };
        for symbol in stocks {
            let quarter = self
                .news_cache()
                .await?
                .reported_quarters(symbol)?
                .and_then(|(quarters, _)| quarters.into_iter().next())
                .filter(|quarter| {
                    NaiveDate::parse_from_str(&quarter.reported_date, "%Y-%m-%d")
                        .is_ok_and(|day| (today - day).num_days() <= 30)
                })
                .and_then(|quarter| transcript_quarter(&quarter.fiscal_date_ending));
            let Some(quarter) = quarter else {
                continue;
            };
            let key = call_key(symbol, &quarter);
            if self.news_cache().await?.reading(&key)?.is_some() || !self.background_budget().await
            {
                continue;
            }
            let Ok(payload) = self
                .alpha_vantage_json(&[
                    ("function", "EARNINGS_CALL_TRANSCRIPT"),
                    ("symbol", symbol),
                    ("quarter", &quarter),
                ])
                .await
            else {
                break;
            };
            let candidates = management_remarks(&payload)
                .map(|remarks| earnings::guidance_candidates(&reading::sentences(&remarks)))
                .unwrap_or_default();
            let (guidance, reader) = if candidates.is_empty() {
                (Vec::new(), reading::RULE.to_owned())
            } else {
                let offered: Vec<String> = candidates.into_iter().take(CALL_CANDIDATES).collect();
                let input = reading::numbered("Call sentences:", &offered, MAX_TRANSCRIPT_CHARS);
                match self.local_guidance(model, input).await {
                    Ok(reply) => (reading::listed(&reply, &offered), format!("local:{model}")),
                    Err(_) => continue,
                }
            };
            self.news_cache().await?.save_reading(
                &key,
                &json!({ "sentences": guidance, "quarter": quarter }),
                &reader,
                now.timestamp(),
            )?;
            done += 1;
        }
        Ok(done)
    }

    async fn local_guidance(&self, model: &str, input: String) -> Result<Value, String> {
        self.read_text(reading::Task::Guidance, input, Some(model))
            .await
            .map(|(reply, _)| reply)
    }
}

/// A stock's estimates, and its next report's day with the move priced in for it.
pub(crate) type SavedExpectation = (Vec<EstimateTrend>, Option<(String, f64)>);

/// What's expected of a stock from saved data: its estimates, and its next report on or after
/// `today` with the move priced in for it.
pub(crate) fn saved_expectation(
    cache: &news_cache::NewsCache,
    symbol: &str,
    today: NaiveDate,
) -> Option<SavedExpectation> {
    let estimates = cache
        .saved_estimates(symbol)
        .ok()
        .flatten()
        .map(|(estimates, _)| estimates)
        .unwrap_or_default();
    let prefix = format!("expected:{symbol}:");
    let next = cache
        .readings_like(&prefix)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(key, reading)| {
            let day = key.strip_prefix(&prefix)?.to_owned();
            let date = NaiveDate::parse_from_str(&day, "%Y-%m-%d").ok()?;
            let pct = reading.get("pct")?.as_f64()?;
            (date >= today).then_some((day, pct))
        })
        .min_by(|a, b| a.0.cmp(&b.0));
    (!estimates.is_empty() || next.is_some()).then_some((estimates, next))
}

/// A report's expected move as saved before it, when one was read.
pub(crate) fn saved_expected_move(
    cache: &news_cache::NewsCache,
    symbol: &str,
    report_date: &str,
) -> Option<f64> {
    cache
        .reading(&expected_key(symbol, report_date))
        .ok()??
        .0
        .get("pct")?
        .as_f64()
}

/// The latest call's guidance sentences, with who chose them, when a local model read it.
pub(crate) fn saved_call_guidance(
    cache: &news_cache::NewsCache,
    symbol: &str,
    fiscal_date_ending: &str,
) -> Option<(Vec<String>, String)> {
    let quarter = transcript_quarter(fiscal_date_ending)?;
    let (reading, reader) = cache.reading(&call_key(symbol, &quarter)).ok()??;
    Some((
        reading["sentences"]
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        reader,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn estimates_keep_the_next_quarter_and_year_with_their_revisions() {
        let payload = json!({ "symbol": "TEST", "estimates": [
            { "date": "2027-12-31", "horizon": "fiscal year", "eps_estimate_average": "5.50" },
            { "date": "2026-12-31", "horizon": "fiscal year", "eps_estimate_average": "5.00",
              "eps_estimate_average_30_days_ago": "4.80", "eps_estimate_revision_up_trailing_30_days": "12.0000",
              "eps_estimate_revision_down_trailing_30_days": null, "eps_estimate_analyst_count": "20.0000" },
            { "date": "2026-09-30", "horizon": "fiscal quarter", "eps_estimate_average": "1.00" },
            { "date": "2026-12-31", "horizon": "fiscal quarter", "eps_estimate_average": "1.30",
              "eps_estimate_average_30_days_ago": "1.40" }
        ]});
        let estimates = parse_estimates(&payload, day("2026-11-08")).unwrap();
        assert_eq!(estimates.len(), 2);
        assert_eq!(estimates[0].horizon, "fiscal quarter");
        assert_eq!(estimates[0].fiscal_date_ending, "2026-12-31");
        assert_eq!(estimates[0].change_pct, Some(-7.1));
        assert_eq!(estimates[1].horizon, "fiscal year");
        assert_eq!(estimates[1].change_pct, Some(4.2));
        assert_eq!(estimates[1].revisions_up, Some(12));
        assert_eq!(estimates[1].revisions_down, None);
        assert_eq!(estimates[1].analysts, Some(20));
        assert!(parse_estimates(&json!({ "Information": "limit" }), day("2026-11-08")).is_err());
    }

    #[test]
    fn the_expected_move_is_the_at_the_money_straddle_through_the_report() {
        let quote = |bid: f64, ask: f64| json!({ "latestQuote": { "bp": bid, "ap": ask } });
        let payload = json!({ "snapshots": {
            // Expires before the report reacts: ignored.
            "TEST261119C00100000": quote(9.0, 9.5),
            "TEST261119P00100000": quote(9.0, 9.5),
            // The first expiry that covers it: strikes 95, 100, 105.
            "TEST261120C00100000": quote(4.0, 4.4),
            "TEST261120P00100000": quote(3.6, 4.0),
            "TEST261120C00105000": quote(2.0, 2.2),
            "TEST261120P00105000": quote(6.0, 6.4),
            "TEST261120C00095000": quote(7.0, 7.4),
            // No put quoted at 95, so it can't be the straddle.
            "TEST261127C00100000": quote(5.0, 5.4),
            "TEST261127P00100000": quote(5.0, 5.4)
        }});
        let (expiry, pct) = expected_move(&payload, 101.0, day("2026-11-20")).unwrap();
        assert_eq!(expiry, day("2026-11-20"));
        // (4.2 + 3.8) / 101 = 7.9%.
        assert_eq!(pct, 7.9);
        assert_eq!(expected_move(&payload, 0.0, day("2026-11-20")), None);
        assert_eq!(
            expected_move(&json!({ "snapshots": {} }), 101.0, day("2026-11-20")),
            None
        );
        assert_eq!(
            occ("BRKB261120C00450500"),
            Some((day("2026-11-20"), 'C', 450.5))
        );
        assert_eq!(occ("BAD"), None);
    }

    #[test]
    fn a_call_is_read_from_managements_remarks() {
        let payload = json!({ "symbol": "TEST", "quarter": "2026Q3", "transcript": [
            { "speaker": "Operator", "title": "Operator", "content": "Welcome to the call." },
            { "speaker": "Jane Doe", "title": "CEO", "content": "We expect revenue growth to continue next quarter." },
            { "speaker": "Sam Analyst", "title": "Analyst, Example Bank", "content": "Do you expect margins to rise next year?" },
            { "speaker": "John Roe", "title": "Chief Financial Officer", "content": "For the fourth quarter we expect revenue of about $1.3 billion." }
        ]});
        let remarks = management_remarks(&payload).unwrap();
        assert!(remarks.contains("We expect revenue growth"));
        assert!(remarks.contains("For the fourth quarter"));
        assert!(!remarks.contains("Do you expect margins"));
        assert_eq!(transcript_quarter("2026-09-30").as_deref(), Some("2026Q3"));
        assert_eq!(transcript_quarter("2027-01-31").as_deref(), Some("2027Q1"));
    }
}

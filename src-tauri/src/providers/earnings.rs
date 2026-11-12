//! Earnings, read: what a company reported against what analysts expected (Alpha Vantage's
//! reported quarters, within its free daily budget), and what its press release says (the
//! Exhibit 99.1 of an 8-K reporting Item 2.02, from EDGAR). Brief quotes the release's own
//! sentences for revenue and guidance; it never computes a figure from release text.
use super::*;
use serde_json::json;
use std::sync::LazyLock;

/// A release's text is cut here; past it are tables a sentence reader can't use.
const MAX_RELEASE_CHARS: usize = 120_000;
/// A report this recent has its release read and its result fetched.
const RECENT_REPORT_DAYS: i64 = 10;
/// Reported quarters fetched per background run, leaving the daily budget for the calendar.
const RESULTS_PER_RUN: usize = 3;
/// Releases read per background run.
const RELEASES_PER_RUN: usize = 4;
/// Sentences of a release offered for guidance: the outlook sits in the opening pages.
const GUIDANCE_CANDIDATES: usize = 40;

/// One quarter as a company reported it and as analysts expected it, from Alpha Vantage.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReportedQuarter {
    pub fiscal_date_ending: String,
    pub reported_date: String,
    pub reported_eps: Option<f64>,
    pub estimated_eps: Option<f64>,
    pub surprise_pct: Option<f64>,
    /// `pre-market` or `post-market`, when Alpha Vantage says.
    pub report_time: Option<String>,
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
    .filter(|value: &f64| value.is_finite())
}

/// The latest reported quarters in Alpha Vantage's `EARNINGS` response, newest first.
pub(super) fn parse_reported_quarters(payload: &Value) -> Result<Vec<ReportedQuarter>, String> {
    let quarters = payload
        .get("quarterlyEarnings")
        .and_then(Value::as_array)
        .ok_or("Alpha Vantage returned no reported quarters")?;
    Ok(quarters
        .iter()
        .take(8)
        .filter_map(|quarter| {
            let reported_date = quarter["reportedDate"].as_str()?.trim();
            NaiveDate::parse_from_str(reported_date, "%Y-%m-%d").ok()?;
            Some(ReportedQuarter {
                fiscal_date_ending: quarter["fiscalDateEnding"].as_str()?.trim().into(),
                reported_date: reported_date.into(),
                reported_eps: number(&quarter["reportedEPS"]),
                estimated_eps: number(&quarter["estimatedEPS"]),
                surprise_pct: number(&quarter["surprisePercentage"]),
                report_time: quarter["reportTime"]
                    .as_str()
                    .map(str::trim)
                    .filter(|time| matches!(*time, "pre-market" | "post-market"))
                    .map(str::to_owned),
            })
        })
        .collect())
}

/// The Exhibit 99.1 link in an EDGAR filing index page, when it lists one.
pub(super) fn exhibit_link(index_html: &str) -> Option<String> {
    static ROW: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(?is)<tr\b.*?</tr>").expect("valid pattern"));
    static LINK: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"(?i)href="(/Archives/edgar/data/\d+/\d+/[A-Za-z0-9._-]+\.html?)""#)
            .expect("valid pattern")
    });
    static TYPE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"(?i)>\s*EX-99\.0?1\s*<").expect("valid pattern"));
    ROW.find_iter(index_html)
        .map(|row| row.as_str())
        .find(|row| TYPE.is_match(row))
        .and_then(|row| LINK.captures(row))
        .map(|link| format!("https://www.sec.gov{}", &link[1]))
}

/// A filing's index page, from its document link: `…/{cik}/{accession}/` and the accession
/// written with its dashes.
pub(super) fn index_link(filing_url: &str) -> Option<String> {
    let path = filing_url.strip_prefix("https://www.sec.gov/Archives/edgar/data/")?;
    let mut parts = path.split('/');
    let cik = parts
        .next()
        .filter(|cik| cik.chars().all(|c| c.is_ascii_digit()))?;
    let accession = parts.next().filter(|accession| {
        accession.len() == 18 && accession.chars().all(|c| c.is_ascii_digit())
    })?;
    Some(format!(
        "https://www.sec.gov/Archives/edgar/data/{cik}/{accession}/{}-{}-{}-index.html",
        &accession[..10],
        &accession[10..12],
        &accession[12..]
    ))
}

static REVENUE: LazyLock<regex::Regex> =
    LazyLock::new(|| news::pattern(r"\b(revenue|revenues|net sales|total sales)\b.{0,80}\$\s?\d"));
static GUIDANCE: LazyLock<regex::Regex> = LazyLock::new(|| {
    news::pattern(
        r"\b(outlook|guidance|expects?|expected to|anticipates?|projects?|forecasts?|now sees|reaffirm(s|ed)?|rais(es|ed|ing)|lower(s|ed|ing)|narrow(s|ed|ing))\b",
    )
});
static FUTURE: LazyLock<regex::Regex> = LazyLock::new(|| {
    news::pattern(
        r"\b(next|coming|upcoming|fourth|third|second|first|full|fiscal)\s+(quarter|year|fiscal)|\bfiscal (year )?20\d\d\b|\b(q[1-4]|fy)\s?'?\d{2,4}\b|\bfor (the )?(year|quarter)\b",
    )
});

/// The release's first sentence that states revenue with an amount, quoted as written.
pub(crate) fn revenue_sentence(sentences: &[String]) -> Option<String> {
    sentences
        .iter()
        .take(GUIDANCE_CANDIDATES)
        .find(|sentence| sentence.chars().count() <= 400 && REVENUE.is_match(sentence))
        .cloned()
}

/// Sentences that may state guidance: outlook words about a future period.
pub(crate) fn guidance_candidates(sentences: &[String]) -> Vec<String> {
    sentences
        .iter()
        .filter(|sentence| sentence.chars().count() <= 500)
        .filter(|sentence| GUIDANCE.is_match(sentence) && FUTURE.is_match(sentence))
        .take(GUIDANCE_CANDIDATES)
        .cloned()
        .collect()
}

/// What a release says, as quoted sentences, and who chose the guidance among them.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseReading {
    pub url: String,
    pub revenue: Option<String>,
    pub guidance: Vec<String>,
    /// `apple`, `local:<model>`, or `rule`.
    pub guidance_by: String,
}

pub(crate) fn guidance_key(accession: &str) -> String {
    format!("guidance:{accession}")
}

/// The latest earnings report in a company's saved filings: an 8-K reporting Item 2.02.
pub(crate) fn latest_report(filings: &[sec::SecFiling]) -> Option<&sec::SecFiling> {
    filings
        .iter()
        .filter(|filing| filing.form.starts_with("8-K"))
        .filter(|filing| filing.items.iter().any(|item| item.code == "2.02"))
        .max_by(|a, b| a.filed_at.cmp(&b.filed_at))
}

impl Providers {
    /// A company's reported quarters, from Alpha Vantage, spending one request of the daily
    /// budget, saved for later reads.
    async fn fetch_reported_quarters(&self, symbol: &str) -> Result<Vec<ReportedQuarter>, String> {
        let credentials = self.alpha_vantage_credentials()?;
        let now = Utc::now().timestamp();
        self.news_cache().await?.reserve(now)?;
        let response = self
            .http
            .get("https://www.alphavantage.co/query")
            .query(&[
                ("function", "EARNINGS"),
                ("symbol", symbol),
                ("apikey", credentials.api_key.as_str()),
            ])
            .send()
            .await
            .map_err(|_| "Could not reach Alpha Vantage")?;
        if !response.status().is_success() {
            return Err(format!("Alpha Vantage returned {}", response.status()));
        }
        let body = http::response_bytes(response, 4 * 1024 * 1024)
            .await
            .map_err(|_| "Could not read Alpha Vantage earnings")?;
        let payload: Value =
            serde_json::from_slice(&body).map_err(|_| "Could not read Alpha Vantage earnings")?;
        let mut cache = self.news_cache().await?;
        if payload.get("quarterlyEarnings").is_none() {
            cache.provider_limit(&payload, now)?;
            return Err("Earnings unavailable: quota reached".into());
        }
        let quarters = parse_reported_quarters(&payload)?;
        cache.save_reported_quarters(symbol, &quarters, now)?;
        Ok(quarters)
    }

    /// Fetches what recent reporters reported and reads their releases, a few per run. A
    /// report is recent when an 8-K Item 2.02 was filed in the last `RECENT_REPORT_DAYS`.
    pub async fn read_earnings(
        &self,
        stocks: &[String],
        local: Option<&str>,
    ) -> Result<usize, String> {
        let cutoff = (Utc::now() - Duration::days(RECENT_REPORT_DAYS)).to_rfc3339();
        let reports: Vec<(String, sec::SecFiling)> = {
            let cache = self.news_cache().await?;
            stocks
                .iter()
                .filter_map(|symbol| {
                    let company = cache.company(symbol).ok()?.0?;
                    let filing = latest_report(&company.filings)?.clone();
                    (filing.filed_at >= cutoff).then(|| (symbol.clone(), filing))
                })
                .collect()
        };
        let mut done = 0;
        let mut fetched = 0;
        for (symbol, filing) in &reports {
            // Alpha Vantage lists a quarter once it's reported; one saved since the filing has it.
            let saved = self.news_cache().await?.reported_quarters(symbol)?;
            let has_it = saved.as_ref().is_some_and(|(quarters, _)| {
                quarters.first().is_some_and(|quarter| {
                    filing
                        .filed_at
                        .get(..10)
                        .is_some_and(|day| quarter.reported_date.as_str() >= day)
                })
            });
            let fresh = saved
                .as_ref()
                .is_some_and(|(_, at)| Utc::now().timestamp() - at < 12 * 3_600);
            if !has_it
                && !fresh
                && fetched < RESULTS_PER_RUN
                && self
                    .integration_status()
                    .is_ok_and(|status| status.alpha_vantage)
            {
                fetched += 1;
                if self.fetch_reported_quarters(symbol).await.is_ok() {
                    done += 1;
                }
            }
        }
        let Some(contact) = self.sec_contact()? else {
            return Ok(done);
        };
        let mut read = 0;
        for (symbol, filing) in reports {
            if read == RELEASES_PER_RUN {
                break;
            }
            let Some(accession) = sec::accession(&filing).map(str::to_owned) else {
                continue;
            };
            let text = match self.news_cache().await?.release(&accession)? {
                Some((_, text)) => text,
                None => {
                    let Some(index) = index_link(&filing.url) else {
                        continue;
                    };
                    let Ok(page) = self.sec_text(&contact, &index, 2 * 1024 * 1024).await else {
                        continue;
                    };
                    let Some(exhibit) = exhibit_link(&page) else {
                        continue;
                    };
                    let Ok(html) = self.sec_text(&contact, &exhibit, 8 * 1024 * 1024).await else {
                        continue;
                    };
                    let text: String = news::plain_text(&html)
                        .chars()
                        .take(MAX_RELEASE_CHARS)
                        .collect();
                    self.news_cache().await?.save_release(
                        &accession,
                        &symbol,
                        &exhibit,
                        &text,
                        Utc::now().timestamp(),
                    )?;
                    text
                }
            };
            let key = guidance_key(&accession);
            if self.news_cache().await?.reading(&key)?.is_some() {
                continue;
            }
            let candidates = guidance_candidates(&reading::sentences(&text));
            // With no outlook words about a future period, the release gives no guidance.
            let (guidance, reader) = if candidates.is_empty() {
                (Vec::new(), reading::RULE.to_owned())
            } else {
                // A local model reads every candidate; Apple's, as many as fit.
                let limit = if local.is_some() {
                    reading::LOCAL_CHARS
                } else {
                    reading::APPLE_CHARS
                };
                let input = reading::numbered("Release sentences:", &candidates, limit);
                match self.read_text(reading::Task::Guidance, input, local).await {
                    Ok((reply, reader)) => (reading::listed(&reply, &candidates), reader),
                    // A model that can't read it now leaves it for the next run.
                    Err(_) => continue,
                }
            };
            self.news_cache().await?.save_reading(
                &key,
                &json!({ "sentences": guidance }),
                &reader,
                Utc::now().timestamp(),
            )?;
            read += 1;
        }
        Ok(done + read)
    }
}

/// What the summary knows about a holding's latest report, from saved data.
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct SavedReport {
    pub filing: Option<sec::SecFiling>,
    pub quarter: Option<ReportedQuarter>,
    pub release: Option<ReleaseReading>,
    /// The move the options market priced in before the report, in percent.
    pub expected_move_pct: Option<f64>,
    /// What management said about the outlook on the call, with who chose the sentences.
    pub call_guidance: Option<(Vec<String>, String)>,
}

/// A holding's latest report as saved: the 8-K, the matching reported quarter (reported on or
/// after the filing's day, or the newest when there's no filing), and the release's reading.
pub(crate) fn saved_report(cache: &news_cache::NewsCache, symbol: &str) -> SavedReport {
    let filing = cache
        .company(symbol)
        .ok()
        .and_then(|(company, _)| company)
        .and_then(|company| latest_report(&company.filings).cloned());
    let quarters = cache
        .reported_quarters(symbol)
        .ok()
        .flatten()
        .map(|(quarters, _)| quarters)
        .unwrap_or_default();
    let quarter = match &filing {
        Some(filing) => {
            let day = filing.filed_at.get(..10).unwrap_or("");
            quarters
                .iter()
                .find(|quarter| {
                    NaiveDate::parse_from_str(&quarter.reported_date, "%Y-%m-%d")
                        .ok()
                        .zip(NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
                        .is_some_and(|(reported, filed)| (reported - filed).num_days().abs() <= 3)
                })
                .cloned()
        }
        None => quarters.first().cloned(),
    };
    let release = filing.as_ref().and_then(|filing| {
        let accession = sec::accession(filing)?;
        let (url, text) = cache.release(accession).ok()??;
        let sentences = reading::sentences(&text);
        let (guidance, guidance_by) = match cache.reading(&guidance_key(accession)).ok()? {
            Some((reading, reader)) => (
                reading["sentences"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
                reader,
            ),
            // Not read yet: no guidance is claimed.
            None => (Vec::new(), reading::RULE.to_owned()),
        };
        Some(ReleaseReading {
            url,
            revenue: revenue_sentence(&sentences),
            guidance,
            guidance_by,
        })
    });
    let report_day = filing
        .as_ref()
        .and_then(|filing| crate::session::date_of(&filing.filed_at))
        .map(|day| day.to_string())
        .or_else(|| {
            quarter
                .as_ref()
                .map(|quarter| quarter.reported_date.clone())
        });
    let expected_move_pct = report_day
        .as_deref()
        .and_then(|day| expectations::saved_expected_move(cache, symbol, day));
    let call_guidance = quarter.as_ref().and_then(|quarter| {
        expectations::saved_call_guidance(cache, symbol, &quarter.fiscal_date_ending)
    });
    SavedReport {
        filing,
        quarter,
        release,
        expected_move_pct,
        call_guidance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reported_quarters_keep_numbers_and_drop_what_isnt_one() {
        let quarters = parse_reported_quarters(&json!({
            "symbol": "TEST",
            "quarterlyEarnings": [
                { "fiscalDateEnding": "2026-09-30", "reportedDate": "2026-10-21", "reportedEPS": "2.95",
                  "estimatedEPS": "2.88", "surprise": "0.07", "surprisePercentage": "2.4306", "reportTime": "post-market" },
                { "fiscalDateEnding": "2026-06-30", "reportedDate": "2026-07-22", "reportedEPS": "None",
                  "estimatedEPS": "2.10", "surprisePercentage": "None", "reportTime": "sometime" },
                { "fiscalDateEnding": "2026-03-31", "reportedDate": "not a date" }
            ]
        }))
        .unwrap();
        assert_eq!(quarters.len(), 2);
        assert_eq!(quarters[0].reported_eps, Some(2.95));
        assert_eq!(quarters[0].estimated_eps, Some(2.88));
        assert_eq!(quarters[0].report_time.as_deref(), Some("post-market"));
        assert_eq!(quarters[1].reported_eps, None);
        assert_eq!(quarters[1].report_time, None);
        assert!(parse_reported_quarters(&json!({ "Information": "limit" })).is_err());
    }

    #[test]
    fn the_release_is_found_by_its_exhibit_type_on_the_index_page() {
        assert_eq!(
            index_link("https://www.sec.gov/Archives/edgar/data/1234/000123456726000042/test-8k.htm").as_deref(),
            Some("https://www.sec.gov/Archives/edgar/data/1234/000123456726000042/0001234567-26-000042-index.html")
        );
        assert_eq!(index_link("https://example.com/0001234567"), None);
        let page = r#"<table><tr><td>1</td><td><a href="/Archives/edgar/data/1234/000123456726000042/test-8k.htm">test-8k.htm</a></td><td>8-K</td></tr>
            <tr><td>2</td><td><a href="/Archives/edgar/data/1234/000123456726000042/q3pr.htm">q3pr.htm</a></td><td>EX-99.1</td></tr></table>"#;
        assert_eq!(
            exhibit_link(page).as_deref(),
            Some("https://www.sec.gov/Archives/edgar/data/1234/000123456726000042/q3pr.htm")
        );
        assert_eq!(exhibit_link("<tr><td>EX-99.2</td></tr>"), None);
    }

    #[test]
    fn revenue_and_guidance_are_quoted_from_the_release() {
        let sentences = reading::sentences(
            "Test Corp reported results for its third quarter. \
             Revenue was $1.24 billion, up 18% from a year ago. \
             Net income rose on lower costs in the quarter. \
             For the fourth quarter, the company expects revenue between $1.30 billion and $1.35 billion. \
             The company raised its full year outlook for operating margin. \
             Last year the company expected growth in the prior period too, as it said then.",
        );
        assert_eq!(
            revenue_sentence(&sentences).as_deref(),
            Some("Revenue was $1.24 billion, up 18% from a year ago.")
        );
        assert_eq!(
            guidance_candidates(&sentences),
            [
                "For the fourth quarter, the company expects revenue between $1.30 billion and $1.35 billion.",
                "The company raised its full year outlook for operating margin.",
            ]
        );
    }
}

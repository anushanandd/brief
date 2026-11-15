//! Earnings, read: what a company reported against what analysts expected (Alpha Vantage's
//! reported quarters, within its free daily budget, which say when each report was), and what
//! its press release says (the Exhibit 99.1 of an 8-K reporting Item 2.02, or of a foreign
//! issuer's 6-K filed with the report, from EDGAR). Brief quotes the release's own sentences
//! for revenue and guidance; it never computes a figure from release text.
use super::*;
use serde_json::json;
use std::sync::LazyLock;

/// A release's text is cut here; past it are tables a sentence reader can't use.
const MAX_RELEASE_CHARS: usize = 120_000;
/// A report this recent has its release read and its result fetched.
const RECENT_REPORT_DAYS: i64 = 10;
/// Days either side of a reported date that its release may be filed.
const RELEASE_DAYS: i64 = 3;
/// A company reports once a quarter, so a 6-K this soon after its last report isn't its next.
const NEXT_REPORT_DAYS: i64 = 60;
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

/// An 8-K reporting Item 2.02: an earnings release, said so by the filer.
fn is_report(filing: &sec::SecFiling) -> bool {
    filing.form.starts_with("8-K") && filing.items.iter().any(|item| item.code == "2.02")
}

/// A filing that can carry an earnings release: an 8-K reporting Item 2.02, or a foreign
/// issuer's 6-K, whose contents aren't coded.
fn may_report(filing: &sec::SecFiling) -> bool {
    is_report(filing) || filing.form.starts_with("6-K")
}

fn filed_day(filing: &sec::SecFiling) -> Option<NaiveDate> {
    crate::session::date_of(&filing.filed_at)
}

fn reported_day(quarter: &ReportedQuarter) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&quarter.reported_date, "%Y-%m-%d").ok()
}

/// The latest earnings report in a company's saved filings: an 8-K reporting Item 2.02.
pub(crate) fn latest_report(filings: &[sec::SecFiling]) -> Option<&sec::SecFiling> {
    filings
        .iter()
        .filter(|filing| is_report(filing))
        .max_by(|a, b| a.filed_at.cmp(&b.filed_at))
}

/// The day of a company's latest report: its newest reported quarter, or an 8-K reporting
/// Item 2.02 filed after it, whose results aren't listed yet.
fn latest_report_day(
    filings: &[sec::SecFiling],
    quarters: &[ReportedQuarter],
) -> Option<NaiveDate> {
    let filed = latest_report(filings).and_then(filed_day);
    let listed = quarters.first().and_then(reported_day);
    filed.max(listed)
}

/// The filings that may carry the release of a report on `day`, within `RELEASE_DAYS`: 8-Ks
/// reporting Item 2.02 first, then 6-Ks, each oldest first.
fn release_candidates(filings: &[sec::SecFiling], day: NaiveDate) -> Vec<&sec::SecFiling> {
    let mut found: Vec<&sec::SecFiling> = filings
        .iter()
        .filter(|filing| may_report(filing))
        .filter(|filing| {
            filed_day(filing).is_some_and(|filed| (filed - day).num_days().abs() <= RELEASE_DAYS)
        })
        .collect();
    found.sort_by(|a, b| (!is_report(a), &a.filed_at).cmp(&(!is_report(b), &b.filed_at)));
    found
}

/// Whether to ask Alpha Vantage for a company's reported quarters: never asked, or, at most
/// twice a day, a recent filing that may carry a report newer than the newest listed. A 6-K
/// counts only once a quarter has passed since that report, since a company reports quarterly
/// and a foreign issuer files 6-Ks for much else.
fn results_due(
    filings: &[sec::SecFiling],
    saved: Option<&(Vec<ReportedQuarter>, i64)>,
    now: DateTime<Utc>,
) -> bool {
    let Some((quarters, at)) = saved else {
        return true;
    };
    if now.timestamp() - at < 12 * 3_600 {
        return false;
    }
    let newest = quarters.first().and_then(reported_day);
    let cutoff = now.date_naive() - Duration::days(RECENT_REPORT_DAYS);
    filings
        .iter()
        .filter(|filing| may_report(filing))
        .any(|filing| {
            filed_day(filing).is_some_and(|day| {
                day >= cutoff
                    && newest.is_none_or(|newest| {
                        day > newest + Duration::days(RELEASE_DAYS)
                            && (is_report(filing)
                                || day >= newest + Duration::days(NEXT_REPORT_DAYS))
                    })
            })
        })
}

/// Whether a saved release reads as an earnings release: it states revenue.
fn reads_as_results(text: &str) -> bool {
    revenue_sentence(&reading::sentences(text)).is_some()
}

/// A company's latest report and the filing that carried its release. An 8-K reporting Item
/// 2.02 newer than the listed results is a report whose results aren't listed yet. Otherwise the
/// newest listed quarter is the report, and its release is an 8-K reporting Item 2.02 filed with
/// it, or a 6-K filed with it whose saved exhibit (`release`) reads as results.
fn report_and_release(
    filings: &[sec::SecFiling],
    quarters: &[ReportedQuarter],
    release: impl Fn(&str) -> Option<String>,
) -> (Option<ReportedQuarter>, Option<sec::SecFiling>) {
    let filed = latest_report(filings);
    let newest = quarters.first();
    let listed = newest.and_then(reported_day);
    let unlisted = filed
        .and_then(filed_day)
        .is_some_and(|day| listed.is_none_or(|listed| day > listed + Duration::days(RELEASE_DAYS)));
    if unlisted {
        return (None, filed.cloned());
    }
    let Some(listed) = listed else {
        return (None, None);
    };
    let carried = release_candidates(filings, listed)
        .into_iter()
        .find(|filing| {
            is_report(filing)
                || sec::accession(filing)
                    .and_then(&release)
                    .is_some_and(|text| reads_as_results(&text))
        })
        .cloned();
    (newest.cloned(), carried)
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

    /// Fetches each stock's reported quarters when they may have changed (`results_due`), a few
    /// per run, then reads the releases of reports from the last `RECENT_REPORT_DAYS`. Alpha
    /// Vantage's quarters say when a company reported, whatever form it filed; without them,
    /// only an 8-K reporting Item 2.02 says so.
    pub async fn read_earnings(&self, stocks: &[String]) -> Result<usize, String> {
        let now = Utc::now();
        let companies: Vec<(String, Vec<sec::SecFiling>)> = {
            let cache = self.news_cache().await?;
            stocks
                .iter()
                .map(|symbol| {
                    let filings = cache
                        .company(symbol)
                        .ok()
                        .and_then(|(company, _)| company)
                        .map(|company| company.filings)
                        .unwrap_or_default();
                    (symbol.clone(), filings)
                })
                .collect()
        };
        let mut done = 0;
        if self
            .integration_status()
            .is_ok_and(|status| status.alpha_vantage)
        {
            let mut fetched = 0;
            for (symbol, filings) in &companies {
                if fetched == RESULTS_PER_RUN {
                    break;
                }
                let saved = self.news_cache().await?.reported_quarters(symbol)?;
                if results_due(filings, saved.as_ref(), now) {
                    fetched += 1;
                    if self.fetch_reported_quarters(symbol).await.is_ok() {
                        done += 1;
                    }
                }
            }
        }
        let cutoff = now.date_naive() - Duration::days(RECENT_REPORT_DAYS);
        let reports: Vec<(String, sec::SecFiling)> = {
            let cache = self.news_cache().await?;
            companies
                .iter()
                .flat_map(|(symbol, filings)| {
                    let quarters = cache
                        .reported_quarters(symbol)
                        .ok()
                        .flatten()
                        .map(|(quarters, _)| quarters)
                        .unwrap_or_default();
                    latest_report_day(filings, &quarters)
                        .filter(|day| *day >= cutoff)
                        .map(|day| release_candidates(filings, day))
                        .unwrap_or_default()
                        .into_iter()
                        .map(|filing| (symbol.clone(), filing.clone()))
                        .collect::<Vec<_>>()
                })
                .collect()
        };
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
            // Looked up on its own line: a cache guard left in the match would be held across
            // the SEC requests below and deadlock on the save.
            let saved = self.news_cache().await?.release(&accession)?;
            let text = match saved {
                Some((_, text)) => text,
                None => {
                    let Some(index) = index_link(&filing.url) else {
                        continue;
                    };
                    let Ok(page) = self.sec_text(&contact, &index, 2 * 1024 * 1024).await else {
                        continue;
                    };
                    let Some(exhibit) = exhibit_link(&page) else {
                        // A filing without an Exhibit 99.1 has no release; it's noted as read.
                        self.news_cache().await?.save_release(
                            &accession,
                            &symbol,
                            &index,
                            "",
                            Utc::now().timestamp(),
                        )?;
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
            // A 6-K that isn't a results release has no guidance to read.
            if !is_report(&filing) && !reads_as_results(&text) {
                continue;
            }
            let key = guidance_key(&accession);
            if self.news_cache().await?.reading(&key)?.is_some() {
                continue;
            }
            let candidates = guidance_candidates(&reading::sentences(&text));
            // With no outlook words about a future period, the release gives no guidance.
            let (guidance, reader) = if candidates.is_empty() {
                (Vec::new(), reading::RULE.to_owned())
            } else {
                // Apple's model reads the candidates a window at a time.
                let limit = reading::APPLE_CHARS;
                let mut picked = Vec::new();
                let mut reader = reading::RULE.to_owned();
                let mut unread = false;
                for chunk in reading::chunks("Release sentences:", &candidates, limit) {
                    let input = reading::numbered("Release sentences:", &chunk, limit);
                    match self.read_text(reading::Task::Guidance, input).await {
                        Ok((reply, by)) => {
                            picked.extend(reading::listed(&reply, &chunk));
                            reader = by;
                        }
                        Err(_) => {
                            unread = true;
                            break;
                        }
                    }
                }
                // A model that can't read it all now leaves it for the next run.
                if unread {
                    continue;
                }
                picked.truncate(4);
                (picked, reader)
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
}

/// A holding's latest report as saved: the 8-K, the matching reported quarter (reported on or
/// after the filing's day, or the newest when there's no filing), and the release's reading.
pub(crate) fn saved_report(cache: &news_cache::NewsCache, symbol: &str) -> SavedReport {
    let filings = cache
        .company(symbol)
        .ok()
        .and_then(|(company, _)| company)
        .map(|company| company.filings)
        .unwrap_or_default();
    let quarters = cache
        .reported_quarters(symbol)
        .ok()
        .flatten()
        .map(|(quarters, _)| quarters)
        .unwrap_or_default();
    let (quarter, filing) = report_and_release(&filings, &quarters, |accession| {
        cache
            .release(accession)
            .ok()
            .flatten()
            .map(|(_, text)| text)
    });
    let release = filing.as_ref().and_then(|filing| {
        let accession = sec::accession(filing)?;
        let (url, text) = cache
            .release(accession)
            .ok()?
            .filter(|(_, text)| !text.is_empty())?;
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
    SavedReport {
        filing,
        quarter,
        release,
        expected_move_pct,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filing(form: &str, item: Option<&str>, at: &str, accession: &str) -> sec::SecFiling {
        sec::SecFiling {
            form: form.into(),
            filed_at: at.into(),
            url: format!("https://www.sec.gov/Archives/edgar/data/1/{accession}/doc.htm"),
            items: item
                .map(|code| sec::SecItem {
                    code: code.into(),
                    label: String::new(),
                })
                .into_iter()
                .collect(),
            title: String::new(),
        }
    }

    fn quarter(reported: &str) -> ReportedQuarter {
        ReportedQuarter {
            fiscal_date_ending: "2026-09-30".into(),
            reported_date: reported.into(),
            reported_eps: Some(0.3),
            estimated_eps: Some(-0.7),
            surprise_pct: None,
            report_time: Some("pre-market".into()),
        }
    }

    #[test]
    fn a_foreign_issuers_report_is_found_from_its_results_and_its_6k_release() {
        let filings = vec![
            filing(
                "6-K",
                None,
                "2026-11-12T14:22:00+00:00",
                "000000000126000001",
            ),
            filing(
                "6-K",
                None,
                "2026-11-12T21:10:00+00:00",
                "000000000126000002",
            ),
            filing(
                "6-K",
                None,
                "2026-10-22T12:00:00+00:00",
                "000000000126000003",
            ),
        ];
        let quarters = [quarter("2026-11-12"), quarter("2026-08-07")];
        // Of the two 6-Ks filed with the report, the one whose exhibit states revenue carried it.
        let release = |accession: &str| {
            (accession == "000000000126000002")
                .then(|| "Revenue was $146 million, up 355% from a year ago.".to_owned())
        };
        let (found, carried) = report_and_release(&filings, &quarters, release);
        assert_eq!(found.map(|q| q.reported_date), Some("2026-11-12".into()));
        assert_eq!(
            carried.as_ref().and_then(sec::accession),
            Some("000000000126000002")
        );
        // Not yet read, no 6-K is claimed as the release.
        let (_, unread) = report_and_release(&filings, &quarters, |_| None);
        assert!(unread.is_none());
    }

    #[test]
    fn an_8k_newer_than_the_listed_results_is_a_report_whose_results_are_pending() {
        let filings = vec![filing(
            "8-K",
            Some("2.02"),
            "2026-11-12T20:05:00+00:00",
            "000000000126000004",
        )];
        let (found, carried) = report_and_release(&filings, &[quarter("2026-08-07")], |_| None);
        assert!(found.is_none());
        assert!(carried.is_some());
        // Once listed, the quarter and the 8-K filed with it are one report.
        let (found, carried) = report_and_release(&filings, &[quarter("2026-11-12")], |_| None);
        assert!(found.is_some() && carried.is_some());
    }

    #[test]
    fn results_are_asked_for_once_then_after_a_filing_that_may_carry_a_new_report() {
        let now: DateTime<Utc> = "2026-11-13T12:00:00Z".parse().unwrap();
        let day_ago = now.timestamp() - 86_400;
        let saved = (vec![quarter("2026-08-07")], day_ago);
        let six_k = |at: &str| vec![filing("6-K", None, at, "000000000126000005")];
        // Never asked: ask, whatever was filed.
        assert!(results_due(&[], None, now));
        // A 6-K a quarter after the last report may be the next one.
        assert!(results_due(
            &six_k("2026-11-12T14:00:00+00:00"),
            Some(&saved),
            now
        ));
        // A 6-K weeks after a report is something else.
        let recent = (vec![quarter("2026-10-20")], day_ago);
        assert!(!results_due(
            &six_k("2026-11-12T14:00:00+00:00"),
            Some(&recent),
            now
        ));
        // Asked within half a day: wait.
        let fresh = (vec![quarter("2026-08-07")], now.timestamp() - 3_600);
        assert!(!results_due(
            &six_k("2026-11-12T14:00:00+00:00"),
            Some(&fresh),
            now
        ));
        // Once the report is listed, nothing is due.
        let listed = (vec![quarter("2026-11-12")], day_ago);
        assert!(!results_due(
            &six_k("2026-11-12T14:00:00+00:00"),
            Some(&listed),
            now
        ));
    }

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

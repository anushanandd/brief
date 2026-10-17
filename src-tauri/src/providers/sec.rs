//! SEC EDGAR: company identity, industry, and current reports (8-K, or 6-K from foreign issuers). Free and keyless, but the
//! SEC requires a contact in every request's User-Agent and allows 10 requests a second.
use super::*;

const TICKERS_URL: &str = "https://www.sec.gov/files/company_tickers.json";
/// How long filings are shown after they are accepted.
pub(super) const FILING_DAYS: i64 = 30;
const MAX_SAVED_FILINGS: usize = 40;
const MAX_INSIDER_FILINGS: usize = 24;

/// A company as EDGAR knows it, with its recent current reports.
#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecCompany {
    pub cik: u64,
    pub name: String,
    pub sic: Option<u32>,
    pub filings: Vec<SecFiling>,
    /// Recent Form 4s (insider trades), newest first.
    #[serde(default)]
    pub insider_filings: Vec<super::insiders::InsiderFiling>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecFiling {
    pub form: String,
    pub filed_at: String,
    pub url: String,
    pub items: Vec<SecItem>,
    /// What the filing reports, recomputed from the form and items whenever filings are read.
    #[serde(default)]
    pub title: String,
}

/// A 6-K or a periodic report carries no item codes, so it can only be named as a whole.
pub(crate) fn filing_title(filing: &SecFiling) -> String {
    if filing.form.starts_with("10-Q") {
        "Quarterly report".into()
    } else if filing.form.starts_with("10-K") {
        "Annual report".into()
    } else if filing.items.is_empty() {
        "Report from a foreign issuer".into()
    } else {
        filing
            .items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecItem {
    pub code: String,
    pub label: String,
}

pub(super) fn sec_contact_email(value: &str) -> Result<String, String> {
    let email = value.trim();
    let valid = email.len() <= 254
        && email.split_once('@').is_some_and(|(name, domain)| {
            !name.is_empty() && domain.contains('.') && !domain.starts_with('.')
        })
        && !email.chars().any(|c| c.is_whitespace() || c.is_control());
    if valid {
        Ok(email.into())
    } else {
        Err("Enter a contact email for SEC requests".into())
    }
}

impl Providers {
    pub(super) fn sec_contact(&self) -> Result<Option<SecContact>, String> {
        self.read_secret(SEC_CONTACT_KEY)
    }

    /// Requests `url` from EDGAR, whose terms require the user's contact in every request.
    pub(super) async fn sec_request(
        &self,
        contact: &SecContact,
        url: &str,
    ) -> Result<reqwest::Response, String> {
        self.http
            .get(url)
            .header(
                reqwest::header::USER_AGENT,
                format!("Brief personal finance {}", contact.email),
            )
            .timeout(StdDuration::from_secs(20))
            .send()
            .await
            .map_err(|_| "Could not reach SEC EDGAR".to_string())
    }

    async fn sec_get(
        &self,
        contact: &SecContact,
        url: &str,
        endpoint: &str,
    ) -> Result<Value, String> {
        let response = self.sec_request(contact, url).await?;
        provider_response("SEC EDGAR", endpoint, response).await
    }

    /// A document from EDGAR's archive as text, such as a Form 4's XML.
    pub(super) async fn sec_text(
        &self,
        contact: &SecContact,
        url: &str,
        limit: usize,
    ) -> Result<String, String> {
        let response = self.sec_request(contact, url).await?;
        if !response.status().is_success() {
            return Err(format!("SEC EDGAR returned {}", response.status().as_u16()));
        }
        String::from_utf8(http::response_bytes(response, limit).await?)
            .map_err(|_| "SEC EDGAR returned an unreadable document".into())
    }

    /// Loads EDGAR's ticker-to-company map into the news cache at most weekly.
    pub(super) async fn sec_ticker_map(
        &self,
        contact: &SecContact,
        force: bool,
    ) -> Result<(), String> {
        let now = Utc::now().timestamp();
        if !force && self.news_cache().await?.sec_tickers_fresh(now)? {
            return Ok(());
        }
        let payload = self
            .sec_get(contact, TICKERS_URL, "/files/company_tickers.json")
            .await?;
        let tickers = parse_sec_tickers(&payload)?;
        self.news_cache().await?.save_sec_tickers(&tickers, now)
    }

    /// Fetches a company's identity and recent 8-Ks. `Ok(None)` means EDGAR lists no company
    /// for the ticker, as for most funds.
    pub(super) async fn sec_company(
        &self,
        contact: &SecContact,
        symbol: &str,
    ) -> Result<Option<SecCompany>, String> {
        self.sec_ticker_map(contact, false).await?;
        let cik = { self.news_cache().await?.sec_cik(symbol)? };
        let Some(cik) = cik else {
            return Ok(None);
        };
        let payload = self
            .sec_get(
                contact,
                &format!("https://data.sec.gov/submissions/CIK{cik:010}.json"),
                "/submissions",
            )
            .await?;
        parse_sec_company(&payload, cik).map(Some)
    }
}

pub(super) fn parse_sec_tickers(payload: &Value) -> Result<Vec<(String, u64)>, String> {
    let rows = payload
        .as_object()
        .ok_or("SEC EDGAR returned no ticker list")?;
    let tickers: Vec<_> = rows
        .values()
        .filter_map(|row| {
            let ticker = row["ticker"].as_str()?.trim().to_ascii_uppercase();
            let cik = row["cik_str"].as_u64()?;
            (!ticker.is_empty()).then_some((ticker, cik))
        })
        .collect();
    if tickers.is_empty() {
        return Err("SEC EDGAR returned no ticker list".into());
    }
    Ok(tickers)
}

/// A filing's accession number, from its EDGAR address: the key that never changes.
pub(crate) fn accession(filing: &SecFiling) -> Option<&str> {
    filing
        .url
        .strip_prefix("https://www.sec.gov/Archives/edgar/data/")?
        .split('/')
        .nth(1)
        .filter(|accession| !accession.is_empty())
}

/// EDGAR writes share classes with a hyphen (BRK-B); brokers often use a dot.
pub(super) fn sec_ticker(symbol: &str) -> String {
    symbol.replace('.', "-")
}

pub(super) fn parse_sec_company(payload: &Value, cik: u64) -> Result<SecCompany, String> {
    let recent = payload
        .pointer("/filings/recent")
        .ok_or("SEC EDGAR returned no filings")?;
    let column = |name: &str| recent[name].as_array().cloned().unwrap_or_default();
    let (forms, accessions, accepted, items, documents) = (
        column("form"),
        column("accessionNumber"),
        column("acceptanceDateTime"),
        column("items"),
        column("primaryDocument"),
    );
    let mut filings = Vec::new();
    let mut insider_filings = Vec::new();
    for (index, form) in forms.iter().enumerate() {
        if form.as_str() == Some("4") && insider_filings.len() < MAX_INSIDER_FILINGS {
            if let (Some(accession), Some(filed_at), Some(document)) = (
                accessions.get(index).and_then(Value::as_str),
                accepted
                    .get(index)
                    .and_then(Value::as_str)
                    .and_then(|value| DateTime::parse_from_rfc3339(value).ok()),
                documents.get(index).and_then(Value::as_str),
            ) {
                if accession.chars().all(|c| c.is_ascii_digit() || c == '-') {
                    insider_filings.extend(super::insiders::insider_filing(
                        &format!(
                            "https://www.sec.gov/Archives/edgar/data/{cik}/{}",
                            accession.replace('-', "")
                        ),
                        &document
                            .split('/')
                            .map(urlencoding_path)
                            .collect::<Vec<_>>()
                            .join("/"),
                        filed_at.with_timezone(&Utc).to_rfc3339(),
                    ));
                }
            }
            continue;
        }
        let Some(form) = form
            .as_str()
            .filter(|form| {
                matches!(
                    *form,
                    "8-K" | "8-K/A" | "6-K" | "6-K/A" | "10-Q" | "10-Q/A" | "10-K" | "10-K/A"
                )
            })
            .filter(|_| filings.len() < MAX_SAVED_FILINGS)
        else {
            continue;
        };
        let (Some(accession), Some(filed_at), Some(document)) = (
            accessions.get(index).and_then(Value::as_str),
            accepted
                .get(index)
                .and_then(Value::as_str)
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok()),
            documents.get(index).and_then(Value::as_str),
        ) else {
            continue;
        };
        if document.is_empty() || !accession.chars().all(|c| c.is_ascii_digit() || c == '-') {
            continue;
        }
        let codes = items
            .get(index)
            .and_then(Value::as_str)
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter_map(|code| {
                item_label(code).map(|label| SecItem {
                    code: code.into(),
                    label: label.into(),
                })
            })
            .collect::<Vec<_>>();
        // An 8-K with only exhibits or housekeeping items says nothing to show.
        if codes.is_empty() && form.starts_with("8-K") {
            continue;
        }
        filings.push(SecFiling {
            form: form.into(),
            filed_at: filed_at.with_timezone(&Utc).to_rfc3339(),
            url: format!(
                "https://www.sec.gov/Archives/edgar/data/{cik}/{}/{}",
                accession.replace('-', ""),
                urlencoding_path(document)
            ),
            items: codes,
            title: String::new(),
        });
    }
    Ok(SecCompany {
        cik,
        name: payload["name"].as_str().unwrap_or("").trim().into(),
        sic: payload["sic"]
            .as_str()
            .and_then(|value| value.parse().ok())
            .or_else(|| {
                payload["sic"]
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
            }),
        filings,
        insider_filings,
    })
}

fn urlencoding_path(document: &str) -> String {
    document
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect()
}

/// What an 8-K reports. Exhibits-only (9.01) and housekeeping items are left out.
pub(super) fn item_label(code: &str) -> Option<&'static str> {
    Some(match code {
        "1.01" => "Material agreement",
        "1.02" => "Agreement ended",
        "1.03" => "Bankruptcy",
        "1.05" => "Cybersecurity incident",
        "2.01" => "Acquisition or sale completed",
        "2.02" => "Earnings results",
        "2.03" => "New debt",
        "2.04" => "Debt accelerated",
        "2.05" => "Restructuring",
        "2.06" => "Impairment",
        "3.01" => "Listing notice",
        "3.02" => "Unregistered share sale",
        "3.03" => "Shareholder rights changed",
        "4.01" => "Auditor change",
        "4.02" => "Restatement",
        "5.01" => "Change in control",
        "5.02" => "Executive or board change",
        "5.07" => "Shareholder vote",
        "7.01" => "Investor disclosure",
        "8.01" => "Other event",
        _ => return None,
    })
}

/// A sector ETF for an SEC industry code. An approximation: EDGAR's codes predate today's
/// sector definitions (search and social companies file as software, for example).
pub(crate) fn sector_for_sic(sic: u32) -> Option<(&'static str, &'static str)> {
    const TECHNOLOGY: (&str, &str) = ("XLK", "Technology");
    const HEALTH: (&str, &str) = ("XLV", "Health care");
    const FINANCIALS: (&str, &str) = ("XLF", "Financials");
    const ENERGY: (&str, &str) = ("XLE", "Energy");
    const INDUSTRIALS: (&str, &str) = ("XLI", "Industrials");
    const DISCRETIONARY: (&str, &str) = ("XLY", "Consumer discretionary");
    const STAPLES: (&str, &str) = ("XLP", "Consumer staples");
    const UTILITIES: (&str, &str) = ("XLU", "Utilities");
    const MATERIALS: (&str, &str) = ("XLB", "Materials");
    const REAL_ESTATE: (&str, &str) = ("XLRE", "Real estate");
    const COMMUNICATION: (&str, &str) = ("XLC", "Communication services");
    Some(match sic {
        1311 | 1381..=1389 | 2900..=2999 => ENERGY,
        1000..=1499 => MATERIALS,
        1500..=1799 => INDUSTRIALS,
        2000..=2199 | 2840..=2844 => STAPLES,
        2830..=2836 => HEALTH,
        2700..=2799 => COMMUNICATION,
        2200..=2399 | 2500..=2599 => DISCRETIONARY,
        2400..=2499 | 2600..=2699 | 2800..=2899 | 3000..=3399 => MATERIALS,
        3570..=3579 | 3661..=3679 => TECHNOLOGY,
        3630..=3639 | 3711..=3716 | 3751 | 3900..=3999 => DISCRETIONARY,
        3841..=3851 => HEALTH,
        3800..=3899 => TECHNOLOGY,
        3400..=3799 => INDUSTRIALS,
        4800..=4899 => COMMUNICATION,
        4900..=4999 => UTILITIES,
        4000..=4799 => INDUSTRIALS,
        5122 | 5912 => HEALTH,
        5140..=5149 | 5400..=5499 => STAPLES,
        5000..=5199 => INDUSTRIALS,
        5200..=5999 => DISCRETIONARY,
        6500..=6553 | 6798 => REAL_ESTATE,
        6000..=6799 => FINANCIALS,
        7310..=7319 | 7800..=7899 => COMMUNICATION,
        7370..=7379 => TECHNOLOGY,
        8000..=8099 => HEALTH,
        8700..=8799 => INDUSTRIALS,
        7000..=8999 => DISCRETIONARY,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_k_filings_keep_labeled_items_and_safe_links() {
        let payload = json!({
            "name": "Synthetic Corp", "sic": "3674",
            "filings": {"recent": {
                "form": ["8-K", "10-Q", "8-K", "8-K", "4", "4"],
                "accessionNumber": ["0000000001-26-000001", "0000000001-26-000002", "0000000001-26-000003", "0000000001-26-000004", "0000000001-26-000005", "0000000001-26-000006"],
                "acceptanceDateTime": ["2026-10-01T20:05:00.000Z", "2026-09-30T20:00:00.000Z", "2026-09-20T12:00:00.000Z", "not a date", "2026-09-18T21:00:00.000Z", "2026-09-17T21:00:00.000Z"],
                "items": ["2.02,9.01", "", "9.01", "5.02", "", ""],
                "primaryDocument": ["synthetic-8k.htm", "q.htm", "exhibits.htm", "late.htm", "xslF345X05/form4.xml", "../escape.htm"]
            }}
        });
        let company = parse_sec_company(&payload, 1).unwrap();
        assert_eq!(company.name, "Synthetic Corp");
        assert_eq!(filing_title(&company.filings[0]), "Earnings results");
        assert_eq!(company.sic, Some(3674));
        // The quarterly report is kept to complete its earnings chain; it has no items.
        assert_eq!(company.filings.len(), 2);
        assert_eq!(filing_title(&company.filings[1]), "Quarterly report");
        assert_eq!(accession(&company.filings[1]), Some("000000000126000002"));
        let filing = &company.filings[0];
        assert_eq!(filing.items.len(), 1);
        assert_eq!(filing.items[0].label, "Earnings results");
        assert_eq!(filing.filed_at, "2026-10-01T20:05:00+00:00");
        assert_eq!(
            filing.url,
            "https://www.sec.gov/Archives/edgar/data/1/000000000126000001/synthetic-8k.htm"
        );
        // Form 4s are kept apart, pointing at their XML; anything but XML is skipped.
        assert_eq!(company.insider_filings.len(), 1);
        assert_eq!(
            company.insider_filings[0].xml,
            "https://www.sec.gov/Archives/edgar/data/1/000000000126000005/form4.xml"
        );
        assert!(company.insider_filings[0]
            .page
            .ends_with("/xslF345X05/form4.xml"));
        assert!(parse_sec_company(&json!({}), 1).is_err());
        let foreign = parse_sec_company(
            &json!({"name": "Synthetic N.V.", "filings": {"recent": {
                "form": ["6-K", "20-F"], "accessionNumber": ["0000000002-26-000001", "0000000002-26-000002"],
                "acceptanceDateTime": ["2026-10-01T06:00:00.000Z", "2026-04-01T06:00:00.000Z"],
                "items": ["", ""], "primaryDocument": ["report.htm", "annual.htm"]
            }}}),
            2,
        )
        .unwrap();
        assert_eq!(foreign.filings.len(), 1);
        assert_eq!(
            filing_title(&foreign.filings[0]),
            "Report from a foreign issuer"
        );
    }

    #[test]
    fn tickers_map_share_classes_and_contacts_are_checked() {
        let tickers = parse_sec_tickers(&json!({
            "0": {"cik_str": 1, "ticker": "test", "title": "Synthetic"},
            "1": {"cik_str": 2, "ticker": "TEST-B", "title": "Synthetic B"},
            "2": {"ticker": "BAD"}
        }))
        .unwrap();
        assert_eq!(tickers, vec![("TEST".into(), 1), ("TEST-B".into(), 2)]);
        assert_eq!(sec_ticker("TEST.B"), "TEST-B");
        assert!(parse_sec_tickers(&json!({})).is_err());
        assert_eq!(
            sec_contact_email(" a@example.com ").unwrap(),
            "a@example.com"
        );
        for invalid in ["", "a@b", "a b@example.com", "@example.com"] {
            assert!(sec_contact_email(invalid).is_err());
        }
    }

    #[test]
    fn industry_codes_map_to_sector_funds() {
        assert_eq!(sector_for_sic(3674).unwrap().0, "XLK");
        assert_eq!(sector_for_sic(2834).unwrap().0, "XLV");
        assert_eq!(sector_for_sic(6022).unwrap().0, "XLF");
        assert_eq!(sector_for_sic(1311).unwrap().0, "XLE");
        assert_eq!(sector_for_sic(6798).unwrap().0, "XLRE");
        assert_eq!(sector_for_sic(5961).unwrap().0, "XLY");
        assert!(sector_for_sic(9999).is_none());
    }
}

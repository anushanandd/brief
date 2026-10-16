//! Insider trades from SEC Form 4: open-market purchases and sales by a company's officers,
//! directors, and largest holders. Purchases are the informative ones; most sales are
//! scheduled (Rule 10b5-1 plans) or pay taxes on vested stock, so each sale says whether it was
//! planned. Filings never change, so each is fetched once and kept.
use super::*;
use std::sync::LazyLock;

/// Form 4s read per holding: a quarter's worth for most companies.
const INSIDER_FILINGS: usize = 12;
const MAX_FORM4_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsiderTrade {
    pub date: String,
    pub filed_at: String,
    pub name: String,
    /// The insider's title, `Director`, or `10% owner`.
    pub role: String,
    /// `buy` (code P) or `sell` (code S).
    pub side: String,
    pub shares: f64,
    pub price: Option<f64>,
    /// Shares times price, when the filing gives a price.
    pub value: Option<f64>,
    /// Made under a Rule 10b5-1 trading plan adopted in advance.
    pub planned: bool,
    pub url: String,
}

/// A Form 4 in a company's submissions: its XML for parsing and its rendered page for people.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsiderFiling {
    pub filed_at: String,
    pub xml: String,
    pub page: String,
}

fn tag(name: &str) -> regex::Regex {
    regex::RegexBuilder::new(&format!(
        r"<{name}>\s*(?:<value>)?\s*([^<]*?)\s*(?:</value>)?\s*</{name}>"
    ))
    .case_insensitive(true)
    .build()
    .expect("valid pattern")
}

static OWNER: LazyLock<regex::Regex> = LazyLock::new(|| tag("rptOwnerName"));
static TITLE: LazyLock<regex::Regex> = LazyLock::new(|| tag("officerTitle"));
static DIRECTOR: LazyLock<regex::Regex> = LazyLock::new(|| tag("isDirector"));
static OWNER_10: LazyLock<regex::Regex> = LazyLock::new(|| tag("isTenPercentOwner"));
static PLAN: LazyLock<regex::Regex> = LazyLock::new(|| tag("aff10b5One"));
static DATE: LazyLock<regex::Regex> = LazyLock::new(|| tag("transactionDate"));
static CODE: LazyLock<regex::Regex> = LazyLock::new(|| tag("transactionCode"));
static SHARES: LazyLock<regex::Regex> = LazyLock::new(|| tag("transactionShares"));
static PRICE: LazyLock<regex::Regex> = LazyLock::new(|| tag("transactionPricePerShare"));
static TRANSACTION: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::RegexBuilder::new(r"<nonDerivativeTransaction>(.*?)</nonDerivativeTransaction>")
        .case_insensitive(true)
        .dot_matches_new_line(true)
        .build()
        .expect("valid pattern")
});
static FOOTNOTES: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::RegexBuilder::new(r"<footnotes>(.*?)</footnotes>")
        .case_insensitive(true)
        .dot_matches_new_line(true)
        .build()
        .expect("valid pattern")
});

fn first<'a>(pattern: &regex::Regex, text: &'a str) -> Option<&'a str> {
    pattern
        .captures(text)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().trim())
        .filter(|value| !value.is_empty())
}

fn decoded(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// EDGAR writes names last name first in capitals ("DOE JANE A"); they read better in title
/// case, in the order filed.
fn title_case(name: &str) -> String {
    name.split_whitespace()
        .map(|word| {
            let lower = word.to_lowercase();
            let mut letters = lower.chars();
            letters
                .next()
                .map(|first| first.to_uppercase().chain(letters).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// Open-market purchases (P) and sales (S) in one Form 4. Grants, option exercises, tax
/// withholding, and gifts aren't decisions to trade, so they are left out.
pub(super) fn parse_form4(xml: &str, filed_at: &str, url: &str) -> Vec<InsiderTrade> {
    let truthy = |pattern: &regex::Regex| {
        first(pattern, xml).is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
    };
    let Some(name) = first(&OWNER, xml).map(|name| title_case(&decoded(name))) else {
        return Vec::new();
    };
    let role = first(&TITLE, xml)
        .map(decoded)
        .or_else(|| truthy(&DIRECTOR).then(|| "Director".to_owned()))
        .or_else(|| truthy(&OWNER_10).then(|| "10% owner".to_owned()))
        .unwrap_or_else(|| "Insider".to_owned());
    // Since 2023 the form has a checkbox; earlier filings say so in a footnote.
    let planned = truthy(&PLAN)
        || first(&FOOTNOTES, xml).is_some_and(|notes| notes.to_lowercase().contains("10b5-1"));
    TRANSACTION
        .captures_iter(xml)
        .filter_map(|captures| {
            let body = captures.get(1)?.as_str();
            let side = match first(&CODE, body)? {
                "P" => "buy",
                "S" => "sell",
                _ => return None,
            };
            let shares: f64 = first(&SHARES, body)?.parse().ok()?;
            if !shares.is_finite() || shares <= 0.0 {
                return None;
            }
            let price = first(&PRICE, body)
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|price| price.is_finite() && *price > 0.0);
            let date = first(&DATE, body)?.get(..10)?.to_owned();
            NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok()?;
            Some(InsiderTrade {
                date,
                filed_at: filed_at.to_owned(),
                name: name.clone(),
                role: role.clone(),
                side: side.to_owned(),
                shares,
                price,
                value: price.map(|price| crate::numeric::round2(shares * price)),
                planned,
                url: url.to_owned(),
            })
        })
        .collect()
}

/// The rendered page's document is the XML under an `xslF345…/` folder.
pub(super) fn insider_filing(
    base: &str,
    document: &str,
    filed_at: String,
) -> Option<InsiderFiling> {
    let xml = document
        .split_once('/')
        .filter(|(folder, _)| folder.starts_with("xslF345"))
        .map_or(document, |(_, file)| file);
    xml.ends_with(".xml").then(|| InsiderFiling {
        filed_at,
        xml: format!("{base}/{xml}"),
        page: format!("{base}/{document}"),
    })
}

impl Providers {
    /// The holding's insider trades filed on or after `since`, newest first, from its most
    /// recent Form 4s. Each filing is fetched once; filings that fail are tried again next time.
    pub async fn insider_trades(
        &self,
        symbol: &str,
        since: NaiveDate,
    ) -> Result<Vec<InsiderTrade>, String> {
        let since = since.to_string();
        let (filings, saved) = {
            let cache = self.news_cache().await?;
            let filings: Vec<InsiderFiling> = cache
                .company(symbol)?
                .0
                .map(|company| company.insider_filings)
                .unwrap_or_default()
                .into_iter()
                .filter(|filing| {
                    crate::session::date_of(&filing.filed_at)
                        .is_some_and(|day| day.to_string() >= since)
                })
                .take(INSIDER_FILINGS)
                .collect();
            let mut saved = BTreeMap::new();
            for filing in &filings {
                if let Some(trades) = cache.insider_trades(&filing.xml)? {
                    saved.insert(filing.xml.clone(), trades);
                }
            }
            (filings, saved)
        };
        let missing: Vec<&InsiderFiling> = filings
            .iter()
            .filter(|filing| !saved.contains_key(&filing.xml))
            .collect();
        let mut fetched = BTreeMap::new();
        if !missing.is_empty() {
            let contact = self
                .sec_contact()?
                .ok_or("Add a contact email for SEC EDGAR in Settings")?;
            for filing in missing {
                if let Ok(xml) = self.sec_text(&contact, &filing.xml, MAX_FORM4_BYTES).await {
                    fetched.insert(
                        filing.xml.clone(),
                        parse_form4(&xml, &filing.filed_at, &filing.page),
                    );
                }
            }
            let cache = self.news_cache().await?;
            for (url, trades) in &fetched {
                cache.save_insider_trades(url, trades)?;
            }
        }
        let mut trades: Vec<InsiderTrade> = saved
            .into_values()
            .chain(fetched.into_values())
            .flatten()
            .filter(|trade| trade.date >= since)
            .collect();
        trades.sort_by(|a, b| {
            b.date
                .cmp(&a.date)
                .then(b.filed_at.cmp(&a.filed_at))
                .then(a.name.cmp(&b.name))
        });
        Ok(trades)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORM4: &str = r#"<?xml version="1.0"?>
<ownershipDocument>
  <reportingOwner>
    <reportingOwnerId><rptOwnerName>DOE JANE A</rptOwnerName></reportingOwnerId>
    <reportingOwnerRelationship>
      <isDirector>0</isDirector><isOfficer>1</isOfficer>
      <officerTitle>Chief Executive Officer</officerTitle>
    </reportingOwnerRelationship>
  </reportingOwner>
  <aff10b5One>1</aff10b5One>
  <nonDerivativeTable>
    <nonDerivativeTransaction>
      <transactionDate><value>2026-09-14</value></transactionDate>
      <transactionCoding><transactionCode>S</transactionCode></transactionCoding>
      <transactionAmounts>
        <transactionShares><value>1200</value></transactionShares>
        <transactionPricePerShare><value>110.5</value></transactionPricePerShare>
        <transactionAcquiredDisposedCode><value>D</value></transactionAcquiredDisposedCode>
      </transactionAmounts>
    </nonDerivativeTransaction>
    <nonDerivativeTransaction>
      <transactionDate><value>2026-09-14</value></transactionDate>
      <transactionCoding><transactionCode>F</transactionCode></transactionCoding>
      <transactionAmounts>
        <transactionShares><value>300</value></transactionShares>
        <transactionPricePerShare><value>110.0</value></transactionPricePerShare>
      </transactionAmounts>
    </nonDerivativeTransaction>
    <nonDerivativeTransaction>
      <transactionDate><value>2026-09-15</value></transactionDate>
      <transactionCoding><transactionCode>P</transactionCode></transactionCoding>
      <transactionAmounts>
        <transactionShares><value>100</value></transactionShares>
        <transactionPricePerShare><footnoteId id="F1"/></transactionPricePerShare>
      </transactionAmounts>
    </nonDerivativeTransaction>
  </nonDerivativeTable>
</ownershipDocument>"#;

    #[test]
    fn form4_keeps_open_market_trades_with_role_and_plan() {
        let trades = parse_form4(
            FORM4,
            "2026-09-16T20:00:00+00:00",
            "https://www.sec.gov/x.html",
        );
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].name, "Doe Jane A");
        assert_eq!(trades[0].role, "Chief Executive Officer");
        assert_eq!(trades[0].side, "sell");
        assert_eq!(trades[0].value, Some(132_600.0));
        assert!(trades[0].planned);
        // A price given only in a footnote leaves the value unknown, never guessed.
        assert_eq!(trades[1].side, "buy");
        assert_eq!(trades[1].price, None);
        assert_eq!(trades[1].value, None);
        assert!(parse_form4("<ownershipDocument/>", "", "").is_empty());
    }

    #[test]
    fn director_without_title_and_footnoted_plan() {
        let xml = FORM4
            .replace("<officerTitle>Chief Executive Officer</officerTitle>", "")
            .replace("<isDirector>0</isDirector>", "<isDirector>true</isDirector>")
            .replace("<aff10b5One>1</aff10b5One>", "")
            .replace(
                "</ownershipDocument>",
                "<footnotes><footnote id=\"F1\">Sold under a Rule 10b5-1 plan.</footnote></footnotes></ownershipDocument>",
            );
        let trades = parse_form4(&xml, "", "");
        assert_eq!(trades[0].role, "Director");
        assert!(trades[0].planned);
    }

    #[test]
    fn rendered_form4_documents_point_at_their_xml() {
        let filing = insider_filing(
            "https://www.sec.gov/Archives/edgar/data/1/000000000026000001",
            "xslF345X05/wk-form4_1.xml",
            "2026-09-16T20:00:00+00:00".into(),
        )
        .unwrap();
        assert!(filing.xml.ends_with("/000000000026000001/wk-form4_1.xml"));
        assert!(filing.page.ends_with("/xslF345X05/wk-form4_1.xml"));
        assert!(insider_filing("https://x", "form4.htm", String::new()).is_none());
    }
}

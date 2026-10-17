//! A company's latest reported quarter from SEC EDGAR's XBRL facts: revenue, its change from the
//! same quarter a year earlier, and net margin. Only figures the company tagged are used; a value
//! EDGAR does not hold is left out, never estimated.
use super::*;
use std::sync::LazyLock;

/// Revenue is tagged under several concepts; the first with a recent quarter is used.
const REVENUE_TAGS: [&str; 4] = [
    "RevenueFromContractWithCustomerExcludingAssessedTax",
    "Revenues",
    "SalesRevenueNet",
    "RevenueFromContractWithCustomerIncludingAssessedTax",
];
const NET_INCOME_TAG: &str = "NetIncomeLoss";
/// A quarter that ended longer ago than this is stale; a later tag may hold the current one.
const RECENT_QUARTER_DAYS: i64 = 200;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Fundamentals {
    /// The calendar quarter EDGAR aligns the figures to, such as `CY2026Q2`.
    pub frame: String,
    /// The last day of the company's quarter.
    pub quarter_end: String,
    pub revenue: f64,
    /// Against the same calendar quarter a year earlier, when both are reported.
    pub revenue_growth_pct: Option<f64>,
    /// Net income over revenue for the same quarter, when net income is reported.
    pub net_margin_pct: Option<f64>,
    /// The report the revenue came from, such as `10-Q`, when it was filed, and its index page.
    pub form: String,
    pub filed: String,
    pub url: String,
}

/// One quarterly figure from a companyconcept payload.
#[derive(Clone, Debug, PartialEq)]
struct Quarter {
    frame: String,
    end: String,
    value: f64,
    form: String,
    filed: String,
    accession: String,
}

static QUARTER_FRAME: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^CY(\d{4})Q([1-4])$").expect("valid pattern"));

/// Quarterly USD durations from a companyconcept payload, one per calendar frame. EDGAR assigns
/// a frame only to the fact that best fits a calendar quarter, so frames are already deduplicated.
fn quarters(payload: &Value) -> Vec<Quarter> {
    payload
        .pointer("/units/USD")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|fact| {
            let frame = fact["frame"].as_str()?;
            QUARTER_FRAME.is_match(frame).then_some(())?;
            let value = fact["val"].as_f64().filter(|value| value.is_finite())?;
            let end = fact["end"].as_str()?;
            NaiveDate::parse_from_str(end, "%Y-%m-%d").ok()?;
            Some(Quarter {
                frame: frame.into(),
                end: end.into(),
                value,
                form: fact["form"].as_str().unwrap_or("").into(),
                filed: fact["filed"].as_str().unwrap_or("").into(),
                accession: fact["accn"]
                    .as_str()
                    .filter(|accn| {
                        !accn.is_empty() && accn.chars().all(|c| c.is_ascii_digit() || c == '-')
                    })
                    .unwrap_or("")
                    .into(),
            })
        })
        .collect()
}

/// The same calendar quarter one year earlier: `CY2026Q2` → `CY2025Q2`.
fn year_earlier(frame: &str) -> Option<String> {
    let captures = QUARTER_FRAME.captures(frame)?;
    let year: u32 = captures[1].parse().ok()?;
    Some(format!("CY{}Q{}", year.checked_sub(1)?, &captures[2]))
}

/// The latest reported quarter and its comparisons. `None` when no revenue tag holds a quarter
/// that ended within `RECENT_QUARTER_DAYS` of `today`.
pub(super) fn latest_fundamentals(
    revenue_payloads: &[Value],
    net_income: Option<&Value>,
    cik: u64,
    today: NaiveDate,
) -> Option<Fundamentals> {
    let recent = |quarter: &Quarter| {
        NaiveDate::parse_from_str(&quarter.end, "%Y-%m-%d")
            .is_ok_and(|end| end <= today && (today - end).num_days() <= RECENT_QUARTER_DAYS)
    };
    let revenue = revenue_payloads.iter().map(quarters).find(|quarters| {
        quarters
            .iter()
            .any(|quarter| recent(quarter) && quarter.value > 0.0)
    })?;
    let latest = revenue
        .iter()
        .filter(|quarter| recent(quarter) && quarter.value > 0.0)
        .max_by(|a, b| a.end.cmp(&b.end))?
        .clone();
    let prior = year_earlier(&latest.frame).and_then(|frame| {
        revenue
            .iter()
            .find(|quarter| quarter.frame == frame && quarter.value > 0.0)
    });
    let income = net_income
        .map(quarters)
        .unwrap_or_default()
        .into_iter()
        .find(|quarter| quarter.frame == latest.frame);
    let round = |value: f64| (value * 10.0).round() / 10.0;
    Some(Fundamentals {
        revenue_growth_pct: prior.map(|prior| round((latest.value / prior.value - 1.0) * 100.0)),
        net_margin_pct: income.map(|income| round(income.value / latest.value * 100.0)),
        url: if latest.accession.is_empty() {
            format!("https://www.sec.gov/cgi-bin/browse-edgar?action=getcompany&CIK={cik:010}")
        } else {
            format!(
                "https://www.sec.gov/Archives/edgar/data/{cik}/{}/",
                latest.accession.replace('-', "")
            )
        },
        frame: latest.frame,
        quarter_end: latest.end,
        revenue: latest.value,
        form: latest.form,
        filed: latest.filed,
    })
}

impl Providers {
    /// A concept's facts for one company, or `None` when EDGAR holds none (as for most funds).
    async fn sec_concept(
        &self,
        contact: &SecContact,
        cik: u64,
        tag: &str,
    ) -> Result<Option<Value>, String> {
        let url =
            format!("https://data.sec.gov/api/xbrl/companyconcept/CIK{cik:010}/us-gaap/{tag}.json");
        let response = self.sec_request(contact, &url).await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        provider_response("SEC EDGAR", "/api/xbrl/companyconcept", response)
            .await
            .map(Some)
    }

    /// The latest quarter for a ticker, from the cache when it is fresh. Requests stop at the
    /// first revenue tag with a recent quarter, so most companies take two requests a week.
    pub(crate) async fn fundamentals(&self, symbol: &str) -> Result<Option<Fundamentals>, String> {
        let now = Utc::now().timestamp();
        if let Some(saved) = self.news_cache().await?.fundamentals(symbol, now)? {
            return Ok(saved);
        }
        let Some(contact) = self.sec_contact()? else {
            return Ok(None);
        };
        self.sec_ticker_map(&contact, false).await?;
        let cik = { self.news_cache().await?.sec_cik(symbol)? };
        let today = crate::session::new_york_date(Utc::now());
        let result = async {
            let Some(cik) = cik else {
                return Ok(None);
            };
            let mut revenue = Vec::new();
            for tag in REVENUE_TAGS {
                if let Some(payload) = self.sec_concept(&contact, cik, tag).await? {
                    revenue.push(payload);
                    if latest_fundamentals(&revenue, None, cik, today).is_some() {
                        break;
                    }
                }
            }
            if latest_fundamentals(&revenue, None, cik, today).is_none() {
                return Ok(None);
            }
            let income = self.sec_concept(&contact, cik, NET_INCOME_TAG).await?;
            Ok(latest_fundamentals(&revenue, income.as_ref(), cik, today))
        }
        .await;
        let cache = self.news_cache().await?;
        match result {
            Ok(fundamentals) => {
                cache.save_fundamentals(symbol, fundamentals.as_ref(), now)?;
                Ok(fundamentals)
            }
            Err(error) => {
                // A failure waits a day before asking EDGAR again.
                cache.fundamentals_failure(symbol, now)?;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(frame: Option<&str>, end: &str, value: f64) -> Value {
        let mut fact = json!({
            "start": "2026-01-01", "end": end, "val": value, "accn": "0000000000-26-000001",
            "fy": 2026, "fp": "Q2", "form": "10-Q", "filed": "2026-07-30"
        });
        if let Some(frame) = frame {
            fact["frame"] = frame.into();
        }
        fact
    }

    fn concept(facts: Vec<Value>) -> Value {
        json!({ "units": { "USD": facts } })
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 16).unwrap()
    }

    #[test]
    fn latest_quarter_compares_with_the_same_quarter_a_year_earlier() {
        let revenue = concept(vec![
            fact(Some("CY2025Q2"), "2025-06-28", 100.0),
            // A restated duplicate without a frame is ignored.
            fact(None, "2026-06-27", 999.0),
            fact(Some("CY2026Q2"), "2026-06-27", 112.0),
            // Year-to-date and annual durations carry other frames and are ignored.
            fact(Some("CY2026"), "2026-06-27", 500.0),
            fact(Some("CY2026Q1I"), "2026-03-28", 1.0),
        ]);
        let income = concept(vec![fact(Some("CY2026Q2"), "2026-06-27", 28.0)]);
        let result = latest_fundamentals(&[revenue], Some(&income), 1, today()).unwrap();
        assert_eq!(result.frame, "CY2026Q2");
        assert_eq!(result.quarter_end, "2026-06-27");
        assert_eq!(result.revenue, 112.0);
        assert_eq!(result.revenue_growth_pct, Some(12.0));
        assert_eq!(result.net_margin_pct, Some(25.0));
        assert_eq!(result.form, "10-Q");
        assert_eq!(
            result.url,
            "https://www.sec.gov/Archives/edgar/data/1/000000000026000001/"
        );
    }

    #[test]
    fn missing_comparisons_stay_unavailable_and_stale_tags_give_way_to_current_ones() {
        let stale = concept(vec![fact(Some("CY2023Q4"), "2023-12-30", 80.0)]);
        let current = concept(vec![fact(Some("CY2026Q2"), "2026-06-27", 50.0)]);
        let result = latest_fundamentals(&[stale.clone(), current], None, 1, today()).unwrap();
        assert_eq!(result.revenue, 50.0);
        assert_eq!(result.revenue_growth_pct, None);
        assert_eq!(result.net_margin_pct, None);
        // Only stale or non-positive revenue says nothing current.
        assert!(latest_fundamentals(&[stale], None, 1, today()).is_none());
        let negative = concept(vec![fact(Some("CY2026Q2"), "2026-06-27", -5.0)]);
        assert!(latest_fundamentals(&[negative], None, 1, today()).is_none());
        assert!(latest_fundamentals(&[], None, 1, today()).is_none());
    }

    #[test]
    fn frames_step_back_a_year() {
        assert_eq!(year_earlier("CY2026Q2").as_deref(), Some("CY2025Q2"));
        assert_eq!(year_earlier("CY2026"), None);
        assert_eq!(year_earlier("CY2026Q2I"), None);
    }
}

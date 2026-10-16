//! Dividend income ahead: for each security held, the cash dividends per share over the last
//! year, the next one announced, and what the shares held would earn at that pace. Special
//! dividends are one-off, so they count toward the next payment but not the yearly pace.
//! Amounts sum in whole cents.
use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use crate::finance_contract::{DividendRecord, Holding};
use crate::numeric::minor_units;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NextDividend {
    pub ex_date: NaiveDate,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payable_date: Option<NaiveDate>,
    pub rate: f64,
    /// At the shares held now.
    pub amount: f64,
    pub special: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityDividends {
    pub ticker: String,
    pub shares: f64,
    /// Regular dividends per share with an ex-date in the last year.
    pub trailing_rate: f64,
    pub payments: usize,
    /// The trailing rate against the saved price, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yield_percent: Option<f64>,
    /// The trailing rate on the shares held now.
    pub annual_income: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<NextDividend>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpcomingDividend {
    pub ticker: String,
    #[serde(flatten)]
    pub dividend: NextDividend,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DividendOutlook {
    /// Largest annual income first.
    pub securities: Vec<SecurityDividends>,
    pub annual_income: f64,
    /// Announced payments still to come, soonest first.
    pub upcoming: Vec<UpcomingDividend>,
}

fn date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

pub fn outlook(
    records: &[DividendRecord],
    holdings: &[Holding],
    today: NaiveDate,
) -> DividendOutlook {
    let mut held = BTreeMap::<String, (f64, Option<f64>)>::new();
    for holding in holdings
        .iter()
        .filter(|holding| holding.currency.as_deref().is_none_or(|code| code == "USD"))
    {
        let entry = held
            .entry(holding.ticker.trim().to_ascii_uppercase())
            .or_insert((0.0, holding.price));
        entry.0 += holding.shares.unwrap_or(0.0);
    }
    let year_ago = today - Duration::days(365);
    let mut outlook = DividendOutlook::default();
    let mut annual = 0_i64;
    for (ticker, (shares, price)) in held.into_iter().filter(|(_, (shares, _))| *shares > 0.0) {
        let own = records
            .iter()
            .filter(|record| record.symbol.eq_ignore_ascii_case(&ticker))
            .filter_map(|record| Some((date(&record.ex_date)?, record, record.rate?)))
            .collect::<Vec<_>>();
        let trailing = own
            .iter()
            .filter(|(ex_date, record, _)| {
                !record.special && *ex_date > year_ago && *ex_date <= today
            })
            .collect::<Vec<_>>();
        let trailing_rate = trailing.iter().map(|(_, _, rate)| rate).sum::<f64>();
        // The next payment still to come: not yet paid, or not yet past its ex-date.
        let next = own
            .iter()
            .filter(|(ex_date, record, _)| {
                let payable = record.payable_date.as_deref().and_then(date);
                payable.map_or(*ex_date >= today, |payable| payable >= today)
            })
            .min_by_key(|(ex_date, _, _)| *ex_date)
            .map(|(ex_date, record, rate)| NextDividend {
                ex_date: *ex_date,
                payable_date: record.payable_date.as_deref().and_then(date),
                rate: *rate,
                amount: minor_units(rate * shares) as f64 / 100.0,
                special: record.special,
            });
        if trailing.is_empty() && next.is_none() {
            continue;
        }
        let income = minor_units(trailing_rate * shares);
        annual += income;
        if let Some(next) = &next {
            outlook.upcoming.push(UpcomingDividend {
                ticker: ticker.clone(),
                dividend: next.clone(),
            });
        }
        outlook.securities.push(SecurityDividends {
            ticker,
            shares,
            trailing_rate: (trailing_rate * 1_000_000.0).round() / 1_000_000.0,
            payments: trailing.len(),
            yield_percent: price
                .filter(|price| *price > 0.0 && trailing_rate > 0.0)
                .map(|price| (trailing_rate / price * 10_000.0).round() / 100.0),
            annual_income: income as f64 / 100.0,
            next,
        });
    }
    outlook.securities.sort_by(|left, right| {
        right
            .annual_income
            .total_cmp(&left.annual_income)
            .then(left.ticker.cmp(&right.ticker))
    });
    outlook.upcoming.sort_by(|left, right| {
        left.dividend
            .ex_date
            .cmp(&right.dividend.ex_date)
            .then(left.ticker.cmp(&right.ticker))
    });
    outlook.annual_income = annual as f64 / 100.0;
    outlook
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn record(
        symbol: &str,
        ex_date: &str,
        payable: &str,
        rate: f64,
        special: bool,
    ) -> DividendRecord {
        DividendRecord {
            symbol: symbol.into(),
            ex_date: ex_date.into(),
            payable_date: Some(payable.into()),
            rate: Some(rate),
            special,
        }
    }

    fn holding(ticker: &str, account: &str, shares: f64, price: f64) -> Holding {
        serde_json::from_value(json!({
            "ticker": ticker, "name": ticker, "accountId": account, "shares": shares,
            "price": price, "value": shares * price, "costBasis": null, "dailyChangePct": null,
            "totalChangePct": null, "color": "#000", "currency": "USD"
        }))
        .unwrap()
    }

    #[test]
    fn projects_income_from_the_last_years_regular_dividends_and_finds_the_next() {
        let records = [
            record("VTI", "2025-09-20", "2025-09-25", 0.90, false),
            record("VTI", "2025-12-20", "2025-12-26", 0.95, false),
            record("VTI", "2026-03-20", "2026-03-25", 0.85, false),
            record("VTI", "2026-06-20", "2026-06-25", 0.90, false),
            record("VTI", "2026-07-01", "2026-07-08", 2.00, true),
            // Ex-date passed, payment still to come.
            record("VTI", "2026-09-18", "2026-09-24", 0.92, false),
            record("NONE", "2026-09-18", "2026-09-24", 1.00, false),
        ];
        let holdings = [
            holding("VTI", "a", 60.0, 300.0),
            holding("VTI", "b", 40.0, 300.0),
        ];
        let result = outlook(&records, &holdings, day("2026-09-20"));
        let vti = &result.securities[0];
        // The year back from today keeps Dec, Mar, Jun, and Sep; the special one doesn't count.
        assert_eq!((vti.trailing_rate, vti.payments), (3.62, 4));
        assert_eq!(vti.annual_income, 362.0);
        assert_eq!(vti.yield_percent, Some(1.21));
        let next = vti.next.as_ref().unwrap();
        assert_eq!((next.ex_date, next.amount), (day("2026-09-18"), 92.0));
        assert_eq!(result.annual_income, 362.0);
        assert_eq!(result.securities.len(), 1);
    }
}

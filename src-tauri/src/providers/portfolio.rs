//! The portfolio's move today from one live snapshot, and its sector exposure from EDGAR
//! industry codes. Both read held positions from the committed snapshot.
use super::news::{Benchmark, MARKET_FUND};
use super::*;
use crate::finance_contract::Holding;
use news_cache::NewsCache;

/// A day worth mentioning beside the long view: the whole portfolio moved this much.
pub const NOTABLE_DAY_PERCENT: f64 = 1.0;
/// One sector above this share of the stocks and funds held dominates the portfolio.
pub const SECTOR_CONCENTRATED_PERCENT: f64 = 40.0;
/// EDGAR is asked about at most this many unclassified stocks per request.
const SECTOR_LOOKUPS: usize = 40;

/// One security merged across accounts. Only USD positions with a known value and share count.
#[derive(Clone, Debug, PartialEq)]
pub struct HeldSecurity {
    pub ticker: String,
    pub name: String,
    pub shares: f64,
    pub value: f64,
    pub kind: Option<String>,
}

pub fn held_securities(holdings: &[Holding]) -> Vec<HeldSecurity> {
    let mut merged = BTreeMap::<String, HeldSecurity>::new();
    for holding in holdings {
        let usd = holding.currency.as_deref().is_none_or(|code| code == "USD");
        let (Some(shares), Some(value)) = (holding.shares, holding.value) else {
            continue;
        };
        if !usd || !shares.is_finite() || !value.is_finite() || shares <= 0.0 {
            continue;
        }
        let entry = merged
            .entry(holding.ticker.clone())
            .or_insert_with(|| HeldSecurity {
                ticker: holding.ticker.clone(),
                name: holding.name.clone(),
                shares: 0.0,
                value: 0.0,
                kind: holding.instrument_kind.clone(),
            });
        entry.shares += shares;
        entry.value += value;
    }
    merged.into_values().collect()
}

pub(crate) fn fund_or_other(kind: Option<&str>) -> bool {
    kind.map(str::to_lowercase).is_some_and(|kind| {
        matches!(
            kind.as_str(),
            "etf"
                | "mutual fund"
                | "mutual_fund"
                | "fund"
                | "crypto"
                | "cryptocurrency"
                | "bond"
                | "fixed income"
                | "fixed_income"
                | "option"
                | "derivative"
        )
    })
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioToday {
    /// The day's change across the positions with a live price and previous close.
    pub change: f64,
    pub change_pct: Option<f64>,
    pub market: Option<Benchmark>,
    /// `ahead`, `behind`, or `inline` (within 0.05 points) of the market today.
    pub vs_market: Option<&'static str>,
    /// Some held positions have no live day change, so `change` covers part of the portfolio.
    pub incomplete: bool,
    /// Every counted position, largest dollar effect first.
    pub positions: Vec<PositionToday>,
    /// The portfolio moved at least `NOTABLE_DAY_PERCENT`.
    pub notable: bool,
    pub as_of: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PositionToday {
    pub ticker: String,
    pub name: String,
    pub change_pct: f64,
    pub change: f64,
    pub sector: Option<Benchmark>,
}

/// The day's dollar effect of each position at live prices. Every amount is computed in cents.
pub(crate) fn portfolio_today(
    held: &[HeldSecurity],
    snapshots: &BTreeMap<String, MarketSnapshot>,
    sectors: &BTreeMap<String, (&'static str, &'static str)>,
) -> PortfolioToday {
    // A snapshot without a previous close has no day change; never treat it as flat.
    let quote = |symbol: &str| {
        snapshots
            .get(symbol)
            .filter(|snapshot| snapshot.previous_close_as_of.is_some())
            .filter(|snapshot| {
                snapshot.price.is_finite()
                    && snapshot.previous_close.is_finite()
                    && snapshot.previous_close > 0.0
            })
    };
    let benchmark = |(symbol, name): (&str, &str)| {
        quote(symbol).map(|snapshot| Benchmark {
            symbol: symbol.into(),
            name: name.into(),
            change_pct: snapshot.daily_change_pct,
        })
    };
    let market = benchmark(MARKET_FUND);
    let (mut change, mut prior, mut incomplete) = (0_i64, 0_i64, false);
    let mut as_of: Option<&str> = None;
    let mut positions = Vec::new();
    for security in held {
        let Some(snapshot) = quote(&security.ticker) else {
            incomplete = true;
            continue;
        };
        let cents = crate::numeric::minor_units;
        let effect = cents(security.shares * (snapshot.price - snapshot.previous_close));
        change += effect;
        prior += cents(security.shares * snapshot.previous_close);
        as_of = as_of.max(Some(snapshot.as_of.as_str()).filter(|value| !value.is_empty()));
        let sector = sectors.get(&security.ticker).copied().and_then(benchmark);
        positions.push(PositionToday {
            ticker: security.ticker.clone(),
            name: security.name.clone(),
            change_pct: snapshot.daily_change_pct,
            change: effect as f64 / 100.0,
            sector,
        });
    }
    positions.sort_by(|a, b| {
        b.change
            .abs()
            .total_cmp(&a.change.abs())
            .then(a.ticker.cmp(&b.ticker))
    });
    let change_pct = (prior > 0).then(|| (change as f64 / prior as f64 * 10_000.0).round() / 100.0);
    PortfolioToday {
        change: change as f64 / 100.0,
        change_pct,
        vs_market: change_pct
            .zip(market.as_ref().map(|market| market.change_pct))
            .map(|(you, index)| crate::insights::returns::compare(you, index)),
        notable: change_pct.is_some_and(|pct| pct.abs() >= NOTABLE_DAY_PERCENT),
        market,
        incomplete,
        positions,
        as_of: as_of.map(str::to_owned),
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SectorSlice {
    pub fund: String,
    pub label: String,
    pub value: f64,
    pub percent: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SectorExposure {
    /// Every held security with a known value; cash is not included.
    pub total: f64,
    /// Stocks by the sector fund matching their SEC industry code, largest first.
    pub sectors: Vec<SectorSlice>,
    /// Funds, crypto, bonds, and options: not broken down into their holdings.
    pub funds: f64,
    /// Stocks without an industry code yet, or one that maps to no sector.
    pub unclassified: f64,
    /// The largest sector, when it is at least `SECTOR_CONCENTRATED_PERCENT` of `total`.
    pub concentrated: Option<SectorSlice>,
    /// No SEC contact is saved, so stocks cannot be classified.
    pub needs_contact: bool,
}

pub(super) fn sector_exposure(
    held: &[HeldSecurity],
    sic: &BTreeMap<String, Option<u32>>,
    needs_contact: bool,
) -> SectorExposure {
    let cents = crate::numeric::minor_units;
    let total = held
        .iter()
        .map(|security| cents(security.value))
        .sum::<i64>();
    let mut sectors = BTreeMap::<&str, (&str, i64)>::new();
    let (mut funds, mut unclassified) = (0_i64, 0_i64);
    for security in held {
        let value = cents(security.value);
        if fund_or_other(security.kind.as_deref()) || sic.get(&security.ticker) == Some(&None) {
            // EDGAR lists no operating company for the ticker: a fund or a foreign listing.
            funds += value;
            continue;
        }
        match sic
            .get(&security.ticker)
            .copied()
            .flatten()
            .and_then(sec::sector_for_sic)
        {
            Some((fund, label)) => sectors.entry(fund).or_insert((label, 0)).1 += value,
            None => unclassified += value,
        }
    }
    let percent = |part: i64| {
        if total > 0 {
            (part as f64 / total as f64 * 10_000.0).round() / 100.0
        } else {
            0.0
        }
    };
    let mut sectors: Vec<_> = sectors
        .into_iter()
        .map(|(fund, (label, value))| SectorSlice {
            fund: fund.into(),
            label: label.into(),
            value: value as f64 / 100.0,
            percent: percent(value),
        })
        .collect();
    sectors.sort_by(|a, b| b.value.total_cmp(&a.value).then(a.fund.cmp(&b.fund)));
    SectorExposure {
        total: total as f64 / 100.0,
        concentrated: sectors
            .first()
            .filter(|slice| slice.percent >= SECTOR_CONCENTRATED_PERCENT)
            .cloned(),
        sectors,
        funds: funds as f64 / 100.0,
        unclassified: unclassified as f64 / 100.0,
        needs_contact,
    }
}

impl Providers {
    pub async fn portfolio_today(&self, held: Vec<HeldSecurity>) -> Result<PortfolioToday, String> {
        let held: Vec<_> = held
            .into_iter()
            .filter(|security| {
                !matches!(
                    security.kind.as_deref().map(str::to_lowercase).as_deref(),
                    Some("crypto" | "cryptocurrency")
                ) && alpaca::stock_symbol(&security.ticker).is_ok()
            })
            .collect();
        let sectors = {
            let cache = self.news_cache().await?;
            let mut sectors = BTreeMap::new();
            for security in &held {
                if let Some(sector) = cache
                    .company(&security.ticker)?
                    .0
                    .and_then(|company| company.sic)
                    .and_then(sec::sector_for_sic)
                    .filter(|(fund, _)| *fund != security.ticker)
                {
                    sectors.insert(security.ticker.clone(), sector);
                }
            }
            sectors
        };
        let mut request: Vec<String> = held
            .iter()
            .map(|security| security.ticker.clone())
            .collect();
        request.push(MARKET_FUND.0.into());
        request.extend(sectors.values().map(|(fund, _)| (*fund).to_owned()));
        let snapshots = self.market_snapshots(request).await?.snapshots;
        Ok(portfolio_today(&held, &snapshots, &sectors))
    }

    /// Sector exposure for held stocks. Stocks EDGAR has not been asked about are looked up
    /// once, and the answer is saved with their filings.
    pub async fn portfolio_sectors(
        &self,
        held: Vec<HeldSecurity>,
    ) -> Result<SectorExposure, String> {
        let contact = self.sec_contact()?;
        let saved = |ticker: &str, cache: &NewsCache| -> Result<Option<Option<u32>>, String> {
            // `Some(None)`: EDGAR lists no company. `None`: never asked.
            if cache.filings_checked(ticker)? {
                Ok(Some(
                    cache.company(ticker)?.0.and_then(|company| company.sic),
                ))
            } else {
                Ok(None)
            }
        };
        let missing: Vec<String> = {
            let cache = self.news_cache().await?;
            let mut missing = Vec::new();
            for security in &held {
                if !fund_or_other(security.kind.as_deref())
                    && alpaca::stock_symbol(&security.ticker).is_ok()
                    && saved(&security.ticker, &cache)?.is_none()
                {
                    missing.push(security.ticker.clone());
                }
            }
            missing
        };
        if let Some(contact) = &contact {
            for ticker in missing.iter().take(SECTOR_LOOKUPS) {
                // A failed lookup leaves the stock unclassified and is asked again next time.
                if let Ok(company) = self.sec_company(contact, ticker).await {
                    self.news_cache().await?.save_filings(
                        ticker,
                        company.as_ref(),
                        Utc::now().timestamp(),
                    )?;
                }
            }
        }
        let cache = self.news_cache().await?;
        let mut sic = BTreeMap::new();
        for security in &held {
            if let Some(code) = saved(&security.ticker, &cache)? {
                sic.insert(security.ticker.clone(), code);
            }
        }
        Ok(sector_exposure(&held, &sic, contact.is_none()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn security(ticker: &str, shares: f64, value: f64, kind: &str) -> HeldSecurity {
        HeldSecurity {
            ticker: ticker.into(),
            name: ticker.into(),
            shares,
            value,
            kind: Some(kind.into()),
        }
    }

    fn quote(symbol: &str, price: f64, previous: f64) -> (String, MarketSnapshot) {
        (
            symbol.into(),
            MarketSnapshot {
                symbol: symbol.into(),
                price,
                previous_close: previous,
                previous_close_as_of: Some("2026-10-02T20:00:00+00:00".into()),
                daily_change_pct: (price - previous) / previous * 100.0,
                weekly_change_pct: None,
                weekly_reference_price: None,
                weekly_reference_date: None,
                as_of: "2026-10-05T15:00:00Z".into(),
            },
        )
    }

    #[test]
    fn positions_merge_across_accounts_and_skip_unknown_values() {
        let holdings: Vec<Holding> = serde_json::from_value(json!([
            {"ticker": "TEST", "name": "Test", "accountId": "a", "shares": 2.0, "price": 10.0, "value": 20.0,
             "costBasis": null, "dailyChangePct": null, "totalChangePct": null, "color": ""},
            {"ticker": "TEST", "name": "Test", "accountId": "b", "shares": 1.0, "price": 10.0, "value": 10.0,
             "costBasis": null, "dailyChangePct": null, "totalChangePct": null, "color": ""},
            {"ticker": "EUR", "name": "Euro", "accountId": "a", "shares": 1.0, "price": 10.0, "value": 10.0,
             "costBasis": null, "dailyChangePct": null, "totalChangePct": null, "color": "", "currency": "EUR"},
            {"ticker": "NONE", "name": "None", "accountId": "a", "shares": 1.0, "price": null, "value": null,
             "costBasis": null, "dailyChangePct": null, "totalChangePct": null, "color": ""}
        ]))
        .unwrap();
        let held = held_securities(&holdings);
        assert_eq!(held.len(), 1);
        assert_eq!((held[0].shares, held[0].value), (3.0, 30.0));
    }

    #[test]
    fn todays_effect_is_shares_times_the_price_change_and_ranks_by_dollars() {
        let held = [
            security("BIG", 10.0, 1_000.0, "equity"),
            security("SMALL", 100.0, 1_000.0, "equity"),
            security("GONE", 1.0, 50.0, "equity"),
        ];
        let snapshots = BTreeMap::from([
            quote("BIG", 103.0, 100.0),
            quote("SMALL", 9.8, 10.0),
            quote("SPY", 98.4, 100.0),
            quote("XLK", 100.2, 100.0),
        ]);
        let sectors = BTreeMap::from([("BIG".to_string(), ("XLK", "Technology"))]);
        let today = portfolio_today(&held, &snapshots, &sectors);
        assert!(today.incomplete);
        assert_eq!(today.change, 10.0);
        assert_eq!(today.change_pct, Some(0.5));
        assert_eq!(today.positions[0].ticker, "BIG");
        assert_eq!(today.positions[0].change, 30.0);
        assert_eq!(today.positions[1].change, -20.0);
        // +0.5% against the index's −1.6%.
        assert_eq!(today.vs_market, Some("ahead"));
        // Half a percent isn't a notable day for the whole portfolio.
        assert!(!today.notable);
        assert_eq!(today.market.unwrap().symbol, "SPY");
    }

    #[test]
    fn sectors_cover_stocks_and_leave_funds_and_unknown_stocks_apart() {
        let held = [
            security("CHIP", 1.0, 500.0, "equity"),
            security("SOFT", 1.0, 100.0, "equity"),
            security("NEW", 1.0, 100.0, "equity"),
            security("FUND", 1.0, 200.0, "etf"),
            security("ADR", 1.0, 100.0, "equity"),
        ];
        let sic = BTreeMap::from([
            ("CHIP".to_string(), Some(3674)),
            ("SOFT".to_string(), Some(7372)),
            ("ADR".to_string(), None),
        ]);
        let exposure = sector_exposure(&held, &sic, false);
        assert_eq!(exposure.total, 1_000.0);
        assert_eq!(exposure.sectors.len(), 1);
        assert_eq!(exposure.sectors[0].fund, "XLK");
        assert_eq!(exposure.sectors[0].value, 600.0);
        assert_eq!(exposure.sectors[0].percent, 60.0);
        assert_eq!(exposure.funds, 300.0);
        assert_eq!(exposure.unclassified, 100.0);
        assert_eq!(exposure.concentrated.unwrap().label, "Technology");
    }
}

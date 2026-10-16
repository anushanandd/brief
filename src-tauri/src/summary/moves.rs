//! How the portfolio or a holding moved: over the period against its own normal, and in the
//! latest session.

use super::*;

/// A holding's total return over the period beside its benchmarks', with its layers summed
/// over the period's sessions; its kind follows those layers.
pub(super) fn period_move(
    ticker: &str,
    sector: Option<(&str, &str)>,
    closes: &BTreeMap<String, f64>,
    prices: &BTreeMap<String, f64>,
    layers: Option<HoldingPeriod>,
) -> Option<PeriodMove> {
    let change = |symbol: &str| {
        let close = closes.get(symbol).filter(|close| **close > 0.0)?;
        let price = prices
            .get(symbol)
            .filter(|price| price.is_finite() && **price > 0.0)?;
        Some(((price / close - 1.0) * 10_000.0).round() / 100.0)
    };
    let change_pct = change(ticker)?;
    let benchmark = |(symbol, name): (&str, &str)| {
        change(symbol).map(|change_pct| Benchmark {
            symbol: symbol.into(),
            name: name.into(),
            change_pct,
        })
    };
    let market = (ticker != MARKET_FUND.0)
        .then(|| benchmark(MARKET_FUND))
        .flatten();
    let sector = sector.and_then(benchmark);
    Some(PeriodMove {
        change_pct,
        market,
        sector,
        kind: layers.as_ref().and_then(HoldingPeriod::kind),
        layers,
    })
}

/// Holdings whose period was unusual for them, however small: their own layer at least
/// `NOTABLE_Z` typical periods, largest first.
pub(super) fn notable_moves(
    records: &[&SessionRecord],
    tickers: &[String],
    changes: &BTreeMap<String, f64>,
) -> Vec<NotableMove> {
    let mut notable: Vec<NotableMove> = tickers
        .iter()
        .filter_map(|ticker| {
            let layers = sessions::holding_period(records, ticker)?;
            layers
                .own_z
                .filter(|z| z.abs() >= NOTABLE_Z)
                .map(|_| NotableMove {
                    ticker: ticker.clone(),
                    change_pct: changes.get(ticker).copied(),
                    layers,
                })
        })
        .collect();
    notable.sort_by(|a, b| {
        let z = |item: &NotableMove| item.layers.own_z.unwrap_or(0.0).abs();
        z(b).total_cmp(&z(a)).then(a.ticker.cmp(&b.ticker))
    });
    notable.truncate(NOTABLE);
    notable
}

/// A holding's latest session against its sector fund and the S&P 500.
pub(super) fn holding_today(
    ticker: &str,
    sector: Option<(&'static str, &'static str)>,
    snapshots: &BTreeMap<String, MarketSnapshot>,
) -> Option<TodayMove> {
    // A snapshot without a previous close has no day change; never treat it as flat.
    let change = |symbol: &str| {
        snapshots
            .get(symbol)
            .filter(|snapshot| snapshot.previous_close_as_of.is_some())
            .map(|snapshot| snapshot.daily_change_pct)
            .filter(|value| value.is_finite())
    };
    let stock = change(ticker)?;
    let benchmark = |(symbol, name): (&str, &str)| {
        change(symbol).map(|change_pct| Benchmark {
            symbol: symbol.into(),
            name: name.into(),
            change_pct,
        })
    };
    let market = (ticker != MARKET_FUND.0)
        .then(|| benchmark(MARKET_FUND))
        .flatten();
    let sector = sector.and_then(benchmark);
    Some(TodayMove {
        as_of: snapshots.get(ticker).map(|snapshot| snapshot.as_of.clone()),
        change_pct: Some(stock),
        change: None,
        // Named by its layers once today's session is split.
        kind: None,
        market,
        sector,
        positions: Vec::new(),
        sectors: Vec::new(),
    })
}

/// The portfolio's latest session: the holdings that moved it, largest dollar effect first, and
/// how its holdings in each large sector moved together.
pub(super) fn portfolio_today_move(
    today: &portfolio::PortfolioToday,
    held: &[HeldSecurity],
) -> Option<TodayMove> {
    if today.positions.is_empty() {
        return None;
    }
    let total: f64 = held.iter().map(|security| security.value).sum();
    let mut sectors: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for position in &today.positions {
        let (Some(sector), Some(security)) = (
            &position.sector,
            held.iter()
                .find(|security| security.ticker == position.ticker),
        ) else {
            continue;
        };
        let entry = sectors.entry(sector.name.clone()).or_default();
        entry.0 += security.value;
        entry.1 += position.change;
    }
    let mut sectors: Vec<SectorToday> = sectors
        .into_iter()
        .filter(|(_, (value, change))| *value - *change > 0.0 && total > 0.0)
        .map(|(name, (value, change))| SectorToday {
            name,
            weight_pct: (value / total * 1_000.0).round() / 10.0,
            change_pct: (change / (value - change) * 10_000.0).round() / 100.0,
        })
        .filter(|sector| sector.weight_pct >= SECTOR_WEIGHT_PCT)
        .collect();
    sectors.sort_by(|a, b| {
        b.weight_pct
            .total_cmp(&a.weight_pct)
            .then(a.name.cmp(&b.name))
    });
    sectors.truncate(2);
    Some(TodayMove {
        as_of: today.as_of.clone(),
        change_pct: today.change_pct,
        change: Some(today.change),
        market: today.market.clone(),
        sector: None,
        kind: None,
        positions: today
            .positions
            .iter()
            .take(TODAY_POSITIONS)
            .map(|position| TodayPosition {
                ticker: position.ticker.clone(),
                change_pct: position.change_pct,
                change: position.change,
                kind: None,
            })
            .collect(),
        sectors,
    })
}

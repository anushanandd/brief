//! Open tax lots per position, rebuilt first-in-first-out from the saved trade history, and
//! the day's largest movers. Lots are an estimate: brokers may use another method, and a
//! position whose shares the history does not explain is marked partial, never padded.
use std::collections::BTreeMap;

use chrono::{Months, NaiveDate};
use serde::Serialize;

use crate::finance_contract::{Holding, Trade};
use crate::numeric::{from_minor, minor_units};

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Lot {
    pub acquired: NaiveDate,
    pub units: f64,
    pub cost: f64,
    /// Units at the position's saved price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain: Option<f64>,
    /// Held more than one year as of today.
    pub long_term: bool,
    /// The first day a sale would be long-term.
    pub long_term_on: NaiveDate,
    /// Tax a sale today would add (negative: save) at this year's marginal rates, in a
    /// taxable account with a tax profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_tax: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PositionLots {
    pub account_id: String,
    pub ticker: String,
    /// `complete` when the open lots account for every share held, otherwise `partial`.
    pub coverage: &'static str,
    /// Shares held that no saved purchase explains.
    pub untracked_units: f64,
    pub lots: Vec<Lot>,
    /// Unrealized gain on the tracked lots, when the position has a saved price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_term_gain: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_term_gain: Option<f64>,
    /// What the money in this position earned since `since`, given when it went in and came
    /// out. Only when the saved trades explain every share and the position has a value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub money_weighted_pct: Option<f64>,
    /// The first saved trade.
    pub since: NaiveDate,
    /// Held in a retirement account, where a sale inside the account is not taxed.
    pub tax_deferred: bool,
}

/// Shares in one position that turn long-term on one day.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LongTermSoon {
    pub ticker: String,
    pub account_id: String,
    pub date: NaiveDate,
    pub units: f64,
    /// Unrealized gain on those shares, when every merged lot has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain: Option<f64>,
}

/// How far ahead lots turning long-term are listed. A sale before then is short-term.
pub const LONG_TERM_SOON_DAYS: u64 = 60;

/// Lots that become long-term within `LONG_TERM_SOON_DAYS`, soonest first.
pub fn long_term_soon(positions: &[PositionLots], today: NaiveDate) -> Vec<LongTermSoon> {
    let horizon = today + chrono::Days::new(LONG_TERM_SOON_DAYS);
    let mut merged = BTreeMap::<(NaiveDate, &str, &str), (f64, Option<i64>)>::new();
    // The long-term rate matters only where a sale is taxed.
    for position in positions.iter().filter(|position| !position.tax_deferred) {
        for lot in position
            .lots
            .iter()
            .filter(|lot| !lot.long_term && lot.long_term_on <= horizon)
        {
            let entry = merged
                .entry((
                    lot.long_term_on,
                    position.ticker.as_str(),
                    position.account_id.as_str(),
                ))
                .or_insert((0.0, Some(0)));
            entry.0 += lot.units;
            entry.1 = entry
                .1
                .zip(lot.gain)
                .map(|(sum, gain)| sum + minor_units(gain));
        }
    }
    merged
        .into_iter()
        .map(|((date, ticker, account_id), (units, gain))| LongTermSoon {
            ticker: ticker.to_owned(),
            account_id: account_id.to_owned(),
            date,
            units: round_units(units),
            gain: gain.map(from_minor),
        })
        .collect()
}

fn round_units(units: f64) -> f64 {
    (units * 1_000_000.0).round() / 1_000_000.0
}

/// Open lots for every position with at least one saved purchase.
pub fn lots(holdings: &[Holding], trades: &[Trade], today: NaiveDate) -> Vec<PositionLots> {
    let mut history = BTreeMap::<(&str, &str), Vec<(NaiveDate, &Trade)>>::new();
    for trade in trades {
        let (Some(ticker), Some(date)) = (
            trade.ticker.as_deref(),
            trade
                .date
                .get(..10)
                .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()),
        ) else {
            continue;
        };
        history
            .entry((trade.account_id.as_str(), ticker))
            .or_default()
            .push((date, trade));
    }
    holdings
        .iter()
        .filter_map(|holding| {
            let mut trades = history
                .get(&(holding.account_id.as_str(), holding.ticker.as_str()))?
                .clone();
            trades.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.id.cmp(&right.1.id)));
            // (acquired, units, cost in cents), oldest first.
            let mut open = Vec::<(NaiveDate, f64, i64)>::new();
            let since = trades.first()?.0;
            // Money in (purchases) and out (sales) by days since the first trade.
            let mut flows = Vec::<(i64, f64)>::new();
            for (date, trade) in trades {
                let kind = trade.r#type.to_ascii_uppercase();
                let units = trade.units.filter(|units| *units > 0.0)?;
                let amount = if trade.amount != 0.0 {
                    minor_units(trade.amount.abs())
                } else {
                    minor_units(units * trade.price?)
                };
                if kind.contains("BUY") || kind.contains("REINVEST") {
                    let cost = amount;
                    flows.push(((date - since).num_days(), from_minor(cost)));
                    open.push((date, units, cost));
                } else if kind.contains("SELL") {
                    flows.push(((date - since).num_days(), -from_minor(amount)));
                    let mut remaining = units;
                    while remaining > 1e-9 {
                        // A sale of shares the history never bought leaves the earlier lots
                        // unknowable, so this position has no lot view.
                        let (_, held, cost) = open.first_mut()?;
                        if *held <= remaining + 1e-9 {
                            remaining -= *held;
                            open.remove(0);
                        } else {
                            let kept = (*held - remaining) / *held;
                            *cost = (*cost as f64 * kept).round() as i64;
                            *held -= remaining;
                            remaining = 0.0;
                        }
                    }
                }
            }
            if open.is_empty() {
                return None;
            }
            let tracked = open.iter().map(|(_, units, _)| units).sum::<f64>();
            let shares = holding.shares?;
            let untracked = shares - tracked;
            // More tracked shares than held means the history and the position disagree.
            if untracked < -1e-3 * shares.max(1.0) {
                return None;
            }
            let complete = untracked.abs() <= 1e-3 * shares.max(1.0);
            let lots = open
                .into_iter()
                .map(|(acquired, units, cost)| {
                    let value = holding.price.map(|price| minor_units(units * price));
                    let long_term_on = acquired
                        .checked_add_months(Months::new(12))
                        .and_then(|date| date.succ_opt())
                        .unwrap_or(acquired);
                    Lot {
                        acquired,
                        units: round_units(units),
                        cost: from_minor(cost),
                        value: value.map(from_minor),
                        gain: value.map(|value| from_minor(value - cost)),
                        long_term: today >= long_term_on,
                        long_term_on,
                        estimated_tax: None,
                    }
                })
                .collect::<Vec<_>>();
            let gain = |long_term: bool| {
                holding.price.map(|_| {
                    from_minor(
                        lots.iter()
                            .filter(|lot| lot.long_term == long_term)
                            .filter_map(|lot| lot.gain)
                            .map(minor_units)
                            .sum(),
                    )
                })
            };
            Some(PositionLots {
                account_id: holding.account_id.clone(),
                ticker: holding.ticker.clone(),
                coverage: if complete { "complete" } else { "partial" },
                untracked_units: if complete {
                    0.0
                } else {
                    round_units(untracked)
                },
                short_term_gain: gain(false),
                long_term_gain: gain(true),
                money_weighted_pct: holding
                    .value
                    .filter(|_| complete)
                    .and_then(|value| {
                        super::returns::money_weighted_flows(
                            &flows,
                            (today - since).num_days(),
                            value,
                        )
                    })
                    .map(crate::numeric::round2),
                since,
                lots,
                tax_deferred: false,
            })
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mover {
    pub ticker: String,
    pub name: String,
    pub change_pct: f64,
    /// The position's change in value since the prior close.
    pub change: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Movers {
    /// The latest observation among the positions counted, when the prices carry one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
    /// The change across every position with a saved daily change.
    pub change: f64,
    /// Positions with a known value but no saved daily change are left out of `change`.
    pub incomplete: bool,
    /// The largest moves by amount, either direction.
    pub positions: Vec<Mover>,
}

const LISTED: usize = 5;

/// The day's change by position from saved prices, merged across accounts.
pub fn movers(holdings: &[Holding]) -> Option<Movers> {
    let mut by_ticker = BTreeMap::<&str, (&str, i64, i64)>::new();
    let mut incomplete = false;
    let mut counted = false;
    let mut as_of: Option<&str> = None;
    for holding in holdings {
        let usd = holding.currency.as_deref().is_none_or(|code| code == "USD");
        let (Some(value), Some(percent)) =
            (holding.value.filter(|_| usd), holding.daily_change_pct)
        else {
            incomplete |= holding.value.is_some();
            continue;
        };
        if percent <= -100.0 {
            incomplete = true;
            continue;
        }
        // Today's value is the prior close grown by the change, so the change is the rest.
        let value = minor_units(value);
        let prior = (value as f64 / (1.0 + percent / 100.0)).round() as i64;
        let entry =
            by_ticker
                .entry(holding.ticker.as_str())
                .or_insert((holding.name.as_str(), 0, 0));
        entry.1 += value - prior;
        entry.2 += prior;
        counted = true;
        as_of = as_of.max(holding.market_as_of.as_deref());
    }
    let change = by_ticker.values().map(|(_, change, _)| change).sum::<i64>();
    let mut positions = by_ticker
        .into_iter()
        .filter(|(_, (_, change, prior))| *change != 0 && *prior > 0)
        .map(|(ticker, (name, change, prior))| Mover {
            ticker: ticker.to_owned(),
            name: name.to_owned(),
            change_pct: (change as f64 / prior as f64 * 10_000.0).round() / 100.0,
            change: from_minor(change),
        })
        .collect::<Vec<_>>();
    positions.sort_by(|left, right| {
        right
            .change
            .abs()
            .total_cmp(&left.change.abs())
            .then_with(|| left.ticker.cmp(&right.ticker))
    });
    positions.truncate(LISTED);
    counted.then(|| Movers {
        as_of: as_of.map(str::to_owned),
        change: from_minor(change),
        incomplete,
        positions,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn holding(ticker: &str, account: &str, shares: f64, price: Option<f64>) -> Holding {
        serde_json::from_value(json!({
            "ticker": ticker, "name": ticker, "accountId": account, "shares": shares,
            "price": price, "value": price.map(|price| price * shares), "costBasis": null,
            "dailyChangePct": null, "totalChangePct": null, "color": "",
        }))
        .unwrap()
    }

    fn trade(id: &str, kind: &str, date: &str, units: f64, amount: f64) -> Trade {
        serde_json::from_value(json!({
            "id": id, "type": kind, "date": date, "amount": amount, "account": "Broker",
            "accountId": "a", "ticker": "VTI", "units": units, "price": null,
        }))
        .unwrap()
    }

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn sales_consume_the_oldest_shares_and_terms_turn_long_after_a_year_and_a_day() {
        let trades = [
            trade("1", "BUY", "2025-03-10", 10.0, -1_000.0),
            trade("2", "BUY", "2025-09-16", 10.0, -1_500.0),
            trade("3", "REINVESTMENT", "2026-06-01", 1.0, -200.0),
            trade("4", "SELL", "2026-07-01", 14.0, 2_800.0),
        ];
        let result = lots(
            &[holding("VTI", "a", 7.0, Some(210.0))],
            &trades,
            day("2026-09-16"),
        );
        let position = &result[0];
        assert_eq!(position.coverage, "complete");
        // The first lot and four shares of the second were sold: 6 of 10 remain at 60% of cost.
        assert_eq!(
            position.lots,
            vec![
                Lot {
                    acquired: day("2025-09-16"),
                    units: 6.0,
                    cost: 900.0,
                    value: Some(1_260.0),
                    gain: Some(360.0),
                    // Bought one year ago today: long-term starts tomorrow.
                    long_term: false,
                    long_term_on: day("2026-09-17"),
                    estimated_tax: None,
                },
                Lot {
                    acquired: day("2026-06-01"),
                    units: 1.0,
                    cost: 200.0,
                    value: Some(210.0),
                    gain: Some(10.0),
                    long_term: false,
                    long_term_on: day("2027-06-02"),
                    estimated_tax: None,
                },
            ]
        );
        assert_eq!(position.short_term_gain, Some(370.0));
        assert_eq!(position.long_term_gain, Some(0.0));
        // 2,700 went in and 2,800 came out, with 1,470 still held: the money earned a return.
        assert_eq!(position.since, day("2025-03-10"));
        assert!(position.money_weighted_pct.unwrap() > 50.0);
        let tomorrow = lots(
            &[holding("VTI", "a", 7.0, Some(210.0))],
            &trades,
            day("2026-09-17"),
        );
        assert_eq!(tomorrow[0].long_term_gain, Some(360.0));
        assert_eq!(tomorrow[0].short_term_gain, Some(10.0));
    }

    #[test]
    fn unexplained_shares_are_partial_and_contradictory_history_has_no_lots() {
        let buy = [trade("1", "BUY", "2026-01-05", 5.0, -500.0)];
        // Fifteen shares held, five explained: transferred-in shares have no known lot.
        let partial = lots(&[holding("VTI", "a", 15.0, None)], &buy, day("2026-09-16"));
        assert_eq!(partial[0].coverage, "partial");
        assert_eq!(partial[0].untracked_units, 10.0);
        assert_eq!(partial[0].lots[0].gain, None);
        assert_eq!(partial[0].short_term_gain, None);
        // Shares the history cannot place mean the money that bought them is unknown too.
        assert_eq!(partial[0].money_weighted_pct, None);
        // More bought than held, a sale with nothing to sell, or no purchases at all.
        assert!(lots(&[holding("VTI", "a", 2.0, None)], &buy, day("2026-09-16")).is_empty());
        let oversold = [
            trade("1", "BUY", "2026-01-05", 5.0, -500.0),
            trade("2", "SELL", "2026-02-05", 8.0, 900.0),
        ];
        assert!(lots(
            &[holding("VTI", "a", 1.0, None)],
            &oversold,
            day("2026-09-16")
        )
        .is_empty());
        assert!(lots(&[holding("VTI", "b", 5.0, None)], &buy, day("2026-09-16")).is_empty());
    }

    #[test]
    fn movers_rank_by_amount_and_leave_out_positions_without_a_daily_change() {
        let position = |ticker: &str, account: &str, value: f64, change: Option<f64>| -> Holding {
            serde_json::from_value(json!({
                "ticker": ticker, "name": ticker, "accountId": account, "shares": 1.0,
                "price": value, "value": value, "costBasis": null, "dailyChangePct": change,
                "totalChangePct": null, "color": "", "marketAsOf": "2026-09-16T20:00:00Z",
            }))
            .unwrap()
        };
        let result = movers(&[
            position("BIG", "a", 10_100.0, Some(1.0)),
            position("BIG", "b", 5_050.0, Some(1.0)),
            position("HOT", "a", 1_100.0, Some(10.0)),
            position("DOWN", "a", 950.0, Some(-5.0)),
            position("FLAT", "a", 500.0, Some(0.0)),
            position("DARK", "a", 700.0, None),
        ])
        .unwrap();
        // BIG gained $150 across two accounts, more than HOT's larger percentage.
        assert_eq!(
            result
                .positions
                .iter()
                .map(|mover| (mover.ticker.as_str(), mover.change, mover.change_pct))
                .collect::<Vec<_>>(),
            vec![
                ("BIG", 150.0, 1.0),
                ("HOT", 100.0, 10.0),
                ("DOWN", -50.0, -5.0)
            ]
        );
        assert_eq!(result.change, 200.0);
        assert!(result.incomplete);
        assert_eq!(result.as_of.as_deref(), Some("2026-09-16T20:00:00Z"));
        assert!(movers(&[position("DARK", "a", 700.0, None)]).is_none());
    }

    #[test]
    fn lots_turning_long_term_soon_merge_by_day_and_skip_long_and_distant_lots() {
        let positions = lots(
            &[holding("VTI", "a", 4.0, Some(150.0))],
            &[
                trade("old", "BUY", "2025-01-02", 1.0, -100.0),
                trade("soon", "BUY", "2025-10-20", 1.0, -100.0),
                trade("same", "BUY", "2025-10-20", 1.0, -120.0),
                trade("late", "BUY", "2026-06-01", 1.0, -140.0),
            ],
            day("2026-10-05"),
        );
        let soon = long_term_soon(&positions, day("2026-10-05"));
        assert_eq!(soon.len(), 1);
        assert_eq!(soon[0].ticker, "VTI");
        assert_eq!(soon[0].date, positions[0].lots[1].long_term_on);
        assert_eq!(soon[0].units, 2.0);
        assert_eq!(soon[0].gain, Some(80.0));
        // Nothing is listed while every lot is still more than 60 days away.
        assert!(long_term_soon(&positions, day("2026-06-01")).is_empty());
    }
}

#[cfg(test)]
mod return_tests {
    use chrono::NaiveDate;
    use serde_json::json;

    use super::lots;
    use crate::finance_contract::{Holding, Trade};

    #[test]
    fn one_purchase_held_a_year_returns_its_simple_gain() {
        let holding: Holding = serde_json::from_value(json!({
            "ticker": "VTI", "name": "VTI", "accountId": "a", "shares": 10.0, "price": 110.0,
            "value": 1_100.0, "costBasis": null, "dailyChangePct": null, "totalChangePct": null,
            "color": "",
        }))
        .unwrap();
        let trade: Trade = serde_json::from_value(json!({
            "id": "1", "type": "BUY", "date": "2025-09-16", "amount": -1_000.0, "account": "B",
            "accountId": "a", "ticker": "VTI", "units": 10.0, "price": 100.0,
        }))
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 9, 16).unwrap();
        assert_eq!(
            lots(&[holding], &[trade], today)[0].money_weighted_pct,
            Some(10.0)
        );
    }
}

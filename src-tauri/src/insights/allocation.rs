//! How invested money is spread: by security across accounts, by kind of instrument, and by
//! account, with how concentrated the largest positions are. Only known USD values count,
//! and a total that leaves something out says so.
use std::collections::BTreeMap;

use serde::Serialize;
use ts_rs::TS;

use crate::finance_contract::{Account, Holding};
use crate::numeric::{from_minor, minor_units};

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Slice {
    pub key: String,
    pub label: String,
    pub value: f64,
    pub percent: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "PositionAllocation")]
pub struct Allocation {
    /// Known USD positions plus uninvested cash in the accounts that hold them.
    pub total: f64,
    /// The uninvested cash, and its share of the total.
    pub cash: f64,
    pub cash_percent: f64,
    /// A position's value is unknown or not in USD, so the shares are of a subtotal.
    pub incomplete: bool,
    /// An account's uninvested cash is unknown, so cash may be missing from the total.
    pub cash_unknown: bool,
    /// Empty when the providers report no instrument kind for any position.
    pub kinds: Vec<Slice>,
    pub securities: Vec<Slice>,
    pub accounts: Vec<Slice>,
    /// The largest security's share of the total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub largest_percent: Option<f64>,
    /// The five largest securities' share of the total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub top_five_percent: Option<f64>,
    /// The largest security, when it alone is at least `CONCENTRATED_PERCENT` of the total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub concentrated: Option<Slice>,
}

/// A common rule of thumb: one company above a fifth of a portfolio dominates its risk. A
/// diversified fund at that share is an allocation choice, so funds are never flagged.
pub const CONCENTRATED_PERCENT: f64 = 20.0;

/// Whether an instrument kind names a pooled, diversified holding.
fn is_fund(kind: Option<&str>) -> bool {
    matches!(
        kind.map(str::trim).map(str::to_lowercase).as_deref(),
        Some("etf" | "mutual fund" | "mutual_fund" | "mutualfund" | "fund" | "cef" | "index")
    )
}

const CASH: &str = "cash";

/// One security merged across accounts, for the holdings list. Every security held appears,
/// even without a USD value, so nothing silently drops out of the list.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct SecurityHolding {
    pub ticker: String,
    pub name: String,
    pub accounts: usize,
    /// Known only when every position has a USD value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub value: Option<f64>,
    /// Share of the allocation total, which includes uninvested cash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub percent: Option<f64>,
    /// Value less cost basis, known only when every position has both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gain: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gain_percent: Option<f64>,
}

/// Securities largest first, then those without a known value by ticker.
pub fn security_holdings(
    holdings: &[Holding],
    allocation_total: Option<f64>,
) -> Vec<SecurityHolding> {
    let usd = |currency: Option<&str>| currency.is_none_or(|code| code == "USD");
    let mut merged = BTreeMap::<&str, (&str, Vec<&str>, Option<i64>, Option<i64>)>::new();
    for holding in holdings {
        let entry = merged.entry(holding.ticker.as_str()).or_insert((
            holding.name.as_str(),
            Vec::new(),
            Some(0),
            Some(0),
        ));
        if !entry.1.contains(&holding.account_id.as_str()) {
            entry.1.push(holding.account_id.as_str());
        }
        let value = holding
            .value
            .filter(|value| value.is_finite() && usd(holding.currency.as_deref()))
            .map(minor_units);
        let cost = holding
            .cost_basis
            .filter(|cost| cost.is_finite() && usd(holding.currency.as_deref()))
            .map(minor_units);
        entry.2 = entry.2.zip(value).map(|(sum, value)| sum + value);
        entry.3 = entry.3.zip(cost).map(|(sum, cost)| sum + cost);
    }
    let total = allocation_total.map(minor_units).filter(|total| *total > 0);
    let mut securities: Vec<_> = merged
        .into_iter()
        .map(|(ticker, (name, accounts, value, cost))| {
            let gain = value.zip(cost).map(|(value, cost)| value - cost);
            SecurityHolding {
                ticker: ticker.to_owned(),
                name: name.to_owned(),
                accounts: accounts.len(),
                value: value.map(|value| value as f64 / 100.0),
                percent: value.zip(total).map(|(value, total)| percent(value, total)),
                gain: gain.map(|gain| gain as f64 / 100.0),
                gain_percent: gain
                    .zip(cost.filter(|cost| *cost > 0))
                    .map(|(gain, cost)| percent(gain, cost)),
            }
        })
        .collect();
    securities.sort_by(|a, b| match (a.value, b.value) {
        (Some(a_value), Some(b_value)) => b_value.total_cmp(&a_value).then(a.ticker.cmp(&b.ticker)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.ticker.cmp(&b.ticker),
    });
    securities
}

fn percent(part: i64, total: i64) -> f64 {
    (part as f64 / total as f64 * 10_000.0).round() / 100.0
}

fn slices(groups: BTreeMap<String, (String, i64)>, total: i64) -> Vec<Slice> {
    let mut slices = groups
        .into_iter()
        .filter(|(_, (_, value))| *value > 0)
        .map(|(key, (label, value))| Slice {
            key,
            label,
            value: value as f64 / 100.0,
            percent: percent(value, total),
        })
        .collect::<Vec<_>>();
    slices.sort_by(|left, right| {
        right
            .value
            .total_cmp(&left.value)
            .then_with(|| left.key.cmp(&right.key))
    });
    slices
}

fn kind_label(kind: Option<&str>) -> (String, String) {
    let key = kind
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .unwrap_or("other")
        .to_lowercase();
    let label = match key.as_str() {
        "etf" => "ETFs".into(),
        "equity" | "stock" | "cs" => "Stocks".into(),
        "mutual fund" | "mutual_fund" | "fund" => "Funds".into(),
        "crypto" | "cryptocurrency" => "Crypto".into(),
        "bond" | "fixed income" | "fixed_income" => "Bonds".into(),
        "option" | "derivative" => "Options".into(),
        "other" => "Other".into(),
        other => {
            let mut characters = other.chars();
            characters
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + characters.as_str())
                .unwrap_or_default()
        }
    };
    (key, label)
}

pub fn allocation(holdings: &[Holding], accounts: &[Account]) -> Option<Allocation> {
    let usd = |currency: Option<&str>| currency.is_none_or(|code| code == "USD");
    let mut incomplete = false;
    let mut cash_unknown = false;
    let kinds_known = holdings.iter().any(|holding| {
        holding
            .instrument_kind
            .as_deref()
            .is_some_and(|kind| !kind.trim().is_empty())
    });
    let mut securities = BTreeMap::<String, (String, i64)>::new();
    let mut kinds = BTreeMap::<String, (String, i64)>::new();
    let mut by_account = BTreeMap::<String, (String, i64)>::new();
    let account_name = |id: &str| {
        accounts
            .iter()
            .find(|account| account.id == id)
            .map_or_else(|| id.to_owned(), |account| account.name.clone())
    };
    for holding in holdings {
        let Some(value) = holding
            .value
            .filter(|_| usd(holding.currency.as_deref()))
            .map(minor_units)
        else {
            incomplete = true;
            continue;
        };
        securities
            .entry(holding.ticker.clone())
            .or_insert_with(|| (holding.name.clone(), 0))
            .1 += value;
        let (key, label) = kind_label(holding.instrument_kind.as_deref());
        kinds.entry(key).or_insert((label, 0)).1 += value;
        by_account
            .entry(holding.account_id.clone())
            .or_insert_with(|| (account_name(&holding.account_id), 0))
            .1 += value;
    }
    // Uninvested cash sits beside the positions in the accounts that hold them.
    for account in accounts.iter().filter(|account| {
        holdings
            .iter()
            .any(|holding| holding.account_id == account.id)
    }) {
        match account
            .cash_value
            .filter(|_| usd(account.currency.as_deref()))
        {
            Some(cash) if cash > 0.0 => {
                let cash = minor_units(cash);
                kinds.entry(CASH.into()).or_insert(("Cash".into(), 0)).1 += cash;
                by_account
                    .entry(account.id.clone())
                    .or_insert_with(|| (account.name.clone(), 0))
                    .1 += cash;
            }
            Some(_) => {}
            None => cash_unknown = true,
        }
    }
    let total = kinds
        .values()
        .map(|(_, value)| (*value).max(0))
        .sum::<i64>();
    if total <= 0 {
        return None;
    }
    let securities = slices(securities, total);
    // What the positions leave of the total: the accounts' uninvested cash.
    let cash = total
        - securities
            .iter()
            .map(|slice| minor_units(slice.value))
            .sum::<i64>();
    let share = |count: usize| {
        (!securities.is_empty()).then(|| {
            let part = securities
                .iter()
                .take(count)
                .map(|slice| minor_units(slice.value))
                .sum::<i64>();
            percent(part, total)
        })
    };
    Some(Allocation {
        total: total as f64 / 100.0,
        cash: from_minor(cash.max(0)),
        cash_percent: percent(cash.max(0), total),
        incomplete,
        cash_unknown,
        largest_percent: share(1),
        top_five_percent: share(5),
        concentrated: securities
            .first()
            .filter(|slice| slice.percent >= CONCENTRATED_PERCENT)
            .filter(|slice| {
                !holdings
                    .iter()
                    .filter(|holding| holding.ticker == slice.key)
                    .any(|holding| is_fund(holding.instrument_kind.as_deref()))
            })
            .cloned(),
        securities,
        kinds: if kinds_known {
            slices(kinds, total)
        } else {
            Vec::new()
        },
        accounts: slices(by_account, total),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn holding(ticker: &str, account: &str, value: Option<f64>, kind: &str) -> Holding {
        serde_json::from_value(json!({
            "ticker": ticker, "name": ticker, "accountId": account, "shares": 1.0, "price": value,
            "value": value, "costBasis": null, "dailyChangePct": null, "totalChangePct": null,
            "color": "", "instrumentKind": kind, "currency": "USD",
        }))
        .unwrap()
    }

    fn account(id: &str, cash: Option<f64>) -> Account {
        serde_json::from_value(json!({
            "id": id, "name": id, "institution": "Broker", "type": "brokerage",
            "value": null, "cashValue": cash, "currency": "USD",
        }))
        .unwrap()
    }

    #[test]
    fn positions_merge_across_accounts_and_cash_counts_once_per_account() {
        let result = allocation(
            &[
                holding("VTI", "a", Some(6_000.0), "etf"),
                holding("VTI", "b", Some(1_000.0), "etf"),
                holding("AAPL", "a", Some(2_000.0), "equity"),
            ],
            &[
                account("a", Some(1_000.0)),
                account("b", Some(0.0)),
                account("idle", Some(900.0)),
            ],
        )
        .unwrap();
        assert_eq!(result.total, 10_000.0);
        // The positions leave the held accounts' uninvested cash.
        assert_eq!((result.cash, result.cash_percent), (1_000.0, 10.0));
        assert!(!result.incomplete);
        assert_eq!(result.securities[0].key, "VTI");
        assert_eq!(result.securities[0].percent, 70.0);
        assert_eq!(result.largest_percent, Some(70.0));
        assert_eq!(result.top_five_percent, Some(90.0));
        // A total-market fund at 70% is an allocation, not a concentration in one company.
        assert!(result.concentrated.is_none());
        assert_eq!(
            result
                .kinds
                .iter()
                .map(|slice| (slice.label.as_str(), slice.percent))
                .collect::<Vec<_>>(),
            vec![("ETFs", 70.0), ("Stocks", 20.0), ("Cash", 10.0)]
        );
        assert_eq!(result.accounts[0].value, 9_000.0);
    }

    #[test]
    fn unknown_values_mark_the_total_incomplete_and_nothing_known_is_unavailable() {
        let result = allocation(
            &[
                holding("VTI", "a", Some(500.0), "etf"),
                holding("XYZ", "a", None, "equity"),
            ],
            &[account("a", None)],
        )
        .unwrap();
        assert!(result.incomplete && result.cash_unknown);
        assert_eq!(result.securities.len(), 1);
        // Without any reported instrument kind there is no kind breakdown to show.
        let unlabeled: Holding = serde_json::from_value(json!({
            "ticker": "VTI", "name": "VTI", "accountId": "a", "shares": 1.0, "price": 5.0,
            "value": 5.0, "costBasis": null, "dailyChangePct": null, "totalChangePct": null,
            "color": "",
        }))
        .unwrap();
        let result = allocation(&[unlabeled], &[account("a", Some(0.0))]).unwrap();
        assert!(result.kinds.is_empty() && !result.incomplete && !result.cash_unknown);
        assert!(allocation(
            &[holding("XYZ", "a", None, "equity")],
            &[account("a", None)]
        )
        .is_none());
        assert!(allocation(&[], &[]).is_none());
    }

    #[test]
    fn a_position_is_flagged_only_from_a_fifth_of_the_total() {
        let spread = allocation(
            &(0..6)
                .map(|index| holding(&format!("T{index}"), "a", Some(100.0), "equity"))
                .collect::<Vec<_>>(),
            &[account("a", Some(0.0))],
        )
        .unwrap();
        assert!(spread.concentrated.is_none());
        let fifth = allocation(
            &[
                holding("BIG", "a", Some(800.0), "equity"),
                holding("REST", "a", Some(200.0), "etf"),
            ],
            &[account("a", Some(0.0))],
        )
        .unwrap();
        assert_eq!(fifth.securities[0].key, "BIG");
        assert_eq!(fifth.concentrated.unwrap().percent, 80.0);
        // The same share in a fund is diversification, not a bet on one company.
        let fund = allocation(
            &[
                holding("REST", "a", Some(800.0), "etf"),
                holding("BIG", "a", Some(200.0), "equity"),
            ],
            &[account("a", Some(0.0))],
        )
        .unwrap();
        assert!(fund.concentrated.is_none());
    }

    #[test]
    fn securities_merge_accounts_and_keep_unknown_values_listed() {
        let mut costly = holding("VTI", "a", Some(600.0), "etf");
        costly.cost_basis = Some(400.0);
        let mut other = holding("VTI", "b", Some(400.0), "etf");
        other.cost_basis = Some(400.0);
        let unknown = holding("ODD", "a", None, "equity");
        let basisless = holding("AAPL", "a", Some(200.0), "equity");
        let list = security_holdings(&[costly, other, unknown, basisless], Some(2_000.0));
        assert_eq!(
            list.iter()
                .map(|item| item.ticker.as_str())
                .collect::<Vec<_>>(),
            vec!["VTI", "AAPL", "ODD"]
        );
        assert_eq!(list[0].accounts, 2);
        assert_eq!(list[0].value, Some(1_000.0));
        assert_eq!(list[0].percent, Some(50.0));
        assert_eq!(list[0].gain, Some(200.0));
        assert_eq!(list[0].gain_percent, Some(25.0));
        assert_eq!(list[1].gain, None);
        assert_eq!(list[2].value, None);
    }
}

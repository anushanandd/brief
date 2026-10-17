//! What about the investments calls for a decision. Only items with a reason to act appear,
//! so an empty list honestly reads as "nothing needs you".
use chrono::NaiveDate;
use serde::Serialize;

use super::allocation::{Allocation, SecurityHolding};
use super::lots::LongTermSoon;
use crate::finance_contract::{Account, Holding};
use crate::numeric::minor_units;

/// A taxable position this far below cost is worth a look for tax-loss harvesting.
const HARVEST_MINIMUM: f64 = 500.0;
const HARVEST_MINIMUM_PERCENT: f64 = 5.0;
const HARVEST_LISTED: usize = 2;
/// Earnings for a holding this large can move the whole portfolio.
pub const EARNINGS_WATCH_PERCENT: f64 = 10.0;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InvestmentFlag {
    #[serde(rename_all = "camelCase")]
    Concentrated { ticker: String, percent: f64 },
    /// Shares in a taxable account that turn long-term soon, with a gain a sale would tax.
    #[serde(rename_all = "camelCase")]
    LongTermSoon {
        ticker: String,
        account_id: String,
        date: NaiveDate,
        units: f64,
        gain: f64,
    },
    /// A taxable position below cost, a loss a sale would realize.
    #[serde(rename_all = "camelCase")]
    HarvestableLoss {
        ticker: String,
        account_id: String,
        loss: f64,
        percent: f64,
    },
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentAttention {
    pub flags: Vec<InvestmentFlag>,
    /// Holdings at least `EARNINGS_WATCH_PERCENT` of investments, whose earnings dates matter.
    pub earnings_watch: Vec<String>,
}

pub fn attention(
    allocation: Option<&Allocation>,
    securities: &[SecurityHolding],
    long_term_soon: &[LongTermSoon],
    holdings: &[Holding],
    accounts: &[Account],
) -> InvestmentAttention {
    let mut flags = Vec::new();
    if let Some(slice) = allocation.and_then(|allocation| allocation.concentrated.as_ref()) {
        flags.push(InvestmentFlag::Concentrated {
            ticker: slice.key.clone(),
            percent: slice.percent,
        });
    }
    // `long_term_soon` already leaves out tax-deferred accounts and is soonest first.
    for soon in long_term_soon {
        if let Some(gain) = soon.gain.filter(|gain| *gain > 0.0) {
            flags.push(InvestmentFlag::LongTermSoon {
                ticker: soon.ticker.clone(),
                account_id: soon.account_id.clone(),
                date: soon.date,
                units: soon.units,
                gain,
            });
        }
    }
    let taxable = |id: &str| {
        accounts
            .iter()
            .any(|account| account.id == id && account.taxable())
    };
    let mut losses: Vec<_> = holdings
        .iter()
        .filter(|holding| holding.currency.as_deref().is_none_or(|code| code == "USD"))
        .filter(|holding| taxable(&holding.account_id))
        .filter_map(|holding| {
            let (value, cost) = (
                minor_units(holding.value?),
                minor_units(holding.cost_basis?),
            );
            let loss = value - cost;
            let percent = (loss as f64 / cost as f64 * 10_000.0).round() / 100.0;
            (cost > 0
                && loss as f64 <= -HARVEST_MINIMUM * 100.0
                && percent <= -HARVEST_MINIMUM_PERCENT)
                .then(|| InvestmentFlag::HarvestableLoss {
                    ticker: holding.ticker.clone(),
                    account_id: holding.account_id.clone(),
                    loss: loss as f64 / 100.0,
                    percent,
                })
        })
        .collect();
    losses.sort_by(|a, b| match (a, b) {
        (
            InvestmentFlag::HarvestableLoss { loss: a, .. },
            InvestmentFlag::HarvestableLoss { loss: b, .. },
        ) => a.total_cmp(b),
        _ => std::cmp::Ordering::Equal,
    });
    flags.extend(losses.into_iter().take(HARVEST_LISTED));
    InvestmentAttention {
        flags,
        earnings_watch: securities
            .iter()
            .filter(|security| {
                security
                    .percent
                    .is_some_and(|p| p >= EARNINGS_WATCH_PERCENT)
            })
            .map(|security| security.ticker.clone())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::allocation::Slice;
    use super::*;

    fn holding(ticker: &str, account: &str, value: f64, cost: f64) -> Holding {
        serde_json::from_value(json!({
            "ticker": ticker, "name": ticker, "accountId": account, "shares": 1.0, "price": value,
            "value": value, "costBasis": cost, "dailyChangePct": null, "totalChangePct": null,
            "color": "", "currency": "USD",
        }))
        .unwrap()
    }

    fn account(id: &str, kind: &str) -> Account {
        serde_json::from_value(json!({
            "id": id, "name": id, "institution": "Broker", "type": kind,
            "value": null, "cashValue": null, "currency": "USD",
        }))
        .unwrap()
    }

    #[test]
    fn flags_only_decisions_and_keeps_retirement_losses_out() {
        let allocation = Allocation {
            total: 10_000.0,
            cash: 0.0,
            cash_percent: 0.0,
            incomplete: false,
            cash_unknown: false,
            kinds: vec![],
            securities: vec![],
            accounts: vec![],
            largest_percent: Some(30.0),
            top_five_percent: Some(90.0),
            concentrated: Some(Slice {
                key: "BIG".into(),
                label: "Big".into(),
                value: 3_000.0,
                percent: 30.0,
            }),
        };
        let soon = [
            LongTermSoon {
                ticker: "GAIN".into(),
                account_id: "taxable".into(),
                date: NaiveDate::from_ymd_opt(2026, 11, 3).unwrap(),
                units: 2.0,
                gain: Some(210.0),
            },
            LongTermSoon {
                ticker: "DOWN".into(),
                account_id: "taxable".into(),
                date: NaiveDate::from_ymd_opt(2026, 11, 4).unwrap(),
                units: 1.0,
                gain: Some(-40.0),
            },
        ];
        let holdings = [
            holding("LOSS", "taxable", 2_000.0, 3_000.0),
            holding("SMALL", "taxable", 950.0, 1_000.0),
            holding("IRA", "ira", 1_000.0, 3_000.0),
        ];
        let securities = [SecurityHolding {
            ticker: "BIG".into(),
            name: "Big".into(),
            accounts: 1,
            value: Some(3_000.0),
            percent: Some(30.0),
            gain: None,
            gain_percent: None,
        }];
        let result = attention(
            Some(&allocation),
            &securities,
            &soon,
            &holdings,
            &[
                account("taxable", "brokerage"),
                account("ira", "retirement"),
            ],
        );
        let value = serde_json::to_value(&result).unwrap();
        assert_eq!(
            value["flags"][0],
            json!({"kind": "concentrated", "ticker": "BIG", "percent": 30.0})
        );
        assert_eq!(value["flags"][1]["kind"], "longTermSoon");
        assert_eq!(value["flags"][1]["accountId"], "taxable");
        assert_eq!(
            value["flags"][2],
            json!({"kind": "harvestableLoss", "ticker": "LOSS", "accountId": "taxable", "loss": -1000.0, "percent": -33.33})
        );
        assert_eq!(result.flags.len(), 3);
        assert_eq!(result.earnings_watch, vec!["BIG".to_string()]);
        assert_eq!(
            attention(None, &[], &[], &[], &[]),
            InvestmentAttention::default()
        );
    }
}

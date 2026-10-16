//! What you did in the period: trades and dividends from the committed snapshot.

use super::*;
use crate::finance_contract::{Snapshot, Transaction};
use crate::transaction_policy::Mark;

/// The committed snapshot's holdings, trades, transactions, and USD accounts.
pub(super) struct Ledger {
    pub held: Vec<HeldSecurity>,
    pub trades: Vec<Trade>,
    pub transactions: Vec<Transaction>,
    pub usd_accounts: BTreeSet<String>,
}

impl Ledger {
    pub(super) fn from_snapshot(snapshot: &Snapshot) -> Self {
        Self {
            held: held_securities(&snapshot.holdings),
            trades: snapshot.trades.clone(),
            transactions: snapshot.transactions.clone(),
            usd_accounts: snapshot
                .accounts
                .iter()
                .filter(|account| account.currency.as_deref().is_none_or(|code| code == "USD"))
                .map(|account| account.id.clone())
                .collect(),
        }
    }

    pub(super) fn activity(&self, period: Period, ticker: Option<&str>) -> Activity {
        activity(
            &self.trades,
            &self.transactions,
            &self.usd_accounts,
            &period,
            ticker,
        )
    }
}

/// Trades and dividends after `start`, through `end`. Dividend reinvestments are income, not
/// trades. A dividend is attributed to a holding only when its activity names the ticker.
pub(super) fn activity(
    trades: &[Trade],
    transactions: &[Transaction],
    usd_accounts: &BTreeSet<String>,
    period: &Period,
    ticker: Option<&str>,
) -> Activity {
    let mut traded: Vec<(&Trade, &'static str)> = trades
        .iter()
        .filter(|trade| period.covers(&trade.date))
        .filter(|trade| ticker.is_none_or(|ticker| trade.ticker.as_deref() == Some(ticker)))
        .filter_map(|trade| {
            let kind = trade.r#type.to_ascii_uppercase();
            if kind.contains("REINVEST") || kind.contains("DIVIDEND") {
                None
            } else if kind.contains("BUY") {
                Some((trade, "buy"))
            } else if kind.contains("SELL") {
                Some((trade, "sell"))
            } else {
                None
            }
        })
        .collect();
    traded.sort_by(|a, b| b.0.date.cmp(&a.0.date).then(a.0.id.cmp(&b.0.id)));
    let sales: Vec<&Trade> = traded
        .iter()
        .filter(|(_, side)| *side == "sell")
        .map(|(trade, _)| *trade)
        .collect();
    let realized = (!sales.is_empty())
        .then(|| {
            sales
                .iter()
                .map(|trade| {
                    trade
                        .estimated_realized_gain
                        .filter(|gain| gain.is_finite())
                        .map(minor_units)
                })
                .sum::<Option<i64>>()
        })
        .flatten()
        .map(|total| total as f64 / 100.0);
    let suffix = ticker.map(|ticker| format!(" · {ticker}"));
    let dividends: Vec<i64> = transactions
        .iter()
        .filter(|t| {
            t.classification
                .as_ref()
                .is_some_and(|classification| classification.mark == Mark::Dividend)
        })
        .filter(|t| !t.pending)
        .filter(|t| {
            t.account_id
                .as_deref()
                .is_some_and(|id| usd_accounts.contains(id))
        })
        .filter(|t| period.covers(t.posted_date()))
        .filter(|t| {
            suffix
                .as_deref()
                .is_none_or(|suffix| t.merchant.ends_with(suffix))
        })
        .map(|t| t.amount)
        .filter(|amount| amount.is_finite() && *amount > 0.0)
        .map(minor_units)
        .collect();
    Activity {
        buys: traded.iter().filter(|(_, side)| *side == "buy").count(),
        sells: sales.len(),
        trades: traded
            .iter()
            .take(LISTED_TRADES)
            .map(|(trade, side)| TradeFact {
                date: listed_date(&trade.date),
                side,
                ticker: trade.ticker.clone().unwrap_or_default(),
                units: trade
                    .units
                    .filter(|units| units.is_finite() && *units > 0.0),
                amount: (minor_units(trade.amount.abs()) as f64) / 100.0,
                realized_gain: (*side == "sell")
                    .then_some(trade.estimated_realized_gain)
                    .flatten()
                    .filter(|gain| gain.is_finite()),
            })
            .collect(),
        realized,
        dividends: dividends.iter().sum::<i64>() as f64 / 100.0,
        dividend_payments: dividends.len(),
    }
}

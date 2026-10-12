//! Snapshot and reference validation applied before every commit.
use super::*;
use crate::finance_contract::{
    Account, AccountMovement, Change, Holding, Performance, PossibleDuplicate, Spending, Trade,
    Transaction,
};

pub(super) fn validate_snapshot(snapshot: &Snapshot) -> Result<(), String> {
    chrono::DateTime::parse_from_rfc3339(&snapshot.updated_at)
        .map_err(|_| "Invalid snapshot timestamp")?;
    if snapshot
        .calendar_date
        .as_deref()
        .is_some_and(|date| !valid_date(date))
    {
        return Err("Invalid snapshot calendar date".into());
    }
    let ids = snapshot
        .accounts
        .iter()
        .map(|account| account.id.as_str())
        .collect::<BTreeSet<_>>();
    if !valuation_reconciles(snapshot, &ids) {
        return Err("Snapshot accounts, holdings, and net worth do not reconcile".into());
    }
    let transaction_ids = snapshot
        .transactions
        .iter()
        .map(|transaction| &transaction.id)
        .collect::<BTreeSet<_>>();
    if transaction_ids.len() != snapshot.transactions.len() {
        return Err("Duplicate transaction IDs".into());
    }
    if !snapshot.transactions.iter().all(valid_transaction) {
        return Err("Invalid transaction amounts or dates".into());
    }
    let trade_ids = snapshot
        .trades
        .iter()
        .map(|trade| &trade.id)
        .collect::<BTreeSet<_>>();
    if trade_ids.len() != snapshot.trades.len() {
        return Err("Duplicate trade IDs".into());
    }
    if !snapshot.trades.iter().all(valid_trade) {
        return Err("Invalid trade amounts or dates".into());
    }
    if !valid_movements(&snapshot.account_movements) {
        return Err("Invalid account movement history".into());
    }
    if !valid_points(&snapshot.net_worth_history, true)
        || !valid_points(&snapshot.observed_net_worth_history, false)
        || !valid_points(&snapshot.benchmark_history, false)
    {
        return Err("Snapshot history is not ordered, unique, and finite".into());
    }
    if !spending_reconciles(&snapshot.spending) {
        return Err("Spending totals do not reconcile".into());
    }
    if !snapshot
        .brokerage_performance
        .iter()
        .all(|performance| valid_brokerage_performance(performance, &ids))
    {
        return Err("Brokerage performance is invalid or references an unknown account".into());
    }
    if !valid_balance_histories(snapshot) {
        return Err(
            "Account balance history is invalid or references an unsupported account".into(),
        );
    }
    if !valid_duplicate_candidates(&snapshot.possible_duplicate_accounts, &ids) {
        return Err("Invalid possible duplicate account candidates".into());
    }
    if snapshot
        .calculation_version
        .is_some_and(|version| version == 0)
    {
        return Err("Invalid financial calculation version".into());
    }
    if snapshot
        .last_change
        .as_ref()
        .is_some_and(|change| !change_reconciles(change, &transaction_ids))
    {
        return Err("Snapshot change does not reconcile".into());
    }
    Ok(())
}

/// One "All accounts" row equal to the other accounts' sum and the net worth, an incomplete
/// flag exactly when some account has no value, and finite, well-referenced positions,
/// accounts, and trades.
fn valuation_reconciles(snapshot: &Snapshot, ids: &BTreeSet<&str>) -> bool {
    let total: f64 = snapshot
        .accounts
        .iter()
        .filter(|account| account.id != "all")
        .filter_map(|account| account.value)
        .sum();
    let all_accounts = snapshot
        .accounts
        .iter()
        .filter(|account| account.id == "all")
        .count();
    all_accounts == 1
        && ids.len() == snapshot.accounts.len()
        && !ids.contains("")
        && total.is_finite()
        && (total - snapshot.net_worth).abs() <= 0.011
        && snapshot.accounts.iter().all(|account| {
            account.id != "all"
                || account
                    .value
                    .is_some_and(|value| (value - total).abs() <= 0.011)
        })
        && snapshot.net_worth_incomplete
            == snapshot
                .accounts
                .iter()
                .any(|account| account.id != "all" && account.value.is_none())
        && snapshot
            .holdings
            .iter()
            .all(|holding| valid_holding(holding, ids))
        && snapshot.transactions.iter().all(|transaction| {
            transaction
                .account_id
                .as_ref()
                .is_none_or(|id| ids.contains(id.as_str()))
        })
        && snapshot
            .trades
            .iter()
            .all(|trade| valid_trade_result(trade, ids))
        && snapshot.accounts.iter().all(valid_account)
}

fn valid_holding(holding: &Holding, ids: &BTreeSet<&str>) -> bool {
    holding.account_id != "all"
        && ids.contains(holding.account_id.as_str())
        && [
            holding.shares,
            holding.price,
            holding.value,
            holding.cost_basis,
            holding.unrealized_gain,
            holding.daily_change_pct,
            holding.weekly_change_pct,
            holding.weekly_reference_price,
            holding.total_change_pct,
        ]
        .into_iter()
        .flatten()
        .all(f64::is_finite)
        && holding
            .market_as_of
            .as_deref()
            .is_none_or(valid_source_time)
        && holding
            .weekly_reference_date
            .as_deref()
            .is_none_or(valid_date)
        && holding.weekly_reference_price.is_some() == holding.weekly_reference_date.is_some()
        && holding.weekly_change_pct.is_some() == holding.weekly_reference_price.is_some()
}

/// A trade's account and the gain estimated for it.
fn valid_trade_result(trade: &Trade, ids: &BTreeSet<&str>) -> bool {
    ids.contains(trade.account_id.as_str())
        && trade.amount.is_finite()
        && [
            trade.realized_cost_basis,
            trade.estimated_realized_gain,
            trade.estimated_realized_gain_pct,
        ]
        .into_iter()
        .flatten()
        .all(f64::is_finite)
        && trade
            .realized_gain_method
            .as_deref()
            .is_none_or(|method| method == "estimated-fifo")
}

fn valid_account(account: &Account) -> bool {
    account.value.is_none_or(f64::is_finite)
        && [
            account.known_cost_basis,
            account.known_unrealized_gain,
            account.known_unrealized_gain_pct,
            account.investment_income_ytd,
            account.sale_proceeds_ytd,
            account.estimated_realized_gain_ytd,
        ]
        .into_iter()
        .flatten()
        .all(f64::is_finite)
        && [
            account.cost_basis_coverage.as_deref(),
            account.realized_gain_coverage.as_deref(),
        ]
        .into_iter()
        .flatten()
        .all(|coverage| ["complete", "partial", "unavailable"].contains(&coverage))
        && [
            account.balance_as_of.as_deref(),
            account.positions_as_of.as_deref(),
            account.activity_as_of.as_deref(),
        ]
        .into_iter()
        .flatten()
        .all(valid_source_time)
        && account
            .balance_fetched_at
            .as_deref()
            .is_none_or(|timestamp| chrono::DateTime::parse_from_rfc3339(timestamp).is_ok())
}

fn valid_transaction(transaction: &Transaction) -> bool {
    transaction.amount.is_finite()
        && valid_date(&transaction.date)
        && transaction.occurred_on.as_deref().is_none_or(valid_date)
        && transaction.posted_on.as_deref().is_none_or(valid_date)
}

fn valid_trade(trade: &Trade) -> bool {
    valid_date(&trade.date)
        && [Some(trade.amount), trade.units, trade.price]
            .into_iter()
            .flatten()
            .all(f64::is_finite)
}

fn valid_movements(movements: &[AccountMovement]) -> bool {
    let ids = movements
        .iter()
        .map(|movement| &movement.id)
        .collect::<BTreeSet<_>>();
    movements.len() <= 2_000
        && ids.len() == movements.len()
        && movements.iter().all(|movement| {
            !movement.id.is_empty()
                && !movement.account_id.is_empty()
                && chrono::DateTime::parse_from_rfc3339(&movement.observed_at).is_ok()
        })
}

/// Non-negative category totals with unique names that sum to the month and, when the month
/// has spending, to 100 percent.
fn spending_reconciles(spending: &Spending) -> bool {
    let total: f64 = spending
        .categories
        .iter()
        .map(|category| category.value)
        .sum();
    let percent: f64 = spending
        .categories
        .iter()
        .map(|category| category.percent)
        .sum();
    let names = spending
        .categories
        .iter()
        .map(|category| category.name.as_str())
        .collect::<BTreeSet<_>>();
    spending.month_total.is_finite()
        && spending.month_total >= 0.0
        && spending.categories.iter().all(|category| {
            category.value.is_finite()
                && category.value >= 0.0
                && category.percent.is_finite()
                && category.percent >= 0.0
        })
        && (total - spending.month_total).abs() <= 0.011
        && names.len() == spending.categories.len()
        && (spending.month_total <= 0.0 || (percent - 100.0).abs() <= 0.02)
        && (spending.month_total != 0.0 || percent.abs() <= 0.001)
}

fn valid_brokerage_performance(performance: &Performance, ids: &BTreeSet<&str>) -> bool {
    (performance.account_id == "total" || ids.contains(performance.account_id.as_str()))
        && performance.current_value.is_finite()
        && performance.history_start.as_deref().is_none_or(valid_date)
        && performance
            .performance_method
            .as_deref()
            .is_none_or(|method| {
                [
                    "value-only",
                    "value-with-comparisons",
                    "time-weighted",
                    "modified-dietz",
                ]
                .contains(&method)
            })
        && performance.history_source.as_deref().is_none_or(|source| {
            ["reported", "provider-estimated", "estimated", "unavailable"].contains(&source)
        })
        && valid_performance_points(
            &performance.points,
            performance.performance_method.as_deref(),
        )
}

/// One value-only, transaction-derived history per cash or card account.
fn valid_balance_histories(snapshot: &Snapshot) -> bool {
    let ids = snapshot
        .account_balance_history
        .iter()
        .map(|history| history.account_id.as_str())
        .collect::<BTreeSet<_>>();
    ids.len() == snapshot.account_balance_history.len()
        && snapshot.account_balance_history.iter().all(|history| {
            snapshot
                .accounts
                .iter()
                .find(|account| account.id == history.account_id)
                .is_some_and(|account| ["cash", "credit"].contains(&account.r#type.as_str()))
                && history.current_value.is_finite()
                && history.history_start.as_deref().is_none_or(valid_date)
                && matches!(
                    history.history_source.as_deref(),
                    Some("transaction-derived" | "unavailable")
                )
                && history.performance_method.as_deref() == Some("value-only")
                && valid_performance_points(&history.points, Some("value-only"))
        })
}

fn valid_duplicate_candidates(candidates: &[PossibleDuplicate], ids: &BTreeSet<&str>) -> bool {
    let mut pairs = BTreeSet::new();
    candidates.iter().all(|candidate| {
        !candidate.description.trim().is_empty()
            && candidate.plaid_account_id.starts_with("plaid:")
            && candidate.snaptrade_account_id.starts_with("snaptrade:")
            && ids.contains(candidate.plaid_account_id.as_str())
            && ids.contains(candidate.snaptrade_account_id.as_str())
            && pairs.insert((
                candidate.plaid_account_id.as_str(),
                candidate.snaptrade_account_id.as_str(),
            ))
    })
}

/// Account changes summing to the net-worth change, at valid times, naming saved transactions.
fn change_reconciles(change: &Change, transaction_ids: &BTreeSet<&String>) -> bool {
    let sum: f64 = change
        .account_changes
        .iter()
        .map(|account| account.change)
        .sum();
    change.net_worth_change.is_finite()
        && change.previous_net_worth.is_finite()
        && (sum - change.net_worth_change).abs() <= 0.011
        && chrono::DateTime::parse_from_rfc3339(&change.observed_at).is_ok()
        && chrono::DateTime::parse_from_rfc3339(&change.previous_updated_at).is_ok()
        && change
            .account_changes
            .iter()
            .all(|account| !account.account_id.is_empty() && account.change.is_finite())
        && change
            .new_transaction_ids
            .iter()
            .all(|id| transaction_ids.contains(id))
}

pub(super) fn valid_date(value: &str) -> bool {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
}

pub(super) fn valid_source_time(value: &str) -> bool {
    valid_date(value) || chrono::DateTime::parse_from_rfc3339(value).is_ok()
}

pub(super) fn valid_points(points: &[Point], allow_legacy_empty: bool) -> bool {
    let mut previous = None;
    points.iter().all(|point| {
        if !point.value.is_finite()
            || (point.date == "—" && !(allow_legacy_empty && points.len() == 1))
        {
            return false;
        }
        if point.date != "—" && !valid_date(&point.date) {
            return false;
        }
        let ordered = previous.is_none_or(|date: &str| date < point.date.as_str());
        previous = Some(point.date.as_str());
        ordered
    })
}

pub(super) fn valid_performance_points(points: &[PerformancePoint], method: Option<&str>) -> bool {
    let mut previous = None;
    points.iter().all(|point| {
        let ordered = previous.is_none_or(|date: &str| date < point.date.as_str());
        previous = Some(point.date.as_str());
        valid_date(&point.date)
            && ordered
            && point.value.is_finite()
            && point.net_deposits.is_none_or(f64::is_finite)
            && point.sp500.is_none_or(f64::is_finite)
            && point.market_change.is_none_or(f64::is_finite)
            && point.market_change_pct.is_none_or(f64::is_finite)
            && if method == Some("value-only") {
                point.net_deposits.is_none()
                    && point.sp500.is_none()
                    && point.market_change.is_none()
                    && point.market_change_pct.is_none()
            } else {
                point.net_deposits.is_some()
            }
    })
}

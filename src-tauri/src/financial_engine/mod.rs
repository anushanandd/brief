//! Canonical projection from provider caches to the committed snapshot.
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    str::FromStr,
};

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use rust_decimal::{prelude::ToPrimitive, Decimal, RoundingStrategy};
use serde::{de::Error as _, Deserialize, Deserializer};
use serde_json::Value;

use crate::{
    finance_contract::{
        Account, AccountChange, AccountMovement, Allocation, Change, Counterparty, CreditApr,
        CreditLiability, DividendRecord, FixedIncome, HistoryCheck, Holding, MarketProjection,
        OptionContract, Performance, PerformancePoint, Point, PossibleDuplicate, Provenance,
        RecurringStream, SecurityIdentifiers, Snapshot, Source, Spending, Trade, Transaction,
        TransactionLocation,
    },
    providers::{CorporateAction, MarketSnapshot, ProviderSync},
    storage::Annotation,
    transaction_policy::Mark,
};

mod accounts;
mod activities;
pub(crate) mod categories;
mod gains;
mod history;
mod holdings;
mod imported;
mod investments;
mod performance;
mod records;
mod transactions;
mod values;

use accounts::*;
use activities::*;
use gains::*;
use history::*;
use holdings::*;
use imported::*;
use investments::*;
use performance::*;
use records::*;
use transactions::*;
use values::*;

const COLORS: [&str; 6] = [
    "#232424", "#71877c", "#b87543", "#9d9588", "#6f7680", "#a39b8e",
];
const SPENDING_COLORS: [&str; 6] = [
    "#477d75", "#4f78a8", "#b58a3f", "#7d6da5", "#a45f79", "#79924b",
];
pub(crate) use imported::provider_activity;
pub(crate) const CALCULATION_VERSION: u32 = 35;

pub fn project(
    sync: &ProviderSync,
    account_links: &BTreeMap<String, String>,
    now: DateTime<Utc>,
    calendar_date: NaiveDate,
) -> Result<Snapshot, String> {
    let mut warnings = Vec::new();
    let mut parsed = Parsed::from_sync(sync, &mut warnings);
    // Statement rows join the provider's transactions below; the rest of `parsed` stays shared.
    let mut plaid_transactions = std::mem::take(&mut parsed.plaid_transactions);
    let possible_duplicates = possible_duplicate_accounts(&parsed, account_links, &mut warnings);
    let included_plaid_investments = parsed.included_plaid_investments(account_links);
    let included_plaid_ids = included_plaid_investments
        .iter()
        .map(|account| account.account_id.as_str())
        .collect::<BTreeSet<_>>();
    let committed_at = now.to_rfc3339();
    let today = calendar_date;
    let AccountRows {
        rows: mut accounts,
        net_worth,
        incomplete,
    } = project_accounts(sync, &parsed, &included_plaid_investments, &committed_at);
    let mut holdings = project_holdings(&parsed, &included_plaid_investments, &included_plaid_ids);

    let Imported {
        plaid_transactions: imported_plaid,
        history_starts: imported_starts,
        transactions: imported_transactions,
        trades: imported_trades,
    } = project_imported(
        &sync.statements,
        &accounts,
        &plaid_transactions,
        &parsed.snap_activities,
        &mut warnings,
    );
    plaid_transactions.extend(imported_plaid);
    let plaid_histories = reconstructed_plaid_histories(
        &parsed.plaid_accounts,
        &plaid_transactions,
        &sync.plaid.transaction_history_start,
        &imported_starts,
        today,
    );
    let account_names = parsed
        .plaid_accounts
        .iter()
        .map(|account| {
            (
                account.account_id.as_str(),
                text(account.name.as_deref(), "Account"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut transactions = plaid_transactions
        .into_iter()
        .filter_map(|transaction| plaid_transaction(transaction, &account_names))
        .collect::<Vec<_>>();
    let mut trades = Vec::new();
    for account in &parsed.snap_accounts {
        let activities = parsed
            .snap_activities
            .get(&account.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        transactions.extend(snap_transactions(account, activities));
        trades.extend(snap_trades(account, activities));
    }
    transactions.extend(imported_transactions);
    trades.extend(imported_trades);
    let plaid_investment_activity = plaid_investment_activity(
        &sync.plaid,
        &included_plaid_ids,
        &parsed.plaid_securities,
        &included_plaid_investments,
        &mut warnings,
    );
    trades.extend(plaid_investment_activity.trades);
    transactions.extend(plaid_investment_activity.transactions);
    // Newest first, then by id, so rows on one day keep a stable order.
    transactions.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then_with(|| left.id.cmp(&right.id))
    });
    trades.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then_with(|| left.id.cmp(&right.id))
    });
    adjust_for_splits(&mut trades, &holdings, &splits(&sync.corporate_actions));
    let plaid_history = sync
        .plaid
        .investment_history_accounts
        .iter()
        .map(|id| format!("plaid:{id}"))
        .collect::<BTreeSet<_>>();
    add_estimated_realized_gains(&mut trades, &holdings, &|account_id: &str| {
        if account_id.starts_with("plaid:") {
            plaid_history.contains(account_id)
        } else {
            sync.snaptrade.activity_complete
        }
    });
    let dividend_history = dividend_history(&sync.corporate_actions, &holdings, calendar_date);

    let today = today.to_string();
    let benchmark = parse_points(
        sync.benchmark_history
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default(),
        "Alpaca benchmark history",
        &mut warnings,
    );
    let Performances {
        individual: individual_performance,
        brokerage_flows,
    } = investment_performance(
        sync,
        &parsed,
        &included_plaid_investments,
        &benchmark,
        &today,
        &mut warnings,
    );
    let brokerage_performance = combined_performance(&individual_performance);
    let account_balance_history =
        plaid_balance_history(&parsed.plaid_accounts, &plaid_histories, &today);
    let provisional = !sync.balances_fresh || !brokerage_balances_current(&accounts, now);
    // Net worth is estimated back only when every total is known and every bank account has a
    // reconstructed history.
    let mut estimated_history =
        if !incomplete && plaid_histories.len() == parsed.plaid_accounts.len() {
            reconstructed_net_worth_history(
                plaid_histories.values().cloned().collect(),
                &individual_performance,
                &brokerage_flows,
            )
        } else {
            Vec::new()
        };
    if provisional {
        estimated_history.retain(|(date, _)| date != &today);
    }
    crate::transaction_policy::apply(
        &mut transactions,
        accounts
            .iter()
            .map(|account| (account.id.as_str(), account.name.as_str())),
        &BTreeMap::new(),
        &[],
    );
    let tracked_accounts = plaid_histories
        .keys()
        .map(|id| format!("plaid:{id}"))
        .chain(brokerage_flows.keys().cloned())
        .collect::<BTreeSet<_>>();
    let in_transit = in_transit_transfers(&transactions, &tracked_accounts);
    add_investment_metrics(
        &mut accounts,
        &mut holdings,
        &transactions,
        &trades,
        &today[..4],
    );

    let month_start = calendar_date.with_day(1).unwrap_or(calendar_date);
    let spending = spending(&transactions, month_start);

    if incomplete {
        warnings.push("Totals exclude balances without a USD value".into());
    }
    if holdings.iter().any(|holding| holding.value.is_none()) {
        warnings.push("Some positions are unpriced".into());
    }
    let refreshed = sync
        .refreshed_account_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let change = snapshot_change(
        &sync.previous_snapshot,
        &accounts,
        &transactions,
        &refreshed,
        &committed_at,
        incomplete,
    );
    let movements = merge_movements(&sync.previous_snapshot, change.as_ref());
    let observations = observed_history(
        &sync.previous_snapshot,
        net_worth,
        calendar_date,
        !provisional && !incomplete,
    );
    let history_check = history_check(&estimated_history, &observations);
    let (net_worth_history, net_worth_history_estimated) =
        merged_net_worth_history(estimated_history, &observations, &in_transit);
    let benchmark_history = benchmark
        .into_iter()
        .map(|(date, value)| Point {
            date,
            value: money(value),
        })
        .collect();
    warnings.sort();
    warnings.dedup();

    Ok(Snapshot {
        calculation_version: Some(CALCULATION_VERSION),
        revision: None,
        updated_at: committed_at,
        calendar_date: Some(calendar_date.to_string()),
        net_worth: money(net_worth),
        net_worth_incomplete: incomplete,
        net_worth_provisional: provisional,
        sync_warnings: warnings,
        accounts,
        holdings,
        transactions,
        trades,
        account_movements: movements,
        observed_net_worth_history: observations,
        net_worth_history,
        net_worth_history_estimated,
        benchmark_history,
        brokerage_performance,
        account_balance_history,
        possible_duplicate_accounts: possible_duplicates,
        history_check,
        dividend_history,
        recurring_streams: recurring_streams(&sync.plaid.recurring_streams),
        account_links: account_links.clone(),
        spending,
        last_change: change,
        provider_status: None,
        recovery: None,
        provenance: Some(Provenance {
            calculation: "rust".into(),
            benchmark: Source {
                provider: "Alpaca".into(),
                adjustment: "all".into(),
            },
            stock_plan_history: Source {
                provider: "Alpaca".into(),
                adjustment: "split".into(),
            },
        }),
    })
}

/// A committed snapshot as it is read: saved edits and rules applied to its transactions,
/// then this month's spending and each investment account's income this year again.
pub fn apply_annotations(
    snapshot: &mut Snapshot,
    annotations: &BTreeMap<String, Annotation>,
    rules: &[crate::rules::CategoryRule],
) -> Result<(), String> {
    let updated_at = DateTime::parse_from_rfc3339(&snapshot.updated_at)
        .map_err(|_| "Committed snapshot timestamp is invalid")?;
    let calendar_date = snapshot
        .calendar_date
        .as_deref()
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .unwrap_or_else(|| updated_at.date_naive());
    let month_start = calendar_date
        .with_day(1)
        .ok_or("Committed snapshot calendar date is invalid")?;
    crate::transaction_policy::apply(
        &mut snapshot.transactions,
        snapshot
            .accounts
            .iter()
            .map(|account| (account.id.as_str(), account.name.as_str())),
        annotations,
        rules,
    );
    snapshot.spending = spending(&snapshot.transactions, month_start);
    let year = calendar_date.format("%Y").to_string();
    for account in &mut snapshot.accounts {
        // Only investment accounts carry income; the projection set it for them.
        if account.investment_income_ytd.is_some() {
            account.investment_income_ytd = Some(investment_income(
                &snapshot.transactions,
                &account.id,
                &year,
            ));
        }
    }
    Ok(())
}

/// The committed valuation repriced at these quotes, ignoring any older than the saved price.
pub fn apply_market_snapshots(
    snapshot: &Snapshot,
    market: &BTreeMap<String, MarketSnapshot>,
) -> Result<MarketProjection, String> {
    apply_market_snapshots_with_freshness(snapshot, market, true)
}

/// The committed valuation repriced at these past quotes, such as the previous close.
pub fn apply_historical_market_snapshots(
    snapshot: &Snapshot,
    market: &BTreeMap<String, MarketSnapshot>,
) -> Result<MarketProjection, String> {
    apply_market_snapshots_with_freshness(snapshot, market, false)
}

fn apply_market_snapshots_with_freshness(
    snapshot: &Snapshot,
    market: &BTreeMap<String, MarketSnapshot>,
    reject_older_quotes: bool,
) -> Result<MarketProjection, String> {
    let mut projected = MarketProjection::from(snapshot);
    let valued_accounts = projected
        .accounts
        .iter()
        .filter(|account| account.value.is_some())
        .map(|account| account.id.clone())
        .collect::<BTreeSet<_>>();
    let mut deltas = BTreeMap::<String, Decimal>::new();
    let mut costed_deltas = BTreeMap::<String, Decimal>::new();
    for holding in &mut projected.holdings {
        let ticker = holding.ticker.trim().to_ascii_uppercase();
        let Some(quote) = market.get(&ticker) else {
            continue;
        };
        let Ok(quote_time) = DateTime::parse_from_rfc3339(&quote.as_of) else {
            continue;
        };
        if holding.quote_eligible != Some(true)
            || (reject_older_quotes
                && holding
                    .market_as_of
                    .as_deref()
                    .and_then(|source| DateTime::parse_from_rfc3339(source).ok())
                    .is_some_and(|source| quote_time < source))
        {
            continue;
        }
        let (Some(shares), Some(prior), Some(price)) = (
            holding.shares.and_then(decimal_f64),
            holding.value.and_then(decimal_f64),
            decimal_f64(quote.price),
        ) else {
            continue;
        };
        let value = rounded(shares * price);
        let basis = holding.cost_basis.and_then(decimal_f64);
        if valued_accounts.contains(&holding.account_id) {
            *deltas.entry(holding.account_id.clone()).or_default() += value - prior;
            if basis.is_some() {
                *costed_deltas.entry(holding.account_id.clone()).or_default() += value - prior;
            }
        }
        holding.price = Some(scaled(price, 8));
        holding.value = Some(money(value));
        holding.daily_change_pct = Some(quote.daily_change_pct);
        holding.weekly_change_pct = quote.weekly_change_pct;
        holding.weekly_reference_price = quote.weekly_reference_price;
        holding.weekly_reference_date = quote.weekly_reference_date.clone();
        holding.market_as_of = Some(quote.as_of.clone());
        holding.unrealized_gain = basis.map(|basis| money(value - basis));
        holding.total_change_pct = basis
            .filter(|basis| !basis.is_zero())
            .map(|basis| scaled((value - basis) / basis.abs() * Decimal::ONE_HUNDRED, 4));
    }
    let total_delta = deltas.values().copied().sum::<Decimal>();
    for account in &mut projected.accounts {
        let Some(value) = account.value.and_then(decimal_f64) else {
            continue;
        };
        let delta = if account.id == "all" {
            total_delta
        } else {
            deltas.get(&account.id).copied().unwrap_or_default()
        };
        account.value = Some(money(value + delta));
        if let Some(invested) = account.invested_value.and_then(decimal_f64) {
            account.invested_value = Some(money(invested + delta));
        }
        if let (Some(gain), Some(costed_delta), Some(basis)) = (
            account.known_unrealized_gain.and_then(decimal_f64),
            costed_deltas.get(&account.id),
            account.known_cost_basis.and_then(decimal_f64),
        ) {
            let gain = gain + *costed_delta;
            account.known_unrealized_gain = Some(money(gain));
            account.known_unrealized_gain_pct =
                (!basis.is_zero()).then(|| scaled(gain / basis.abs() * Decimal::ONE_HUNDRED, 4));
        }
    }
    let brokerage_delta = projected
        .accounts
        .iter()
        .filter(|account| matches!(account.r#type.as_str(), "brokerage" | "retirement"))
        .filter_map(|account| deltas.get(&account.id).copied())
        .sum::<Decimal>();
    for performance in &mut projected.brokerage_performance {
        let value =
            decimal_f64(performance.current_value).ok_or("Committed performance is invalid")?;
        let delta = if performance.account_id == "total" {
            brokerage_delta
        } else {
            deltas
                .get(&performance.account_id)
                .copied()
                .unwrap_or_default()
        };
        performance.current_value = money(value + delta);
    }
    let net_worth = decimal_f64(projected.net_worth).ok_or("Committed net worth is invalid")?;
    projected.net_worth = money(net_worth + total_delta);
    Ok(projected)
}

#[cfg(test)]
mod tests;

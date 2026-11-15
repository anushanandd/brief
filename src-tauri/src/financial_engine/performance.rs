//! Each investment account's value history beside the benchmark, and each cash account's
//! reconstructed balance history.
//!
//! A brokerage account's history is rebuilt from its records: from today's shares and cash,
//! each earlier day undoes the later cash movements and split-restated share changes, and
//! prices what was then held at Alpaca's close. The provider's own balance history can be wrong
//! by the size of a transfer, so it is used only when the records don't explain the account
//! (fewer than no shares, or shares Alpaca can't price) or activity didn't load.
//!
//! Shares a stock plan allocates arrive with no cash amount. They are a deposit into the account
//! that holds them, once: a custodian may record the allocation in a linked brokerage while the
//! shares sit in the plan, so a brokerage counts only the allocations its own shares explain. A
//! plan's deposit is what its shares cost, from the holding's cost basis, so a purchase discount
//! is a gain; without a cost basis it is that day's close.
use super::*;

/// Every investment account's performance record, sorted by account id, with the external
/// flows the SnapTrade accounts saw.
pub(super) struct Performances {
    pub individual: Vec<Performance>,
    pub brokerage_flows: BTreeMap<String, BTreeMap<String, Decimal>>,
}

/// SnapTrade accounts rebuilt from their trades and cash movements priced by Alpaca, or else
/// from the provider's balance history; Plaid stock plans from each holding's reconciled
/// acquisitions priced by Alpaca. `trades` are restated for splits.
pub(super) fn investment_performance(
    sync: &ProviderSync,
    parsed: &Parsed,
    included: &[&PlaidAccount],
    trades: &[Trade],
    benchmark: &[(String, Decimal)],
    today: &str,
    warnings: &mut Vec<String>,
) -> Performances {
    let mut individual_performance = Vec::new();
    let mut brokerage_flows = BTreeMap::new();
    let mut unheld_tickers = BTreeSet::new();
    let closes = sync
        .security_history
        .iter()
        .map(|(ticker, points)| {
            (
                ticker.to_ascii_uppercase(),
                parse_points(points, "Alpaca security history", warnings),
            )
        })
        .collect::<BTreeMap<_, _>>();
    // `parse_points` sorts each ticker's closes by date.
    let close = |ticker: &str, date: &str| {
        last_on_or_before(closes.get(ticker)?, true, date, |(day, _)| day).map(|(_, price)| *price)
    };
    for account in &parsed.snap_accounts {
        let Some(valuation) = parsed.calculated_snap_values.get(&account.id).copied() else {
            continue;
        };
        let current = valuation.total();
        let account_id = format!("snaptrade:{}", account.id);
        let account_trades = trades
            .iter()
            .filter(|trade| trade.account_id == account_id)
            .collect::<Vec<_>>();
        let positions = parsed
            .snap_positions
            .get(&account.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let recorded = parsed
            .snap_activities
            .get(&account.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let unheld = unheld_allocations(positions, recorded, &account_trades);
        let activities = recorded
            .iter()
            .filter(|activity| {
                share_allocation(activity).is_none_or(|(ticker, _)| !unheld.contains(&ticker))
            })
            .cloned()
            .collect::<Vec<_>>();
        let activities = activities.as_slice();
        unheld_tickers.extend(unheld);
        let reported = sync
            .snaptrade
            .balance_history
            .get(&account.id)
            .map(|points| parse_points(points, "SnapTrade history", warnings))
            .unwrap_or_default();
        let rebuilt = sync
            .snaptrade
            .activity_complete
            .then(|| {
                traded_history(
                    valuation,
                    positions,
                    activities,
                    &account_trades,
                    &close,
                    reported.first().map(|(date, _)| date.as_str()),
                    today,
                )
            })
            .flatten();
        // Only a rebuilt history knows each day's cash; the provider's gives a total.
        let (history, cash, source) = match rebuilt {
            Some(days) => (
                days.iter()
                    .map(|(date, value, _)| (date.clone(), *value))
                    .collect(),
                days.into_iter()
                    .map(|(date, _, cash)| (date, cash))
                    .collect(),
                "estimated",
            ),
            None => (reported, BTreeMap::new(), "provider-estimated"),
        };
        let flows = external_flows(activities, &close);
        let comparison_flows = (sync.snaptrade.activity_complete
            && (source == "estimated" || sync.snaptrade.history_complete))
            .then_some(&flows);
        let mut record = performance_record(
            SeriesAccount {
                id: account_id.clone(),
                name: text(
                    account.name.as_deref().or(account.raw_type.as_deref()),
                    "Brokerage account",
                ),
                institution: text(account.institution_name.as_deref(), "Brokerage"),
            },
            current,
            history,
            today,
            source,
            comparison_flows,
            benchmark,
        );
        split_cash(&mut record, |date| cash.get(date).copied());
        individual_performance.push(record);
        brokerage_flows.insert(account_id, flows);
    }
    for account in included {
        let Some(current) = usd_value(
            account.balances.current,
            account.balances.iso_currency_code.as_deref(),
        ) else {
            continue;
        };
        let (history, flows) = stock_plan_history(account, sync, parsed, warnings);
        let comparison_flows = (!history.is_empty()).then_some(&flows);
        let held = !history.is_empty();
        let mut record = performance_record(
            SeriesAccount {
                id: format!("plaid:{}", account.account_id),
                name: text(account.name.as_deref(), "Stock Plan"),
                institution: text(account.institution_name.as_deref(), "Brokerage"),
            },
            current,
            history,
            today,
            "estimated",
            comparison_flows,
            benchmark,
        );
        // The history exists only when every holding is a priced security, so none is cash.
        if held {
            split_cash(&mut record, |_| Some(Decimal::ZERO));
        }
        individual_performance.push(record);
    }
    let planned = parsed
        .plaid_holdings
        .iter()
        .filter(|holding| {
            included
                .iter()
                .any(|account| account.account_id == holding.account_id)
        })
        .filter_map(|holding| parsed.plaid_securities.get(&holding.security_id))
        .filter_map(|security| security.ticker_symbol.as_deref())
        .map(|ticker| ticker.trim().to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    if unheld_tickers
        .iter()
        .any(|ticker| !planned.contains(ticker))
    {
        warnings.push("Allocated shares no connected account holds aren't counted".into());
    }
    individual_performance.sort_by(|left, right| left.account_id.cmp(&right.account_id));
    Performances {
        individual: individual_performance,
        brokerage_flows,
    }
}

/// Fewer shares than this are none; providers round fractional shares.
const SHARE_TOLERANCE: Decimal = Decimal::from_parts(5, 0, 0, false, 4);

fn position_ticker(position: &SnapPosition) -> Option<String> {
    let label = &position.instrument.label;
    let ticker = label.raw_symbol.as_ref().or(label.symbol.as_ref())?;
    Some(ticker.trim().to_ascii_uppercase())
}

/// The shares a buy, reinvestment, or sale adds, negative for a sale.
fn trade_shares(trade: &Trade) -> Option<Decimal> {
    let kind = trade.r#type.to_ascii_uppercase();
    let shares = trade.units.and_then(decimal_f64).unwrap_or_default().abs();
    if kind.contains("BUY") || kind.contains("REINVEST") {
        Some(shares)
    } else if kind.contains("SELL") {
        Some(-shares)
    } else {
        None
    }
}

/// The tickers whose allocated shares this account never held: undoing its later trades and
/// allocations from today's shares leaves fewer than none on some day.
// shortcut: a ticker's allocations are all held or all not; one moved out in part by an
// unrecorded transfer reads as not held. Split them per allocation if that turns up.
fn unheld_allocations(
    positions: &[SnapPosition],
    activities: &[SnapActivity],
    trades: &[&Trade],
) -> BTreeSet<String> {
    let mut changes = BTreeMap::<String, BTreeMap<String, Decimal>>::new();
    for activity in activities {
        if let (Some((ticker, shares)), Some(date)) = (share_allocation(activity), activity.date())
        {
            *changes.entry(ticker).or_default().entry(date).or_default() += shares;
        }
    }
    for trade in trades {
        let ticker = trade
            .ticker
            .as_deref()
            .map(|ticker| ticker.trim().to_ascii_uppercase());
        if let (Some(days), Some(shares)) = (
            ticker.and_then(|ticker| changes.get_mut(&ticker)),
            trade_shares(trade),
        ) {
            *days.entry(trade.date.clone()).or_default() += shares;
        }
    }
    changes
        .into_iter()
        .filter(|(ticker, days)| {
            let mut held = positions
                .iter()
                .filter(|position| {
                    !position.cash_equivalent && position_ticker(position).as_ref() == Some(ticker)
                })
                .filter_map(|position| position.units)
                .sum::<Decimal>();
            days.values().rev().any(|change| {
                held -= *change;
                held < -SHARE_TOLERANCE
            })
        })
        .map(|(ticker, _)| ticker)
        .collect()
}

/// A SnapTrade account's daily value, rebuilt backward from today's shares and cash: each day
/// undoes the cash movements and share changes recorded after it, and prices the shares then
/// held at that day's close. Before the first recorded activity nothing changed, so it reaches
/// back to `provider_start` while the account held anything and closes price it. None when a
/// day since that activity holds fewer than no shares, or shares without a close, so records
/// that don't explain the account leave it to the provider's history. Each day carries the
/// cash it held.
fn traded_history(
    valuation: SnapValuation,
    positions: &[SnapPosition],
    activities: &[SnapActivity],
    trades: &[&Trade],
    close: &impl Fn(&str, &str) -> Option<Decimal>,
    provider_start: Option<&str>,
    today: &str,
) -> Option<Vec<(String, Decimal, Decimal)>> {
    let tolerance = SHARE_TOLERANCE;
    let mut cash = valuation.cash;
    let mut units = BTreeMap::<String, Decimal>::new();
    for position in positions
        .iter()
        .filter(|position| !position.cash_equivalent)
    {
        *units.entry(position_ticker(position)?).or_default() += position.units?;
    }
    // Each day's change in cash and in each ticker's shares. Shares come from the split-restated
    // trades, so they agree with Alpaca's split-adjusted closes.
    let mut changes = BTreeMap::<String, (Decimal, BTreeMap<String, Decimal>)>::new();
    for activity in activities {
        let date = activity.date()?;
        let change = changes.entry(date).or_default();
        change.0 += activity.amount;
        if let Some((ticker, shares)) = share_allocation(activity) {
            *change.1.entry(ticker).or_default() += shares;
        }
    }
    for trade in trades {
        let Some(signed) = trade_shares(trade) else {
            continue;
        };
        let ticker = trade.ticker.as_deref()?.trim().to_ascii_uppercase();
        *changes
            .entry(trade.date.clone())
            .or_default()
            .1
            .entry(ticker)
            .or_default() += signed;
    }
    let parse = |date: &str| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok();
    let first = parse(changes.keys().next()?)?;
    let start = provider_start
        .and_then(parse)
        .filter(|start| *start < first)
        .unwrap_or(first);
    let mut day = parse(today)?;
    let mut later = changes.into_iter().rev().peekable();
    let mut points = Vec::new();
    while day >= start {
        let date = day.format("%Y-%m-%d").to_string();
        while let Some((_, (amount, shares))) = later.next_if(|(on, _)| *on > date) {
            cash -= amount;
            for (ticker, count) in shares {
                *units.entry(ticker).or_default() -= count;
            }
        }
        let unrecorded = day < first;
        if unrecorded
            && cash.abs() <= tolerance
            && units.values().all(|count| count.abs() <= tolerance)
        {
            break;
        }
        let mut value = Some(cash);
        for (ticker, count) in &units {
            if *count < -tolerance {
                return None;
            }
            if *count > tolerance {
                value = value
                    .zip(close(ticker, &date))
                    .map(|(sum, price)| sum + *count * price);
            }
        }
        match value {
            Some(value) => points.push((date, rounded(value), rounded(cash))),
            None if unrecorded => break,
            None => return None,
        }
        day = day.pred_opt()?;
    }
    points.reverse();
    Some(points)
}

/// A stock plan's value history from each holding's reconciled acquisitions priced at Alpaca's
/// closes, with what the acquisitions cost as its deposits. Empty unless every holding can be
/// priced.
fn stock_plan_history(
    account: &PlaidAccount,
    sync: &ProviderSync,
    parsed: &Parsed,
    warnings: &mut Vec<String>,
) -> (Vec<(String, Decimal)>, BTreeMap<String, Decimal>) {
    let account_holdings = parsed
        .plaid_holdings
        .iter()
        .filter(|holding| holding.account_id == account.account_id)
        .collect::<Vec<_>>();
    let mut flows = BTreeMap::new();
    let histories = account_holdings
        .iter()
        .filter_map(|holding| {
            let security = parsed.plaid_securities.get(&holding.security_id)?;
            let ticker = security.ticker_symbol.as_deref()?;
            let acquisitions = reconciled_acquisitions(holding, ticker, &parsed.snap_activities);
            let prices = sync
                .security_history
                .get(&ticker.to_ascii_uppercase())
                .map(|points| parse_points(points, "Alpaca security history", warnings))
                .unwrap_or_default();
            let at_close = acquisitions
                .iter()
                .filter_map(|acquisition| {
                    let (_, price) = prices.iter().find(|(date, _)| date >= &acquisition.date)?;
                    Some((acquisition.date.clone(), acquisition.quantity * *price))
                })
                .collect::<Vec<_>>();
            let market = at_close.iter().map(|(_, value)| *value).sum::<Decimal>();
            // The cost basis covers every share, so it is spread only over a fully priced set,
            // in proportion to each acquisition's value that day.
            let cost = holding
                .cost_basis
                .filter(|basis| {
                    *basis > Decimal::ZERO
                        && !market.is_zero()
                        && at_close.len() == acquisitions.len()
                })
                .map_or(Decimal::ONE, |basis| basis / market);
            for (date, value) in at_close {
                *flows.entry(date).or_default() += rounded(value * cost);
            }
            (!acquisitions.is_empty() && !prices.is_empty())
                .then(|| holding_history(acquisitions, &prices))
        })
        .collect::<Vec<_>>();
    let history = if histories.len() == account_holdings.len() {
        combine_value_histories(&histories)
    } else {
        Vec::new()
    };
    (history, flows)
}

/// Each Plaid cash account's balance history, transaction-derived from its current balance.
pub(super) fn plaid_balance_history(
    plaid_accounts: &[PlaidAccount],
    plaid_histories: &BTreeMap<String, Vec<(String, Decimal)>>,
    today: &str,
) -> Vec<Performance> {
    plaid_accounts
        .iter()
        .filter_map(|account| {
            let history = plaid_histories.get(&account.account_id)?.clone();
            let current = history.last()?.1;
            Some(performance_record(
                SeriesAccount {
                    id: format!("plaid:{}", account.account_id),
                    name: text(account.name.as_deref(), "Account"),
                    institution: text(account.institution_name.as_deref(), "Unknown institution"),
                },
                current,
                history,
                today,
                "transaction-derived",
                None,
                &[],
            ))
        })
        .collect()
}

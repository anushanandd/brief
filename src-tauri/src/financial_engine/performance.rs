//! Each investment account's value history beside the benchmark, and each cash account's
//! reconstructed balance history.
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
    let close = |ticker: &str, date: &str| {
        closes
            .get(ticker)?
            .iter()
            .rev()
            .find(|(day, _)| day.as_str() <= date)
            .map(|(_, price)| *price)
    };
    for account in &parsed.snap_accounts {
        let Some(valuation) = parsed.calculated_snap_values.get(&account.id).copied() else {
            continue;
        };
        let current = valuation.total();
        let activities = parsed
            .snap_activities
            .get(&account.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let account_id = format!("snaptrade:{}", account.id);
        let account_trades = trades
            .iter()
            .filter(|trade| trade.account_id == account_id)
            .collect::<Vec<_>>();
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
                    parsed
                        .snap_positions
                        .get(&account.id)
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                    activities,
                    &account_trades,
                    &close,
                    reported.first().map(|(date, _)| date.as_str()),
                    today,
                )
            })
            .flatten();
        let (history, source) = match rebuilt {
            Some(history) => (history, "estimated"),
            None => (reported, "provider-estimated"),
        };
        let flows = external_flows(activities, &close);
        let comparison_flows = (sync.snaptrade.activity_complete
            && (source == "estimated" || sync.snaptrade.history_complete))
            .then_some(&flows);
        individual_performance.push(performance_record(
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
        ));
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
        individual_performance.push(performance_record(
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
        ));
    }
    individual_performance.sort_by(|left, right| left.account_id.cmp(&right.account_id));
    Performances {
        individual: individual_performance,
        brokerage_flows,
    }
}

/// A SnapTrade account's daily value, rebuilt backward from today's shares and cash: each day
/// undoes the cash movements and share changes recorded after it, and prices the shares then
/// held at that day's close. Before the first recorded activity nothing changed, so it reaches
/// back to `provider_start` while the account held anything and closes price it. None when a
/// day since that activity holds fewer than no shares, or shares without a close, so records
/// that don't explain the account leave it to the provider's history.
fn traded_history(
    valuation: SnapValuation,
    positions: &[SnapPosition],
    activities: &[SnapActivity],
    trades: &[&Trade],
    close: &impl Fn(&str, &str) -> Option<Decimal>,
    provider_start: Option<&str>,
    today: &str,
) -> Option<Vec<(String, Decimal)>> {
    let tolerance = Decimal::new(5, 4);
    let mut cash = valuation.cash;
    let mut units = BTreeMap::<String, Decimal>::new();
    for position in positions
        .iter()
        .filter(|position| !position.cash_equivalent)
    {
        let label = &position.instrument.label;
        let ticker = label.raw_symbol.as_ref().or(label.symbol.as_ref())?;
        *units.entry(ticker.trim().to_ascii_uppercase()).or_default() += position.units?;
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
        let kind = trade.r#type.to_ascii_uppercase();
        let shares = trade.units.and_then(decimal_f64).unwrap_or_default().abs();
        let signed = if kind.contains("BUY") || kind.contains("REINVEST") {
            shares
        } else if kind.contains("SELL") {
            -shares
        } else {
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
            Some(value) => points.push((date, rounded(value))),
            None if unrecorded => break,
            None => return None,
        }
        day = day.pred_opt()?;
    }
    points.reverse();
    Some(points)
}

/// A stock plan's value history from each holding's reconciled acquisitions priced at Alpaca's
/// closes, with the acquisitions as its deposits. Empty unless every holding can be priced.
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
            for acquisition in &acquisitions {
                if let Some((_, price)) = prices.iter().find(|(date, _)| date >= &acquisition.date)
                {
                    *flows.entry(acquisition.date.clone()).or_default() +=
                        acquisition.quantity * *price;
                }
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

//! Each investment account's value history beside the benchmark, and each cash account's
//! reconstructed balance history.
use super::*;

/// Every investment account's performance record, sorted by account id, with the external
/// flows the SnapTrade accounts saw.
pub(super) struct Performances {
    pub individual: Vec<Performance>,
    pub brokerage_flows: BTreeMap<String, BTreeMap<String, Decimal>>,
}

/// SnapTrade accounts from the provider's balance history; Plaid stock plans from each
/// holding's reconciled acquisitions priced by Alpaca.
pub(super) fn investment_performance(
    sync: &ProviderSync,
    parsed: &Parsed,
    included: &[&PlaidAccount],
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
        let Some(current) = parsed
            .calculated_snap_values
            .get(&account.id)
            .copied()
            .map(SnapValuation::total)
        else {
            continue;
        };
        let history = sync
            .snaptrade
            .balance_history
            .get(&account.id)
            .map(|points| parse_points(points, "SnapTrade history", warnings))
            .unwrap_or_default();
        let flows = external_flows(
            parsed
                .snap_activities
                .get(&account.id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            &close,
        );
        let comparison_flows =
            (sync.snaptrade.activity_complete && sync.snaptrade.history_complete).then_some(&flows);
        individual_performance.push(performance_record(
            SeriesAccount {
                id: format!("snaptrade:{}", account.id),
                name: text(
                    account.name.as_deref().or(account.raw_type.as_deref()),
                    "Brokerage account",
                ),
                institution: text(account.institution_name.as_deref(), "Brokerage"),
            },
            current,
            history,
            today,
            "provider-estimated",
            comparison_flows,
            benchmark,
        ));
        brokerage_flows.insert(format!("snaptrade:{}", account.id), flows);
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

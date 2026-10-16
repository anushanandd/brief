//! SnapTrade activities become the snapshot's transactions (dividends, interest, fees, taxes,
//! and external transfers) and trades (buys, sells, and reinvestments).
use super::*;

/// The account's cash activity, in provider order. A fee charged beside another activity is
/// its own transaction.
pub(super) fn snap_transactions(
    account: &SnapAccount,
    activities: &[SnapActivity],
) -> Vec<Transaction> {
    let account_name = text(
        account.name.as_deref().or(account.raw_type.as_deref()),
        "Investment account",
    );
    let account_id = format!("snaptrade:{}", account.id);
    let mut transactions = Vec::new();
    for (index, activity) in activities.iter().enumerate() {
        if is_reinvestment(activity) {
            continue;
        }
        let kind = activity.kind.to_ascii_uppercase();
        if !kind.contains("FEE") && activity.fee.is_some_and(|fee| !fee.is_zero()) {
            let date = activity.date();
            if let (Some(date), Some(fee)) = (date, activity.fee) {
                let id = activity
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("{date}:{kind}:{index}"));
                transactions.push(Transaction {
                    description: activity.external_reference_id.clone(),
                    ..cash_row(
                        format!("snaptrade:{}:{id}:fee", account.id),
                        "Investment fee".into(),
                        "Fees",
                        date,
                        -fee.abs(),
                        &account_name,
                        &account_id,
                    )
                });
            }
        }
        let mut flow = None;
        let (label, category, amount) = if kind.contains("DIVIDEND") {
            ("Dividend", "Dividend", activity.amount)
        } else if kind.contains("INTEREST") {
            ("Interest", "Interest", activity.amount)
        } else if kind.contains("FEE") {
            ("Investment fee", "Fees", activity.amount)
        } else if kind.contains("TAX") {
            ("Investment tax", "Taxes", activity.amount)
        } else if let Some(amount) = external_flow(activity) {
            flow = Some(if kind.contains("CONTRIBUTION") {
                "contribution"
            } else if kind == "DEPOSIT" {
                "deposit"
            } else if kind.contains("WITHDRAWAL") {
                "withdrawal"
            } else {
                "transfer"
            });
            (
                if amount.is_sign_negative() {
                    "Transfer out"
                } else {
                    "Transfer in"
                },
                "Transfer",
                amount,
            )
        } else {
            continue;
        };
        let Some(date) = activity.date() else {
            continue;
        };
        let posted_on = iso_date(activity.settlement_date.as_deref());
        let ticker = activity_ticker(activity);
        let merchant = if ticker.is_empty() {
            label.to_string()
        } else {
            format!("{label} · {ticker}")
        };
        let id = activity
            .id
            .clone()
            .unwrap_or_else(|| format!("{date}:{kind}:{}:{index}", activity.amount));
        transactions.push(Transaction {
            description: activity.description.clone(),
            occurred_on: iso_date(activity.trade_date.as_deref()),
            posted_on,
            flow: flow.map(str::to_owned),
            ..cash_row(
                format!("snaptrade:{}:{id}", account.id),
                merchant,
                category,
                date,
                amount,
                &account_name,
                &account_id,
            )
        });
    }
    transactions
}

/// The account's trades, in provider order. A dividend reinvestment is a buy paid with income.
pub(super) fn snap_trades(account: &SnapAccount, activities: &[SnapActivity]) -> Vec<Trade> {
    let mut trades = Vec::new();
    for (index, activity) in activities.iter().enumerate() {
        let kind = activity.kind.to_ascii_uppercase();
        let reinvestment = is_reinvestment(activity);
        if !reinvestment
            && !["BUY", "SELL", "TRADE"]
                .iter()
                .any(|needle| kind.contains(needle))
        {
            continue;
        }
        let Some(date) = activity.date() else {
            continue;
        };
        let ticker = activity_ticker(activity);
        let amount = if kind.contains("BUY") || reinvestment {
            -activity.amount.abs()
        } else {
            activity.amount.abs()
        };
        trades.push(Trade {
            id: activity.id.clone().unwrap_or_else(|| {
                format!(
                    "snaptrade:{}:{date}:{kind}:{ticker}:{}:{index}",
                    account.id, activity.amount
                )
            }),
            r#type: if reinvestment {
                "REINVESTMENT".into()
            } else {
                kind
            },
            date,
            amount: money(amount),
            account: text(
                account.name.as_deref().or(account.raw_type.as_deref()),
                "Investment account",
            ),
            account_id: format!("snaptrade:{}", account.id),
            ticker: (!ticker.is_empty()).then_some(ticker),
            description: activity.description.clone(),
            units: activity.units.map(|value| scaled(value, 8)),
            price: activity.price.map(|value| scaled(value, 8)),
            ..Trade::default()
        });
    }
    trades
}

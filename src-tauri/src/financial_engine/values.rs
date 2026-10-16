//! Decimal parsing and rounding, and the snapshot's number output.
use super::*;

/// A provider's number, written plainly or, for very small and large ones, in exponent form.
pub(super) fn decimal_value(value: &Value) -> Option<Decimal> {
    let text = match value {
        Value::Number(number) => number.to_string(),
        Value::String(number) => number.clone(),
        _ => return None,
    };
    Decimal::from_str(&text)
        .or_else(|_| Decimal::from_scientific(&text))
        .ok()
}

/// A finite float as the decimal it prints as, so `0.1` is exactly one tenth.
pub(super) fn decimal_f64(value: f64) -> Option<Decimal> {
    value
        .is_finite()
        .then(|| Decimal::from_str(&value.to_string()).ok())
        .flatten()
}

pub(super) fn rounded(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Dollars to whole cents, as the snapshot carries them.
pub(super) fn money(value: Decimal) -> f64 {
    rounded(value)
        .to_f64()
        .expect("a Decimal value always fits in f64")
}

/// Rounded to `decimal_places`, as the snapshot carries shares, prices, and percentages.
pub(super) fn scaled(value: Decimal, decimal_places: u32) -> f64 {
    value
        .round_dp_with_strategy(decimal_places, RoundingStrategy::MidpointAwayFromZero)
        .to_f64()
        .expect("a Decimal value always fits in f64")
}

pub(super) fn usd_value(value: Option<Decimal>, currency: Option<&str>) -> Option<Decimal> {
    (currency == Some("USD"))
        .then_some(value)
        .flatten()
        .map(rounded)
}

/// A posted cash row in an investment account, as provider activity and statements report it.
pub(super) fn cash_row(
    id: String,
    merchant: String,
    category: &str,
    date: String,
    amount: Decimal,
    account: &str,
    account_id: &str,
) -> Transaction {
    Transaction {
        id,
        merchant,
        category: category.into(),
        date,
        amount: money(amount),
        account: account.into(),
        account_id: Some(account_id.into()),
        ..Transaction::default()
    }
}

//! Plaid's recurring streams as a second opinion on Brief's forecast. A stream Plaid marks
//! inactive and tombstoned ended for good, so the series it covers is no longer expected. A
//! mature, active stream Brief doesn't forecast is listed for review; Brief's own series
//! policy still decides what is forecast.
use std::collections::BTreeSet;

use serde::Serialize;

use super::forecast::{recurring_key, ExpectedEvent};
use crate::finance_contract::RecurringStream;
use crate::finance_contract::Transaction;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnforecastStream {
    pub stream_id: String,
    pub account_id: String,
    pub name: String,
    pub frequency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_next_date: Option<String>,
    /// Brief's sign: negative for money out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_amount: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecurringCheck {
    /// Whether any Plaid streams were saved to compare.
    pub available: bool,
    /// Series no longer forecast because Plaid reports them ended.
    pub ended: Vec<String>,
    pub unforecast: Vec<UnforecastStream>,
}

/// The series keys of the transactions in each stream matching `keep`.
fn series<'a>(
    streams: &'a [RecurringStream],
    transactions: &[Transaction],
    keep: impl Fn(&RecurringStream) -> bool,
) -> Vec<(&'a RecurringStream, BTreeSet<String>)> {
    streams
        .iter()
        .filter(|stream| keep(stream))
        .map(|stream| {
            let keys = transactions
                .iter()
                .filter(|transaction| stream.transaction_ids.contains(&transaction.id))
                .map(recurring_key)
                .collect();
            (stream, keys)
        })
        .collect()
}

/// Series Plaid reports ended: every stream covering them is inactive and tombstoned.
pub fn ended(streams: &[RecurringStream], transactions: &[Transaction]) -> BTreeSet<String> {
    let live = series(streams, transactions, |stream| stream.is_active)
        .into_iter()
        .flat_map(|(_, keys)| keys)
        .collect::<BTreeSet<_>>();
    series(streams, transactions, |stream| {
        !stream.is_active && stream.status == "TOMBSTONED"
    })
    .into_iter()
    .flat_map(|(_, keys)| keys)
    .filter(|key| !live.contains(key))
    .collect()
}

pub fn check(
    streams: &[RecurringStream],
    expected: &[ExpectedEvent],
    transactions: &[Transaction],
) -> RecurringCheck {
    let forecast = expected
        .iter()
        .map(|event| event.recurring_id.clone())
        .collect::<BTreeSet<_>>();
    let mut unforecast = series(streams, transactions, |stream| {
        stream.is_active && stream.status == "MATURE"
    })
    .into_iter()
    .filter(|(_, keys)| !keys.iter().any(|key| forecast.contains(key)))
    .map(|(stream, _)| UnforecastStream {
        stream_id: stream.stream_id.clone(),
        account_id: stream.account_id.clone(),
        name: stream
            .merchant_name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| stream.description.clone()),
        frequency: stream.frequency.clone(),
        predicted_next_date: stream.predicted_next_date.clone(),
        average_amount: stream.average_amount.map(|amount| -amount),
    })
    .collect::<Vec<_>>();
    unforecast.sort_by(|left, right| left.name.cmp(&right.name));
    RecurringCheck {
        available: !streams.is_empty(),
        ended: ended(streams, transactions).into_iter().collect(),
        unforecast,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn charge(id: &str, merchant: &str) -> Transaction {
        serde_json::from_value(json!({
            "id": id, "merchant": merchant, "category": "Entertainment", "date": "2026-08-05",
            "amount": -12.0, "account": "Card", "accountId": "plaid:card", "pending": false,
            "description": null, "categorySource": "provider",
            "classification": { "mark": "initial", "kind": "expense", "spending": true, "credit": false, "zelle": false }
        }))
        .unwrap()
    }

    fn stream(id: &str, status: &str, active: bool, ids: &[&str]) -> RecurringStream {
        RecurringStream {
            stream_id: id.into(),
            account_id: "plaid:card".into(),
            direction: "out".into(),
            description: id.into(),
            merchant_name: Some(id.into()),
            frequency: "MONTHLY".into(),
            status: status.into(),
            is_active: active,
            last_date: Some("2026-08-05".into()),
            predicted_next_date: Some("2026-09-05".into()),
            average_amount: Some(12.0),
            transaction_ids: ids.iter().map(|id| (*id).into()).collect(),
        }
    }

    #[test]
    fn ends_only_series_every_stream_reports_ended_and_lists_missing_ones() {
        let transactions = [
            charge("a", "Old Stream"),
            charge("b", "Gym"),
            charge("c", "Music"),
        ];
        let streams = [
            stream("old", "TOMBSTONED", false, &["a"]),
            stream("gym", "MATURE", true, &["b"]),
            // A tombstoned stream whose series another stream still sees as live stays.
            stream("music-old", "TOMBSTONED", false, &["c"]),
            stream("music", "MATURE", true, &["c"]),
        ];
        let ended = ended(&streams, &transactions);
        assert_eq!(
            ended.into_iter().collect::<Vec<_>>(),
            [recurring_key(&transactions[0])]
        );

        let result = check(&streams, &[], &transactions);
        assert!(result.available);
        let names = result
            .unforecast
            .iter()
            .map(|stream| stream.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["gym", "music"]);
        assert_eq!(result.unforecast[0].average_amount, Some(-12.0));
    }
}

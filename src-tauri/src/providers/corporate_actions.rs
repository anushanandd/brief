//! Splits and cash dividends from Alpaca's corporate actions, as committed with a refresh.
//! Splits restate lots and gains; dividends are events and expected income.
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::alpaca::stock_symbol;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CorporateAction {
    /// On `ex_date` every `old_rate` shares became `new_rate` shares.
    Split {
        symbol: String,
        ex_date: NaiveDate,
        old_rate: f64,
        new_rate: f64,
    },
    /// `rate` dollars per share to holders before `ex_date`.
    Dividend {
        symbol: String,
        ex_date: NaiveDate,
        payable_date: Option<NaiveDate>,
        rate: f64,
        #[serde(default)]
        special: bool,
        #[serde(default = "usd")]
        currency: String,
    },
}

fn usd() -> String {
    "USD".into()
}

impl CorporateAction {
    pub fn symbol(&self) -> &str {
        match self {
            Self::Split { symbol, .. } | Self::Dividend { symbol, .. } => symbol,
        }
    }

    pub fn ex_date(&self) -> NaiveDate {
        match self {
            Self::Split { ex_date, .. } | Self::Dividend { ex_date, .. } => *ex_date,
        }
    }

    /// Splits before dividends on the same day, so records sort and deduplicate stably.
    fn order(&self) -> (&str, NaiveDate, u8) {
        let kind = match self {
            Self::Split { .. } => 0,
            Self::Dividend { .. } => 1,
        };
        (self.symbol(), self.ex_date(), kind)
    }
}

/// Sorted by symbol, ex-date, and kind, with exact repeats removed.
pub fn normalized(mut actions: Vec<CorporateAction>) -> Vec<CorporateAction> {
    actions.sort_by(|left, right| left.order().cmp(&right.order()));
    actions.dedup();
    actions
}

/// Reads saved actions, leaving out any record that no longer reads, so one bad record can
/// never block loading the finance database.
pub fn lenient<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<CorporateAction>, D::Error> {
    Ok(Vec::<Value>::deserialize(deserializer)?
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect())
}

/// Alpaca's `/v1/corporate-actions` page as actions. Records with an invalid symbol, date, or
/// rate are dropped; a split's rates and a dividend's rate must be positive.
pub(super) fn parse(payload: &Value) -> Vec<CorporateAction> {
    let date = |item: &Value, field: &str| {
        item.get(field)
            .and_then(Value::as_str)
            .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
    };
    let positive = |item: &Value, field: &str| {
        item.get(field)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
    };
    let mut actions = Vec::new();
    for (key, split) in [
        ("forward_splits", true),
        ("reverse_splits", true),
        ("cash_dividends", false),
    ] {
        for item in payload
            .pointer(&format!("/corporate_actions/{key}"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(symbol), Some(ex_date)) = (
                item.get("symbol")
                    .and_then(Value::as_str)
                    .and_then(|symbol| stock_symbol(symbol).ok()),
                date(item, "ex_date"),
            ) else {
                continue;
            };
            let action = if split {
                let (Some(old_rate), Some(new_rate)) =
                    (positive(item, "old_rate"), positive(item, "new_rate"))
                else {
                    continue;
                };
                CorporateAction::Split {
                    symbol,
                    ex_date,
                    old_rate,
                    new_rate,
                }
            } else {
                let Some(rate) = positive(item, "rate") else {
                    continue;
                };
                CorporateAction::Dividend {
                    symbol,
                    ex_date,
                    payable_date: date(item, "payable_date"),
                    rate,
                    special: item
                        .get("special")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    currency: item
                        .get("currency")
                        .and_then(Value::as_str)
                        .unwrap_or("USD")
                        .to_owned(),
                }
            };
            actions.push(action);
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_valid_splits_and_dividends_in_the_saved_shape() {
        let payload = json!({ "corporate_actions": {
            "forward_splits": [
                { "symbol": "TEST", "old_rate": 1, "new_rate": 4, "ex_date": "2026-06-10" },
                { "symbol": "BAD SYMBOL", "old_rate": 1, "new_rate": 2, "ex_date": "2026-06-10" }
            ],
            "reverse_splits": [{ "symbol": "REV", "old_rate": 10, "new_rate": 1, "ex_date": "not-a-date" }],
            "cash_dividends": [
                { "symbol": "test", "rate": 0.5, "special": true, "ex_date": "2026-08-01", "payable_date": "2026-08-10", "currency": "USD" },
                { "symbol": "ZERO", "rate": 0, "ex_date": "2026-08-01" }
            ]
        }});
        assert_eq!(
            serde_json::to_value(parse(&payload)).unwrap(),
            json!([
                { "kind": "split", "symbol": "TEST", "exDate": "2026-06-10", "oldRate": 1.0, "newRate": 4.0 },
                { "kind": "dividend", "symbol": "TEST", "exDate": "2026-08-01", "payableDate": "2026-08-10", "rate": 0.5, "special": true, "currency": "USD" },
            ])
        );
    }

    #[test]
    fn saved_records_that_no_longer_read_are_left_out() {
        #[derive(Deserialize)]
        struct Saved {
            #[serde(deserialize_with = "lenient")]
            actions: Vec<CorporateAction>,
        }
        let saved: Saved = serde_json::from_value(json!({ "actions": [
            { "kind": "split", "symbol": "TEST", "exDate": "2026-06-10", "oldRate": 1.0, "newRate": 4.0 },
            { "kind": "merger", "symbol": "TEST", "exDate": "2026-06-10" },
            { "kind": "dividend", "symbol": "TEST", "exDate": "2026-08-01", "payableDate": null, "rate": 0.5, "currency": "USD" }
        ]}))
        .unwrap();
        assert_eq!(saved.actions.len(), 2);
    }

    #[test]
    fn normalized_sorts_and_removes_repeats() {
        let split = CorporateAction::Split {
            symbol: "B".into(),
            ex_date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
            old_rate: 1.0,
            new_rate: 2.0,
        };
        let dividend = CorporateAction::Dividend {
            symbol: "A".into(),
            ex_date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
            payable_date: None,
            rate: 0.1,
            special: false,
            currency: "USD".into(),
        };
        assert_eq!(
            normalized(vec![split.clone(), dividend.clone(), split.clone()]),
            vec![dividend, split]
        );
    }
}

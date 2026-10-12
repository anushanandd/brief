//! Replays calls recorded from the renderer implementation these modules replaced, so the
//! native port is held to the same results on every recorded scenario. The fixture is
//! synthetic test data and is never regenerated from real financial data.
use chrono::NaiveDate;
use serde_json::{json, Value};

use super::benefits::{credit_summary, PLATINUM};
use super::calendar::{self, NamedZone, Zone};
use super::{forecast, statement, AccountView};
use crate::finance_contract::Transaction;

fn transaction(value: &Value) -> Transaction {
    let mut value = value.clone();
    for (field, fallback) in [("account", json!("")), ("pending", json!(false))] {
        if value.get(field).is_none() {
            value[field] = fallback;
        }
    }
    serde_json::from_value(value).expect("recorded transaction")
}

fn transactions(value: &Value) -> Vec<Transaction> {
    value.as_array().unwrap().iter().map(transaction).collect()
}

fn date(value: &Value) -> NaiveDate {
    NaiveDate::parse_from_str(value.as_str().unwrap(), "%Y-%m-%d").unwrap()
}

fn close(left: &Value, right: f64) -> bool {
    left.as_f64()
        .is_some_and(|left| (left - right).abs() < 1e-9)
}

fn optional_close(left: &Value, right: Option<f64>) -> bool {
    match right {
        None => left.is_null(),
        Some(right) => close(left, right),
    }
}

fn reference_today(record: &Value) -> NaiveDate {
    let data = &record["args"][0];
    record["args"]
        .get(1)
        .filter(|value| !value.is_null())
        .map(date)
        .unwrap_or_else(|| date(&json!(data["updatedAt"].as_str().unwrap()[..10])))
}

fn check(record: &Value, zone: &dyn Zone) -> Result<(), String> {
    let args = &record["args"];
    let expected = &record["result"];
    let mismatch = |actual: Value| Err(format!("expected {expected}\n  actual {actual}"));
    match record["name"].as_str().unwrap() {
        "transactionDateKey" => {
            let actual = calendar::transaction_date(
                args[0].as_str().unwrap(),
                args[1].as_str().unwrap(),
                zone,
            )
            .map(calendar::iso)
            .unwrap_or_default();
            if expected != &json!(actual) {
                return mismatch(json!(actual));
            }
        }
        "recurringMoneyKey" => {
            let actual = forecast::recurring_key(&transaction(&args[0]), zone);
            if expected != &json!(actual) {
                return mismatch(json!(actual));
            }
        }
        "platinumBenefitCreditIdentity" => {
            let actual = PLATINUM
                .credit_identity(&transaction(&args[0]), zone)
                .map(|(id, name)| json!({ "id": id, "name": name }))
                .unwrap_or(Value::Null);
            if expected != &actual {
                return mismatch(actual);
            }
        }
        "getPlatinumBenefitActivity" => {
            let items = transactions(&args[0]);
            let refs = items.iter().collect::<Vec<_>>();
            let actual = PLATINUM.activity(&refs, args[1].as_str().unwrap(), zone);
            let shape = |id: &Value, date: &Value, benefit: &Value, kind: &Value| {
                json!([id, date, benefit, kind])
            };
            let expected = expected
                .as_array()
                .unwrap()
                .iter()
                .map(|item| {
                    shape(
                        &item["id"],
                        &item["date"],
                        &item["benefitId"],
                        &item["kind"],
                    )
                })
                .collect::<Vec<_>>();
            let actual = actual
                .iter()
                .map(|item| {
                    shape(
                        &json!(item.id),
                        &json!(calendar::iso(item.date)),
                        &json!(item.benefit_id),
                        &serde_json::to_value(item.kind).unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            if expected != actual {
                return Err(format!("expected {expected:?}\n  actual {actual:?}"));
            }
        }
        "expectedMoneyEvents" => {
            let data = &args[0];
            let accounts = data["accounts"]
                .as_array()
                .map(|accounts| {
                    accounts
                        .iter()
                        .map(|account| {
                            serde_json::from_value::<AccountView>(account.clone()).unwrap()
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let actual = forecast::expected_events(
                &transactions(&data["transactions"]),
                &accounts,
                data["updatedAt"].as_str().unwrap(),
                reference_today(record),
                zone,
            );
            let actual = actual.into_iter().take(16).collect::<Vec<_>>();
            let expected = expected.as_array().unwrap();
            if expected.len() != actual.len() {
                return mismatch(serde_json::to_value(&actual).unwrap());
            }
            for (expected, actual) in expected.iter().zip(&actual) {
                let actual_json = serde_json::to_value(actual).unwrap();
                let same = [
                    "id",
                    "recurringId",
                    "title",
                    "kind",
                    "date",
                    "frequency",
                    "lastSeen",
                    "state",
                ]
                .iter()
                .all(|field| expected[field] == actual_json[field])
                    && expected["observations"] == json!(actual.observations)
                    && expected["accountId"]
                        == actual_json.get("accountId").cloned().unwrap_or(Value::Null)
                    && expected["matchedTransactionId"]
                        == actual_json
                            .get("matchedTransactionId")
                            .cloned()
                            .unwrap_or(Value::Null)
                    && close(&expected["amount"], actual.amount)
                    && close(&expected["amountLow"], actual.amount_low)
                    && close(&expected["amountHigh"], actual.amount_high);
                if !same {
                    return Err(format!("expected {expected}\n  actual {actual_json}"));
                }
            }
        }
        "buildPlatinumBenefitTracker" => {
            let items = transactions(&args[0]);
            let refs = items.iter().collect::<Vec<_>>();
            let reference = args[1].as_str().unwrap();
            let snapshot = args.get(2).and_then(Value::as_str).unwrap_or(reference);
            let actual = PLATINUM.tracker(&refs, reference, snapshot, zone);
            let expected = expected.as_array().unwrap();
            if expected.len() != actual.len() {
                return mismatch(serde_json::to_value(&actual).unwrap());
            }
            for (expected, actual) in expected.iter().zip(&actual) {
                let actual_json = serde_json::to_value(actual).unwrap();
                let fields = [
                    "id",
                    "name",
                    "limit",
                    "cadence",
                    "instruction",
                    "url",
                    "periodUncertain",
                    "snapshotBeforeWindow",
                    "postingDelayDays",
                    "status",
                    "windowStart",
                    "windowEnd",
                    "windowLabel",
                    "daysRemaining",
                    "reset",
                ];
                let optional = ["deadlineNote", "renewalCheck"];
                let last_credit = match &actual.last_credit {
                    None => expected["lastCredit"].is_null(),
                    Some(last) => {
                        expected["lastCredit"]["date"] == json!(last.date)
                            && close(&expected["lastCredit"]["amount"], last.amount)
                    }
                };
                let activity_ids = |value: &Value| {
                    value
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|item| (item["id"].clone(), item["kind"].clone()))
                        .collect::<Vec<_>>()
                };
                let same = last_credit
                    && fields
                        .iter()
                        .all(|field| expected[field] == actual_json[field])
                    && optional.iter().all(|field| {
                        expected[field] == actual_json.get(*field).cloned().unwrap_or(Value::Null)
                    })
                    && close(&expected["cap"], actual.cap)
                    && close(&expected["creditedAmount"], actual.credited_amount)
                    && optional_close(
                        &expected["estimatedCreditAmount"],
                        actual.estimated_credit_amount,
                    )
                    && optional_close(&expected["remainingAmount"], actual.remaining_amount)
                    && activity_ids(&expected["activity"])
                        == activity_ids(&actual_json["activity"]);
                if !same {
                    return Err(format!("expected {expected}\n  actual {actual_json}"));
                }
            }
        }
        "recognizedAmexAutopays" => {
            let items = transactions(&args[0]);
            let refs = items.iter().collect::<Vec<_>>();
            let actual = statement::autopays(&refs, args[1].as_str().unwrap(), zone);
            let expected_items = expected.as_array().unwrap();
            let same = expected_items.len() == actual.len()
                && expected_items
                    .iter()
                    .zip(&actual)
                    .all(|(expected, actual)| {
                        expected["date"] == json!(calendar::iso(actual.date))
                            && close(&expected["amount"], actual.amount)
                    });
            if !same {
                return mismatch(serde_json::to_value(actual).unwrap());
            }
        }
        "autopayStatementBoundaries" => {
            let items = transactions(&args[0]);
            let refs = items.iter().collect::<Vec<_>>();
            let actual = serde_json::to_value(statement::autopay_boundaries(
                &refs,
                args[1].as_str().unwrap(),
                zone,
            ))
            .unwrap();
            if expected != &actual {
                return mismatch(actual);
            }
        }
        "buildPlatinumCreditSummary" => {
            let items = transactions(&args[0]);
            let refs = items.iter().collect::<Vec<_>>();
            let reference = args[1].as_str().unwrap();
            let (start, end) = (date(&args[2]), date(&args[3]));
            let activity = PLATINUM.activity(&refs, reference, zone);
            let outcomes = PLATINUM.window_outcomes(&refs, start, reference, zone);
            let (earned, missed) = credit_summary(&activity, &outcomes, start, end);
            if !close(&expected["earned"], earned) || !close(&expected["missed"], missed) {
                return mismatch(json!({ "earned": earned, "missed": missed }));
            }
        }
        name => return Err(format!("unknown recorded function {name}")),
    }
    Ok(())
}

#[test]
fn native_insights_reproduce_every_recorded_renderer_result() {
    let fixture = include_str!("../../../src/data/fixtures/insights-golden.jsonl");
    let mut failures = Vec::new();
    let mut checked = 0;
    for (line, text) in fixture.lines().enumerate() {
        let record: Value = serde_json::from_str(text).unwrap();
        let zone = NamedZone::parse(record["tz"].as_str().unwrap()).expect("recorded zone");
        checked += 1;
        if let Err(error) = check(&record, &zone) {
            failures.push(format!(
                "line {} {} {}: {error}",
                line + 1,
                record["name"],
                record["tz"]
            ));
        }
    }
    assert!(checked > 800, "fixture is unexpectedly small");
    assert!(
        failures.is_empty(),
        "{} of {checked} recorded results differ:\n{}",
        failures.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

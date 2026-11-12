//! Replays calls recorded from the renderer implementation these modules replaced, so the
//! native port is held to the same results on every recorded scenario. The fixture is
//! synthetic test data and is never regenerated, only edited by hand where a rule changes on
//! purpose.
//!
//! The records that exercise a real card's benefit rules are kept with those rules, outside
//! the repository (`notes/`), since the rules are the user's own data; they run when present.
use chrono::NaiveDate;
use serde_json::{json, Value};

use super::benefits::{credit_summary, CardRules, Catalog};
use super::calendar::{self, NamedZone, Zone};
use super::{forecast, statement, AccountView};
use crate::finance_contract::Transaction;

/// A recorded transaction, with the benefit credit the transaction policy would assign it.
fn transaction(value: &Value, cards: &Catalog) -> Transaction {
    let mut value = value.clone();
    for (field, fallback) in [("account", json!("")), ("pending", json!(false))] {
        if value.get(field).is_none() {
            value[field] = fallback;
        }
    }
    let mut transaction: Transaction = serde_json::from_value(value).expect("recorded transaction");
    transaction.benefit_credit = (transaction.amount > 0.0)
        .then(|| cards.credit_identity(&transaction))
        .flatten();
    transaction
}

fn transactions(value: &Value, cards: &Catalog) -> Vec<Transaction> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| transaction(item, cards))
        .collect()
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

fn check(record: &Value, zone: &dyn Zone, cards: &Catalog) -> Result<(), String> {
    let args = &record["args"];
    let expected = &record["result"];
    let mismatch = |actual: Value| Err(format!("expected {expected}\n  actual {actual}"));
    let card = || {
        cards
            .only()
            .ok_or_else(|| "a card record needs exactly one card's rules".to_owned())
    };
    match record["name"].as_str().unwrap() {
        "transactionDateKey" => {
            let actual = calendar::transaction_date(args[0].as_str().unwrap())
                .map(calendar::iso)
                .unwrap_or_default();
            if expected != &json!(actual) {
                return mismatch(json!(actual));
            }
        }
        "recurringMoneyKey" => {
            let actual = forecast::recurring_key(&transaction(&args[0], cards));
            if expected != &json!(actual) {
                return mismatch(json!(actual));
            }
        }
        "cardBenefitCreditIdentity" => {
            let actual = card()?
                .credit_identity(&transaction(&args[0], cards))
                .map(|(id, name)| json!({ "id": id, "name": name }))
                .unwrap_or(Value::Null);
            if expected != &actual {
                return mismatch(actual);
            }
        }
        "getCardBenefitActivity" => {
            let items = transactions(&args[0], cards);
            let refs = items.iter().collect::<Vec<_>>();
            let actual = card()?.activity(&refs, args[1].as_str().unwrap());
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
                &transactions(&data["transactions"], cards),
                &accounts,
                data["updatedAt"].as_str().unwrap(),
                reference_today(record),
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
        "buildCardBenefitTracker" => {
            let items = transactions(&args[0], cards);
            let refs = items.iter().collect::<Vec<_>>();
            let reference = args[1].as_str().unwrap();
            let snapshot = args.get(2).and_then(Value::as_str).unwrap_or(reference);
            let actual = card()?.tracker(&refs, reference, snapshot, zone);
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
        "recognizedAutopays" => {
            let items = transactions(&args[0], cards);
            let refs = items.iter().collect::<Vec<_>>();
            let actual = statement::autopays(&refs, args[1].as_str().unwrap());
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
            let items = transactions(&args[0], cards);
            let refs = items.iter().collect::<Vec<_>>();
            let actual = serde_json::to_value(statement::autopay_boundaries(
                &refs,
                args[1].as_str().unwrap(),
            ))
            .unwrap();
            if expected != &actual {
                return mismatch(actual);
            }
        }
        "buildCardCreditSummary" => {
            let items = transactions(&args[0], cards);
            let refs = items.iter().collect::<Vec<_>>();
            let reference = args[1].as_str().unwrap();
            let (start, end) = (date(&args[2]), date(&args[3]));
            let card = card()?;
            let activity = card.activity(&refs, reference);
            let history_start = refs
                .iter()
                .filter_map(|transaction| calendar::transaction_date(transaction.posted_date()))
                .min();
            let history = card.history(
                &activity,
                history_start,
                calendar::transaction_date(reference).unwrap(),
                &[],
            );
            let (earned, missed) = credit_summary(&activity, &history, start, end);
            if !close(&expected["earned"], earned) || !close(&expected["missed"], missed) {
                return mismatch(json!({ "earned": earned, "missed": missed }));
            }
        }
        name => return Err(format!("unknown recorded function {name}")),
    }
    Ok(())
}

/// Checks every record, failing with the first few that differ.
fn replay(fixture: &str, cards: &Catalog, at_least: usize) {
    let mut failures = Vec::new();
    let mut checked = 0;
    for (line, text) in fixture.lines().enumerate() {
        let record: Value = serde_json::from_str(text).unwrap();
        let zone = NamedZone::parse(record["tz"].as_str().unwrap()).expect("recorded zone");
        checked += 1;
        if let Err(error) = check(&record, &zone, cards) {
            failures.push(format!(
                "line {} {} {}: {error}",
                line + 1,
                record["name"],
                record["tz"]
            ));
        }
    }
    assert!(checked >= at_least, "fixture is unexpectedly small");
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

#[test]
fn native_insights_reproduce_every_recorded_renderer_result() {
    replay(
        include_str!("../../../src/data/fixtures/insights-golden.jsonl"),
        &Catalog::example(),
        580,
    );
}

/// The records for a real card, read with that card's rules, when this Mac keeps both:
/// `notes/golden-card-benefits.jsonl` and one rules file in `notes/card-rules/`.
#[test]
fn private_card_records_reproduce_with_their_rules() {
    let notes = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../notes");
    let Ok(fixture) = std::fs::read_to_string(notes.join("golden-card-benefits.jsonl")) else {
        eprintln!("no private card records on this Mac; skipped");
        return;
    };
    let rules = std::fs::read_dir(notes.join("card-rules"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .map(|entry| {
            let text = std::fs::read_to_string(entry.path()).expect("readable card rules");
            serde_json::from_str::<CardRules>(&text).expect("valid card rules")
        })
        .collect::<Vec<_>>();
    replay(
        &fixture,
        &Catalog::new(&rules).expect("card rules compile"),
        1,
    );
}

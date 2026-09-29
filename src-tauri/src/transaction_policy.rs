//! Financial transaction meaning is assigned here, after saved overrides.
use crate::{finance_contract::Transaction, storage::Annotation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    Transfer,
    Interest,
    Dividend,
    Income,
    Refund,
    Fee,
    Payment,
    Cash,
    Initial,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Income,
    Dividend,
    Interest,
    Fee,
    Tax,
    Reimbursement,
    Transfer,
    Expense,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub mark: Mark,
    pub kind: Kind,
    pub spending: bool,
    #[serde(default)]
    pub credit: bool,
    pub zelle: bool,
}
fn contains(text: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| text.contains(term))
}
fn word(text: &str, terms: &[&str]) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|part| terms.contains(&part))
}
pub fn classify(t: &Transaction) -> Classification {
    let category = t.category.to_lowercase();
    let merchant = t.merchant.to_lowercase();
    let text = format!("{category} {merchant}");
    let zelle = word(
        &format!(
            "{merchant} {}",
            t.description.as_deref().unwrap_or("").to_lowercase()
        ),
        &["zelle"],
    );
    let income_override = t.amount > 0.0 && zelle;
    let spending = t.amount < 0.0 && !contains(&text, &["income", "transfer", "payment"]);
    let mark = if income_override {
        Mark::Income
    } else if category == "transfer" {
        Mark::Transfer
    } else if category == "reimbursement" {
        Mark::Refund
    } else if text.contains("interest") || word(&text, &["apy"]) {
        Mark::Interest
    } else if contains(
        &text,
        &["dividend", "capital gain", "investment distribution"],
    ) {
        Mark::Dividend
    } else if category == "income" {
        Mark::Income
    } else if contains(
        &text,
        &[
            "refund",
            "reimbursement",
            "cashback",
            "cash back",
            "reversal",
            "credit adjustment",
        ],
    ) {
        Mark::Refund
    } else if word(&text, &["fee", "fees"]) || contains(&text, &["overdraft", "service charge"]) {
        Mark::Fee
    } else if text.contains("cash withdrawal") || word(&text, &["atm"]) {
        Mark::Cash
    } else if contains(
        &text,
        &[
            "credit card payment",
            "loan payment",
            "autopay",
            "bill payment",
        ],
    ) {
        Mark::Payment
    } else if text.contains("transfer") || word(&text, &["wire", "ach"]) {
        Mark::Transfer
    } else if contains(
        &text,
        &["income", "paycheck", "salary", "payroll", "direct deposit"],
    ) {
        Mark::Income
    } else {
        Mark::Initial
    };
    let category = category.trim();
    let detail = format!(
        "{text} {} {}",
        t.category_detail.as_deref().unwrap_or(""),
        t.description.as_deref().unwrap_or("")
    )
    .to_lowercase();
    let kind = if income_override {
        Kind::Income
    } else if category == "reimbursement" {
        Kind::Reimbursement
    } else if category == "income" {
        match mark {
            Mark::Interest => Kind::Interest,
            Mark::Dividend => Kind::Dividend,
            _ => Kind::Income,
        }
    } else if category == "transfer" {
        Kind::Transfer
    } else if contains(&detail, &["reimbursement", "shared expense repayment"]) {
        Kind::Reimbursement
    } else if [
        "fee",
        "fees",
        "bank fee",
        "bank fees",
        "investment fee",
        "investment fees",
    ]
    .contains(&category)
    {
        Kind::Fee
    } else if word(&detail, &["tax", "taxes"]) || detail.contains("withholding") {
        Kind::Tax
    } else {
        match mark {
            Mark::Income => Kind::Income,
            Mark::Dividend => Kind::Dividend,
            Mark::Interest => Kind::Interest,
            Mark::Fee => Kind::Fee,
            Mark::Transfer | Mark::Payment => Kind::Transfer,
            _ if spending => Kind::Expense,
            _ => Kind::Other,
        }
    };
    let credit = t.amount > 0.0 && (mark == Mark::Refund || word(&text, &["credit"]));
    Classification {
        credit,
        mark,
        kind,
        spending,
        zelle,
    }
}

/// Also upgrades legacy/recovery views without changing saved observations.
/// An ambiguous legacy account name never establishes account identity.
pub fn apply(
    transactions: &mut [Value],
    accounts: &[Value],
    annotations: &BTreeMap<String, Annotation>,
) -> Result<(), String> {
    let mut names = BTreeMap::<&str, Option<&str>>::new();
    for account in accounts {
        if let (Some(id), Some(name)) = (account["id"].as_str(), account["name"].as_str()) {
            if id != "all" {
                names
                    .entry(name)
                    .and_modify(|id| *id = None)
                    .or_insert(Some(id));
            }
        }
    }
    for value in transactions {
        let mut t: Transaction =
            serde_json::from_value(value.clone()).map_err(|_| "Invalid saved transaction")?;
        if let Some(a) = annotations.get(&t.id) {
            if let Some(category) = &a.category {
                t.category = category.clone();
                value["category"] = category.clone().into();
            }
            if let Some(confirmed) = a.benefit_confirmed {
                t.benefit_confirmed = Some(confirmed);
                value["benefitConfirmed"] = confirmed.into();
            }
        }
        if t.account_id.is_none() {
            if let Some(Some(id)) = names.get(t.account.as_str()) {
                value["accountId"] = (*id).into();
            }
        }
        value["classification"] =
            serde_json::to_value(classify(&t)).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn transaction(merchant: &str, category: &str, amount: f64) -> Transaction {
        serde_json::from_value(
            json!({"id":"synthetic", "merchant":merchant, "category":category,
            "amount":amount, "account":"Example", "date":"2026-09-18", "pending":false}),
        )
        .unwrap()
    }

    #[test]
    fn financial_policy_handles_overrides_signs_and_reimbursements() {
        for (merchant, category, amount, mark, kind, spending) in [
            (
                "Zelle receipt",
                "Transfer",
                100.0,
                Mark::Income,
                Kind::Income,
                false,
            ),
            (
                "Zelle payment",
                "Transfer",
                -100.0,
                Mark::Transfer,
                Kind::Transfer,
                false,
            ),
            (
                "Zelle receipt",
                "Reimbursement",
                100.0,
                Mark::Income,
                Kind::Income,
                false,
            ),
            (
                "INTEREST",
                "Income",
                10.0,
                Mark::Interest,
                Kind::Interest,
                false,
            ),
            (
                "Dividend TEST",
                "Income",
                10.0,
                Mark::Dividend,
                Kind::Dividend,
                false,
            ),
            (
                "Example payroll",
                "Income",
                10.0,
                Mark::Income,
                Kind::Income,
                false,
            ),
            (
                "Airline fee reimbursement",
                "Bank Fees",
                25.0,
                Mark::Refund,
                Kind::Reimbursement,
                false,
            ),
            ("Annual fee", "Bank Fees", -25.0, Mark::Fee, Kind::Fee, true),
            (
                "Bank fee reversal",
                "Fees",
                4.0,
                Mark::Refund,
                Kind::Fee,
                false,
            ),
            (
                "Tax withholding",
                "Other",
                -10.0,
                Mark::Initial,
                Kind::Tax,
                true,
            ),
            ("Cafe", "Dining", -20.0, Mark::Initial, Kind::Expense, true),
            ("Cafe", "Dining", 0.0, Mark::Initial, Kind::Other, false),
            (
                "AUTOPAY PAYMENT",
                "Loan Payments",
                -20.0,
                Mark::Payment,
                Kind::Transfer,
                false,
            ),
            (
                "Transfer money from a brokerage account",
                "Transfer",
                500.0,
                Mark::Transfer,
                Kind::Transfer,
                false,
            ),
            (
                "Shared expense repayment",
                "Other",
                20.0,
                Mark::Initial,
                Kind::Reimbursement,
                false,
            ),
        ] {
            let result = classify(&transaction(merchant, category, amount));
            assert_eq!(
                (result.mark, result.kind, result.spending),
                (mark, kind, spending),
                "{merchant}"
            );
        }
        let mut t = transaction("Example", "Other", 10.0);
        t.description = Some("ZELLE receipt".into());
        assert_eq!(classify(&t).kind, Kind::Income);
        t.description = Some("NotZelle receipt".into());
        assert!(!classify(&t).zelle);
    }

    #[test]
    fn annotations_reclassify_and_legacy_identity_requires_unique_name() {
        let original = serde_json::to_value(transaction("Cafe", "Dining", -20.0)).unwrap();
        let mut values = vec![original.clone()];
        let accounts = vec![json!({"id":"one", "name":"Example"})];
        let annotations = BTreeMap::from([(
            "synthetic".into(),
            Annotation {
                category: Some("Transfer".into()),
                ..Default::default()
            },
        )]);
        apply(&mut values, &accounts, &annotations).unwrap();
        assert_eq!(values[0]["classification"]["kind"], "transfer");
        assert_eq!(values[0]["classification"]["spending"], false);
        assert_eq!(values[0]["accountId"], "one");
        let mut values = vec![original];
        apply(
            &mut values,
            &[accounts[0].clone(), json!({"id":"two", "name":"Example"})],
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(values[0].get("accountId").is_none());
        assert_eq!(values[0]["classification"]["kind"], "expense");
    }

    #[test]
    fn browser_seed_contains_native_transaction_policy() {
        let seed: Value = serde_json::from_str(include_str!("../../src/data/seed.json")).unwrap();
        let mut projected = seed.clone();
        let accounts = seed["accounts"].as_array().unwrap();
        apply(
            projected["transactions"].as_array_mut().unwrap(),
            accounts,
            &BTreeMap::new(),
        )
        .unwrap();
        if std::env::var_os("BRIEF_UPDATE_CONTRACT_FIXTURE").is_some() {
            std::fs::write(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/data/seed.json"),
                format!("{}\n", serde_json::to_string_pretty(&projected).unwrap()),
            )
            .unwrap();
        } else {
            assert_eq!(
                projected, seed,
                "Regenerate the synthetic browser seed with BRIEF_UPDATE_CONTRACT_FIXTURE=1"
            );
        }
    }
}

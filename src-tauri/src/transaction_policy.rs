//! Financial transaction meaning is assigned here, after saved overrides.
use crate::{
    finance_contract::Transaction, financial_engine::categories::CodeKind, rules::CategoryRule,
    storage::Annotation,
};
use serde::{Deserialize, Serialize};
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
    /// Someone else's money: a purchase made for them, or what they paid you back. Neither is
    /// your spending or your income.
    Owed,
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
/// The longest gap between two legs of one transfer, covering ACH across a weekend.
pub(crate) const TRANSFER_SETTLEMENT_DAYS: i64 = 5;

/// Money sent to other people through payment apps. It is spending, mirroring how money
/// received through Zelle counts as income; a manual or rule category still wins.
pub const PEOPLE_CATEGORY: &str = "Payments To People";

fn mentions_zelle(t: &Transaction) -> bool {
    word(
        &format!(
            "{} {}",
            t.merchant.to_lowercase(),
            t.description.as_deref().unwrap_or("").to_lowercase()
        ),
        &["zelle"],
    )
}

/// Whether a provider-categorized outflow is a Zelle payment Plaid filed as a transfer.
/// Payment-app transfers Plaid recognizes already map to the people category.
fn payment_to_person(t: &Transaction) -> bool {
    t.amount < 0.0
        && ["transfer out", "transfers"].contains(&t.category.to_lowercase().as_str())
        && mentions_zelle(t)
}

fn word(text: &str, terms: &[&str]) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|part| terms.contains(&part))
}
pub fn classify(t: &Transaction) -> Classification {
    if t.amount > 0.0 && crate::insights::benefits::PLATINUM.is_issuer_credit(&t.descriptors()) {
        // A card statement credit repays benefit spending, whatever Plaid's category says.
        return Classification {
            mark: Mark::Refund,
            kind: Kind::Reimbursement,
            spending: false,
            credit: true,
            zelle: false,
        };
    }
    classify_by_code(t).unwrap_or_else(|| classify_by_name(t))
}

/// Plaid's detailed code decides the meaning of a provider-categorized transaction. Refunds
/// and credits in spending categories, unknown codes, and manual or rule categories (whose
/// detail is cleared before classification) use the name and text policy below.
fn classify_by_code(t: &Transaction) -> Option<Classification> {
    let code = t.category_detail.as_deref()?;
    let kind = crate::financial_engine::categories::kind_for(code)?;
    let zelle = mentions_zelle(t);
    let outflow = t.amount < 0.0;
    let to_person = t.category == PEOPLE_CATEGORY;
    let (mark, kind, spending) = match kind {
        _ if t.amount > 0.0 && zelle => (Mark::Income, Kind::Income, false),
        _ if to_person && outflow => (Mark::Transfer, Kind::Expense, true),
        CodeKind::Income => (Mark::Income, Kind::Income, false),
        CodeKind::Interest => (Mark::Interest, Kind::Interest, false),
        CodeKind::Dividend => (Mark::Dividend, Kind::Dividend, false),
        CodeKind::Transfer if code == "TRANSFER_OUT_WITHDRAWAL" => {
            (Mark::Cash, Kind::Transfer, false)
        }
        CodeKind::Transfer => (Mark::Transfer, Kind::Transfer, false),
        CodeKind::Payment => (Mark::Payment, Kind::Transfer, false),
        CodeKind::Fee if outflow => (Mark::Fee, Kind::Fee, true),
        CodeKind::Tax if outflow => (Mark::Initial, Kind::Tax, true),
        CodeKind::Expense if outflow => (Mark::Initial, Kind::Expense, true),
        _ => return None,
    };
    Some(Classification {
        mark,
        kind,
        spending,
        credit: false,
        zelle,
    })
}

fn classify_by_name(t: &Transaction) -> Classification {
    let category = t.category.to_lowercase();
    let merchant = t.merchant.to_lowercase();
    let text = format!("{category} {merchant}");
    let zelle = mentions_zelle(t);
    let income_override = t.amount > 0.0 && zelle;
    let to_person = category == PEOPLE_CATEGORY.to_lowercase();
    let spending =
        t.amount < 0.0 && (to_person || !contains(&text, &["income", "transfer", "payment"]));
    // Plaid's v2 taxonomy reports loan proceeds separately; they are borrowed money moving
    // between accounts, never income or spending.
    let loan_disbursement = category == "loan disbursements";
    let mark = if income_override {
        Mark::Income
    } else if category == "transfer" || category == "transfers" || loan_disbursement || to_person {
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
    } else if to_person && spending {
        Kind::Expense
    } else if category == "reimbursement" {
        Kind::Reimbursement
    } else if category == "income" {
        match mark {
            Mark::Interest => Kind::Interest,
            Mark::Dividend => Kind::Dividend,
            _ => Kind::Income,
        }
    } else if category == "transfer" || category == "transfers" || loan_disbursement {
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

/// Where a transaction's category came from: chosen by hand, by a rule, or the provider's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CategorySource {
    Manual,
    Rule,
    Provider,
}

/// Applies saved edits and category rules, then classifies every transaction: on each read,
/// and without edits when a refresh projects. A legacy row that carries only an account name
/// takes that account's ID when exactly one account has the name.
pub fn apply<'a>(
    transactions: &mut [Transaction],
    accounts: impl IntoIterator<Item = (&'a str, &'a str)>,
    annotations: &BTreeMap<String, Annotation>,
    rules: &[CategoryRule],
) {
    let mut names = BTreeMap::<&str, Option<&str>>::new();
    for (id, name) in accounts {
        if id != "all" {
            names
                .entry(name)
                .and_modify(|id| *id = None)
                .or_insert(Some(id));
        }
    }
    let mut refunds = Vec::<(usize, String)>::new();
    for (index, t) in transactions.iter_mut().enumerate() {
        if t.account_id.is_none() {
            if let Some(Some(id)) = names.get(t.account.as_str()) {
                t.account_id = Some((*id).to_owned());
            }
        }
        // A posted transaction keeps edits made while it was pending.
        let annotation = annotations.get(&t.id).or_else(|| {
            t.pending_transaction_id
                .as_ref()
                .and_then(|pending| annotations.get(pending))
        });
        let rule = crate::rules::matching(
            rules,
            &t.merchant,
            t.description.as_deref(),
            t.account_id.as_deref(),
        );
        let (category, source) = match (annotation.and_then(|a| a.category.as_ref()), rule) {
            (Some(category), _) => (category.clone(), CategorySource::Manual),
            (None, Some(rule)) => (rule.category.clone(), CategorySource::Rule),
            (None, None) if payment_to_person(t) => {
                (PEOPLE_CATEGORY.to_owned(), CategorySource::Provider)
            }
            (None, None) => (t.category.clone(), CategorySource::Provider),
        };
        let provider_category = std::mem::replace(&mut t.category, category);
        let edited = source != CategorySource::Provider;
        t.rule_id = rule
            .filter(|_| source == CategorySource::Rule)
            .map(|rule| rule.id.clone());
        t.provider_category = (t.category != provider_category).then_some(provider_category);
        t.category_source = Some(source);
        if edited {
            // Plaid's label describes Plaid's category, not one chosen by hand or rule.
            t.subcategory = None;
        } else if t.provider_category.is_some() {
            // Only a Zelle payment to a person changes a provider category.
            t.subcategory = Some("Zelle".into());
        }
        t.reviewed = annotation
            .and_then(|a| a.reviewed)
            .filter(|reviewed| *reviewed);
        if let Some(confirmed) = annotation.and_then(|a| a.benefit_confirmed) {
            t.benefit_confirmed = Some(confirmed);
        }
        t.tags = annotation.and_then(|a| a.tags.clone()).unwrap_or_default();
        // A split made for another amount (a pending charge that posted differently) no
        // longer divides this one, so the transaction stays whole.
        t.splits = annotation
            .and_then(|a| a.splits.as_ref())
            .filter(|splits| crate::storage::splits_divide(splits, t.amount))
            .cloned()
            .unwrap_or_default();
        t.refund_of = None;
        if let Some(purchase) = annotation.and_then(|a| a.refund_of.as_ref()) {
            refunds.push((index, purchase.clone()));
        }
        // A category chosen by hand or rule is classified by its name, not Plaid's code.
        let mut classification = if edited {
            let detail = t.category_detail.take();
            let classification = classify(t);
            t.category_detail = detail;
            classification
        } else {
            classify(t)
        };
        t.other = annotation.and_then(|a| a.other) == Some(true);
        if t.other {
            owe(&mut classification, t.amount);
        }
        t.classification = Some(classification);
    }
    // A linked refund repays its purchase: it takes that purchase's category, unless one was
    // chosen by hand, and reduces spending instead of counting as income or a transfer.
    for (index, purchase_id) in refunds {
        let Some((category, purchase_other)) = transactions
            .iter()
            .find(|candidate| candidate.id == purchase_id)
            .filter(|purchase| purchase.amount < 0.0)
            .map(|purchase| (purchase.category.clone(), purchase.other))
        else {
            continue;
        };
        let refund = &mut transactions[index];
        if refund.amount <= 0.0 {
            continue;
        }
        refund.refund_of = Some(purchase_id);
        // A refund marked someone else's is already out of your spending.
        if refund.other {
            continue;
        }
        if refund.category_source != Some(CategorySource::Manual) {
            if refund.provider_category.is_none() && refund.category != category {
                refund.provider_category = Some(refund.category.clone());
            }
            refund.category = category;
            refund.subcategory = None;
        }
        if let Some(classification) = refund.classification.as_mut() {
            classification.kind = Kind::Reimbursement;
            classification.mark = Mark::Refund;
            classification.spending = false;
            // A refund of a purchase made for someone else is theirs too, not your spending.
            if purchase_other {
                classification.kind = Kind::Owed;
            }
        }
        refund.other |= purchase_other;
    }
    own_transfers_filed_as_other_kinds(transactions);
}

/// Money moved between two of your accounts is a transfer, even when the provider files one
/// leg as something else: an instant payment to yourself often arrives as contractor income,
/// and a move out of a brokerage's cash can leave as "Other" spending. When one leg is already
/// a transfer, an equal and opposite leg in another account within the settlement window,
/// filed by the provider as income or spending, is its other half. Legs that are both transfers
/// pair first, each leg pairs once, and a category chosen by hand or by rule is never changed.
fn own_transfers_filed_as_other_kinds(transactions: &mut [Transaction]) {
    struct Leg {
        index: usize,
        date: chrono::NaiveDate,
        account: String,
        cents: i64,
        transfer: bool,
    }
    let legs = transactions
        .iter()
        .enumerate()
        .filter_map(|(index, t)| {
            let kind = &t.classification.as_ref()?.kind;
            let transfer = *kind == Kind::Transfer;
            let provider = t.category_source == Some(CategorySource::Provider);
            let cents = crate::numeric::minor_units(t.amount);
            let eligible = transfer
                || (provider
                    && ((*kind == Kind::Income && cents > 0)
                        || (*kind == Kind::Expense && cents < 0)));
            let date = chrono::NaiveDate::parse_from_str(
                t.posted_on.as_deref().unwrap_or(&t.date),
                "%Y-%m-%d",
            )
            .ok()?;
            let account = t.account_id.clone()?;
            (eligible && !t.pending && !t.other && cents != 0).then_some(Leg {
                index,
                date,
                account,
                cents,
                transfer,
            })
        })
        .collect::<Vec<_>>();
    let mut paired = vec![false; legs.len()];
    let mut reclassify = Vec::new();
    // Transfers that already pair claim their legs before any other kind is considered.
    for mixed in [false, true] {
        for out in 0..legs.len() {
            if paired[out] || legs[out].cents > 0 {
                continue;
            }
            let Some(into) = (0..legs.len())
                .filter(|&into| {
                    let (sent, arrived) = (&legs[out], &legs[into]);
                    !paired[into]
                        && arrived.cents == -sent.cents
                        && arrived.account != sent.account
                        && (arrived.date - sent.date).num_days().abs() <= TRANSFER_SETTLEMENT_DAYS
                        && if mixed {
                            sent.transfer != arrived.transfer
                        } else {
                            sent.transfer && arrived.transfer
                        }
                })
                .min_by_key(|&into| {
                    (
                        (legs[into].date - legs[out].date).num_days().abs(),
                        legs[into].date,
                    )
                })
            else {
                continue;
            };
            paired[out] = true;
            paired[into] = true;
            if mixed {
                let (filed, transfer) = if legs[out].transfer {
                    (into, out)
                } else {
                    (out, into)
                };
                reclassify.push((legs[filed].index, legs[transfer].index));
            }
        }
    }
    for (index, transfer_index) in reclassify {
        let category = transactions[transfer_index].category.clone();
        let leg = &mut transactions[index];
        if leg.provider_category.is_none() {
            leg.provider_category = Some(leg.category.clone());
        }
        leg.category = category;
        leg.subcategory = None;
        if let Some(classification) = leg.classification.as_mut() {
            classification.kind = Kind::Transfer;
            classification.mark = Mark::Transfer;
            classification.spending = false;
        }
    }
}

/// Someone else's money: a purchase made for them, or what they paid you back.
pub(crate) fn owe(classification: &mut Classification, amount: f64) {
    classification.kind = Kind::Owed;
    classification.spending = false;
    classification.credit = false;
    if amount > 0.0 {
        classification.mark = Mark::Transfer;
    }
}

#[cfg(test)]
mod tests;

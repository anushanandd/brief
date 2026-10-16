//! Financial transaction meaning is assigned here, after saved overrides.
use crate::{
    finance_contract::Transaction, financial_engine::categories::CodeKind, storage::Annotation,
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
    /// A purchase made for someone else that would otherwise be spending. `spending` leaves it
    /// out; Spending counts it only when it shows someone else's spending, or everyone's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub others_spending: bool,
}
fn contains(text: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| text.contains(term))
}
/// The longest gap between two legs of one transfer, covering ACH across a weekend.
pub(crate) const TRANSFER_SETTLEMENT_DAYS: i64 = 5;

/// Money sent to other people through payment apps. It is spending; money received through
/// one keeps the provider's meaning, so a friend paying you back is not income. A manual or
/// rule category still wins.
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
            others_spending: false,
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
        others_spending: false,
    })
}

fn classify_by_name(t: &Transaction) -> Classification {
    let category = t.category.to_lowercase();
    let merchant = t.merchant.to_lowercase();
    let text = format!("{category} {merchant}");
    let zelle = mentions_zelle(t);
    let to_person = category == PEOPLE_CATEGORY.to_lowercase();
    let spending =
        t.amount < 0.0 && (to_person || !contains(&text, &["income", "transfer", "payment"]));
    // Plaid's v2 taxonomy reports loan proceeds separately; they are borrowed money moving
    // between accounts, never income or spending.
    let loan_disbursement = category == "loan disbursements";
    let mark =
        if category == "transfer" || category == "transfers" || loan_disbursement || to_person {
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
        } else if word(&text, &["fee", "fees"]) || contains(&text, &["overdraft", "service charge"])
        {
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
    let kind = if to_person && spending {
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
        others_spending: false,
    }
}

/// Where a transaction's category came from: chosen by hand, by a rule, or the provider's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CategorySource {
    Manual,
    Provider,
}

/// Where a transaction's people came from: chosen by hand (possibly nobody), or matched by a
/// person's match name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PeopleSource {
    Manual,
    Matched,
}

/// Applies saved edits, then classifies every transaction: on each read, and without edits
/// when a refresh projects. A legacy row that carries only an account name
/// takes that account's ID when exactly one account has the name.
pub fn apply<'a>(
    transactions: &mut [Transaction],
    accounts: impl IntoIterator<Item = (&'a str, &'a str)>,
    annotations: &BTreeMap<String, Annotation>,
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
        let (category, source) = match annotation.and_then(|a| a.category.as_ref()) {
            Some(category) => (category.clone(), CategorySource::Manual),
            None if payment_to_person(t) => (PEOPLE_CATEGORY.to_owned(), CategorySource::Provider),
            None => (t.category.clone(), CategorySource::Provider),
        };
        let provider_category = std::mem::replace(&mut t.category, category);
        let edited = source != CategorySource::Provider;
        t.provider_category = (t.category != provider_category).then_some(provider_category);
        t.category_source = Some(source);
        if edited {
            // Plaid's label describes Plaid's category, not one chosen by hand.
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
        t.benefit_id = annotation.and_then(|a| a.benefit_id.clone());
        // Decided again from Plaid's own name on every read, so the result follows the records.
        if let Some(original) = t.provider_merchant.take() {
            t.merchant = original;
        }
        if let Some(name) = crate::merchants::corrected_name(t) {
            t.provider_merchant = Some(std::mem::replace(&mut t.merchant, name));
            // Plaid's logo and site belong to the merchant it named.
            t.logo_url = None;
            t.logo_name = None;
            t.website = None;
        }
        t.presence = crate::merchants::presence(t);
        // A remote purchase, or one through a processor that prints its own office (a vending
        // machine's), says nothing about where the cardholder was.
        t.place = (t.presence != Some(crate::merchants::Presence::Remote)
            && !crate::merchants::bills_elsewhere(t))
        .then(|| crate::places::resolve(t))
        .flatten();
        t.tags = annotation.and_then(|a| a.tags.clone()).unwrap_or_default();
        t.note = annotation.and_then(|a| a.note.clone());
        // People chosen by hand; matching by name fills the rest once settings are known.
        let people = annotation.and_then(|a| a.people.as_ref());
        t.people = people.cloned().unwrap_or_default();
        t.people_source = people.map(|_| PeopleSource::Manual);
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
    pair_transfers(transactions);
}

/// Pairs the two legs of each move between two of your accounts, so every reader agrees which
/// transactions are one movement of money: net worth counts it once while its legs post on
/// different days, Data health knows a card received its payment, and the renderer links the
/// legs. A leg's other half is an equal and opposite posted leg in another account within the
/// settlement window, the nearest by date, and each leg pairs once.
///
/// The provider sometimes files one leg as something else: an instant payment to yourself can
/// arrive as contractor income, and a move out of a brokerage's cash can leave as spending or
/// "Other". Such a leg pairs only with a leg that is already a transfer, after transfers have
/// paired with each other, and becomes a transfer itself, unless its category was chosen by
/// hand or by rule. Cash withdrawals, money through a payment app (which is with a person), and
/// someone else's money never pair.
fn pair_transfers(transactions: &mut [Transaction]) {
    struct Leg {
        index: usize,
        date: chrono::NaiveDate,
        account: String,
        cents: i64,
        transfer: bool,
    }
    for t in transactions.iter_mut() {
        t.transfer_pair_id = None;
    }
    let mut legs = transactions
        .iter()
        .enumerate()
        .filter_map(|(index, t)| {
            let classification = t.classification.as_ref()?;
            let kind = &classification.kind;
            let cents = crate::numeric::minor_units(t.amount);
            let payment_app = classification.zelle
                || t.category == PEOPLE_CATEGORY
                || t.category_detail
                    .as_deref()
                    .is_some_and(|code| code.ends_with("_FROM_APPS"));
            let transfer = *kind == Kind::Transfer && classification.mark != Mark::Cash;
            let filed = t.category_source == Some(CategorySource::Provider)
                && match kind {
                    Kind::Income => cents > 0,
                    Kind::Expense => cents < 0,
                    Kind::Other => true,
                    _ => false,
                };
            let date = chrono::NaiveDate::parse_from_str(t.posted_date(), "%Y-%m-%d").ok()?;
            let account = t.account_id.clone()?;
            ((transfer || filed) && !payment_app && !t.pending && !t.other && cents != 0).then_some(
                Leg {
                    index,
                    date,
                    account,
                    cents,
                    transfer,
                },
            )
        })
        .collect::<Vec<_>>();
    // Provider order isn't stable, so legs pair in date order and ties fall to the id.
    legs.sort_by(|left, right| {
        (left.date, &transactions[left.index].id).cmp(&(right.date, &transactions[right.index].id))
    });
    let mut paired = vec![false; legs.len()];
    let mut pairs = Vec::new();
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
                .min_by_key(|&into| (legs[into].date - legs[out].date).num_days().abs())
            else {
                continue;
            };
            paired[out] = true;
            paired[into] = true;
            pairs.push((out, into));
        }
    }
    for (out, into) in pairs {
        let (sent, arrived) = (legs[out].index, legs[into].index);
        transactions[sent].transfer_pair_id = Some(transactions[arrived].id.clone());
        transactions[arrived].transfer_pair_id = Some(transactions[sent].id.clone());
        let (filed, transfer) = match (legs[out].transfer, legs[into].transfer) {
            (true, false) => (arrived, sent),
            (false, true) => (sent, arrived),
            _ => continue,
        };
        let category = transactions[transfer].category.clone();
        let leg = &mut transactions[filed];
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
    classification.others_spending |= classification.spending;
    classification.spending = false;
    classification.credit = false;
    if amount > 0.0 {
        classification.mark = Mark::Transfer;
    }
}

#[cfg(test)]
mod tests;

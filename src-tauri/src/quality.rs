//! How well each detail of a bank or card transaction is supported: its amount and date,
//! merchant, category, and (in person) location. Each detail names the evidence it rests on and
//! a grade, and the transaction's grade is its weakest detail. Graded when the view is read,
//! after edits apply, from what is saved: statement reconciliation, the issuer's export,
//! Plaid's own confidence, where the place came from, and a matched business. Nothing is
//! inferred.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    finance_contract::{Snapshot, Transaction},
    merchants::Presence,
    places::PlaceSource,
    transaction_policy::CategorySource,
};

/// Ordered from weakest to strongest.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Grade {
    /// Sources disagree.
    Conflict,
    /// One uncertain source, or none.
    Weak,
    /// One source that is sure of itself.
    Supported,
    /// Two independent sources agree, or the user set it.
    Confirmed,
}

/// What a detail rests on.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Basis {
    /// A statement row matches Plaid's transaction.
    Statement,
    /// A statement row describing it shows another amount.
    StatementDiffers,
    /// Only an imported statement reports it.
    StatementOnly,
    /// Still pending, so the amount may change.
    Pending,
    /// Plaid's posted report.
    Provider,
    /// Plaid is at least 90% sure.
    ProviderConfident,
    /// Plaid is less sure, or doesn't say.
    ProviderUnsure,
    /// The user set it.
    Manual,
    /// The bank's descriptor and Plaid's location name the same city.
    SourcesAgree,
    /// The bank's descriptor and Plaid's location name different cities.
    SourcesDisagree,
    /// Only the bank's descriptor names the place.
    DescriptorOnly,
    /// Only Plaid's location names the place.
    BankOnly,
    /// Nothing names it.
    Unreported,
    /// The card issuer's export gives the merchant's name or street address.
    Issuer,
    /// The bank's records contradict Plaid's merchant name, so Brief shows theirs.
    ProviderContradicted,
    /// A business in Brief's place data at the issuer's address or in the descriptor's city
    /// matches the name.
    PlaceMatched,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub grade: Grade,
    pub basis: Basis,
}

const fn evidence(grade: Grade, basis: Basis) -> Evidence {
    Evidence { grade, basis }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    /// The weakest detail's grade.
    pub grade: Grade,
    pub amount: Evidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merchant: Option<Evidence>,
    pub category: Evidence,
    /// Graded for in-store purchases only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Evidence>,
    /// What a statement says the amount is, when it disagrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_amount: Option<f64>,
}

/// Plaid's confidence of more than 90%.
fn confident(level: Option<&str>) -> bool {
    matches!(level, Some("VERY_HIGH" | "HIGH"))
}

fn same_place(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn grade(
    transaction: &Transaction,
    matched: &BTreeSet<String>,
    differs: &BTreeMap<String, f64>,
) -> Quality {
    use {Basis::*, Grade::*};
    let statement_only = transaction.id.starts_with("statement:");
    let statement_amount = differs.get(&transaction.id).copied();
    let amount = if statement_amount.is_some() {
        evidence(Conflict, StatementDiffers)
    } else if matched.contains(&transaction.id) {
        evidence(Confirmed, Statement)
    } else if statement_only {
        evidence(Supported, StatementOnly)
    } else if transaction.pending {
        evidence(Weak, Pending)
    } else {
        evidence(Supported, Provider)
    };
    let purchase = matches!(
        transaction.payment_channel.as_deref(),
        Some("in store" | "online")
    );
    let merchant =
        (purchase || !transaction.counterparties.is_empty() || statement_only).then(|| {
            let merchant = transaction
                .counterparties
                .iter()
                .find(|counterparty| counterparty.r#type.as_deref() == Some("merchant"));
            if transaction.business.is_some() {
                evidence(Confirmed, PlaceMatched)
            } else if transaction.provider_merchant.is_some() {
                evidence(Supported, ProviderContradicted)
            } else if transaction
                .issuer
                .as_ref()
                .is_some_and(|issuer| issuer.name.is_some())
            {
                evidence(Confirmed, Issuer)
            } else if statement_only {
                evidence(Supported, StatementOnly)
            } else if transaction.merchant_entity_id.is_some()
                || merchant.is_some_and(|merchant| confident(merchant.confidence.as_deref()))
            {
                evidence(Supported, ProviderConfident)
            } else {
                evidence(Weak, ProviderUnsure)
            }
        });
    let category = if transaction.category_source == Some(CategorySource::Manual) {
        evidence(Confirmed, Manual)
    } else if confident(transaction.category_confidence.as_deref()) {
        evidence(Supported, ProviderConfident)
    } else {
        evidence(Weak, ProviderUnsure)
    };
    let in_person = transaction.presence == Some(Presence::InPerson)
        || (transaction.presence.is_none()
            && transaction.payment_channel.as_deref() == Some("in store"));
    let location = in_person.then(|| {
        // Plaid's location belongs to the merchant Plaid named; when the records contradict
        // that merchant, its address says nothing about this purchase.
        let reported = transaction
            .location
            .as_ref()
            .filter(|_| transaction.provider_merchant.is_none())
            .and_then(|location| location.city.as_deref());
        match (&transaction.place, reported) {
            (None, _) => evidence(Weak, Unreported),
            (Some(_), _) if transaction.business.is_some() => evidence(Confirmed, PlaceMatched),
            // The issuer's address stands only where the descriptor's city agrees or names none.
            (Some(place), _) if place.source == PlaceSource::Issuer => {
                let named = transaction
                    .original_description
                    .as_deref()
                    .or(transaction.description.as_deref())
                    .and_then(crate::places::parse_purchase_descriptor)
                    .is_some_and(|place| place.city.is_some());
                evidence(if named { Confirmed } else { Supported }, Issuer)
            }
            (Some(place), _) if place.source == PlaceSource::Bank => evidence(Supported, BankOnly),
            (Some(place), Some(reported)) => match place.city.as_deref() {
                Some(city) if same_place(city, reported) => evidence(Confirmed, SourcesAgree),
                Some(_) => evidence(Conflict, SourcesDisagree),
                None => evidence(Supported, DescriptorOnly),
            },
            (Some(_), None) => evidence(Supported, DescriptorOnly),
        }
    });
    let grade = [Some(amount), merchant, Some(category), location]
        .into_iter()
        .flatten()
        .map(|evidence| evidence.grade)
        .min()
        .unwrap_or(Supported);
    Quality {
        grade,
        amount,
        merchant,
        category,
        location,
        statement_amount,
    }
}

/// Grades every bank and card transaction in the view.
pub fn apply(snapshot: &mut Snapshot) {
    let reconciliation = &snapshot.statement_reconciliation;
    let matched = reconciliation
        .iter()
        .flat_map(|account| account.matched.iter().cloned())
        .collect::<BTreeSet<_>>();
    let differs = reconciliation
        .iter()
        .flat_map(|account| &account.amount_differs)
        .map(|difference| {
            (
                difference.transaction_id.clone(),
                difference.statement_amount,
            )
        })
        .collect::<BTreeMap<_, _>>();
    for transaction in &mut snapshot.transactions {
        let bank = transaction
            .account_id
            .as_deref()
            .is_some_and(|id| id.starts_with("plaid:"));
        transaction.quality = bank.then(|| grade(transaction, &matched, &differs));
    }
}

#[cfg(test)]
mod tests;

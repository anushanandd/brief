//! Facts about the graph's objects, each dated where a source dates it and naming what states
//! it. Your edits are observed facts whose source is `you`; a provider's report names the
//! provider; a date or figure read from other facts is derived and names `derived`.
use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::{Fact, Kind, Object, Tier};
use crate::{
    documents, insights::View, quality::Grade, storage::FinanceState,
    transaction_policy::CategorySource,
};

/// Who you are in facts about you, such as your employer.
pub(super) const YOU: &str = "person:you";

/// The source an account or transaction id names, such as `snaptrade` or `statement`. Plaid
/// transaction ids are Plaid's own, without a prefix.
fn provider(id: &str) -> String {
    id.split_once(':')
        .map_or("plaid", |(source, _)| source)
        .to_owned()
}

struct Facts(Vec<Fact>);

impl Facts {
    fn add(
        &mut self,
        subject: &str,
        predicate: &str,
        value: Value,
        sources: Vec<String>,
        grade: Option<Grade>,
        tier: Tier,
    ) {
        if value.is_null() {
            return;
        }
        self.0.push(Fact {
            subject: subject.to_owned(),
            predicate: predicate.to_owned(),
            value,
            from: None,
            to: None,
            sources,
            grade,
            tier,
        });
    }

    fn observed(&mut self, subject: &str, predicate: &str, value: Value, source: String) {
        self.add(
            subject,
            predicate,
            value,
            vec![source],
            None,
            Tier::Observed,
        );
    }

    fn yours(&mut self, subject: &str, predicate: &str, value: Value) {
        self.add(
            subject,
            predicate,
            value,
            vec!["you".into()],
            Some(Grade::Confirmed),
            Tier::Observed,
        );
    }

    fn derived(&mut self, subject: &str, predicate: &str, value: Value) {
        self.add(
            subject,
            predicate,
            value,
            vec!["derived".into()],
            None,
            Tier::Derived,
        );
    }

    /// Sets the span of the fact just added.
    fn spanning(&mut self, from: Option<String>, to: Option<String>) {
        if let Some(fact) = self.0.last_mut() {
            fact.from = from;
            fact.to = to;
        }
    }
}

pub(super) fn facts(state: &FinanceState, view: &View, objects: &[Object]) -> Vec<Fact> {
    let mut facts = Facts(Vec::new());
    let snapshot = &view.snapshot;
    let mut first_activity = BTreeMap::<&str, &str>::new();
    for transaction in &snapshot.transactions {
        if let Some(account) = &transaction.account_id {
            let first = first_activity.entry(account).or_insert(&transaction.date);
            if transaction.date.as_str() < *first {
                *first = &transaction.date;
            }
        }
    }
    for account in &snapshot.accounts {
        let id = Kind::Account.id(&account.id);
        let source = provider(&account.id);
        facts.observed(
            &id,
            "institution",
            json!(account.institution),
            source.clone(),
        );
        facts.observed(&id, "type", json!(account.r#type), source);
        if let Some(first) = first_activity.get(account.id.as_str()) {
            facts.derived(&id, "firstActivity", json!(first));
        }
    }
    for (account, closed) in &state.settings.account_closed_dates {
        facts.yours(&Kind::Account.id(account), "closed", json!(closed));
    }
    for transaction in &snapshot.transactions {
        let id = Kind::Transaction.id(&transaction.id);
        let source = provider(&transaction.id);
        let quality = transaction.quality.as_ref();
        facts.add(
            &id,
            "amount",
            json!(transaction.amount),
            vec![source.clone()],
            quality.map(|quality| quality.amount.grade),
            Tier::Observed,
        );
        facts.add(
            &id,
            "merchant",
            json!(transaction.merchant),
            vec![source.clone()],
            quality.and_then(|quality| quality.merchant.map(|evidence| evidence.grade)),
            Tier::Observed,
        );
        let manual = transaction.category_source == Some(CategorySource::Manual);
        facts.add(
            &id,
            "category",
            json!(transaction.category),
            vec![if manual { "you".into() } else { source }],
            quality.map(|quality| quality.category.grade),
            Tier::Observed,
        );
    }
    // Your edits, as saved; the view above already shows their effect.
    for (transaction, annotation) in &state.annotations {
        let id = Kind::Transaction.id(transaction);
        let edit = serde_json::to_value(annotation).unwrap_or(Value::Null);
        for (predicate, value) in edit.as_object().into_iter().flatten() {
            if predicate != "category" {
                facts.yours(&id, predicate, value.clone());
            }
        }
    }
    for holding in &snapshot.holdings {
        let id = Kind::Holding.id(&format!("{}:{}", holding.account_id, holding.ticker));
        facts.observed(
            &id,
            "shares",
            json!(holding.shares),
            provider(&holding.account_id),
        );
    }
    let mut employers = BTreeMap::<String, (String, String, Vec<String>)>::new();
    for document in &state.documents {
        let id = Kind::Document.id(&document.id);
        let source = vec![id.clone()];
        let stated = |facts: &mut Facts, predicate: &str, value: Value| {
            facts.add(&id, predicate, value, source.clone(), None, Tier::Observed);
        };
        stated(&mut facts, "issuer", json!(document.issuer));
        stated(&mut facts, "form", json!(document.form));
        stated(&mut facts, "taxYear", json!(document.tax_year));
        if document.period_start.is_some() || document.period_end.is_some() {
            stated(&mut facts, "period", json!(true));
            facts.spanning(document.period_start.clone(), document.period_end.clone());
        }
        // A pay statement is evidence of employment over its period.
        if document.kind == documents::Kind::Payslip {
            let (Some(issuer), Some(day)) = (
                document.issuer.as_deref().and_then(super::organization_key),
                document
                    .period_end
                    .clone()
                    .or_else(|| document.date.clone()),
            ) else {
                continue;
            };
            let start = document.period_start.clone().unwrap_or_else(|| day.clone());
            let employer = employers
                .entry(issuer)
                .or_insert_with(|| (start.clone(), day.clone(), Vec::new()));
            employer.0 = employer.0.clone().min(start);
            employer.1 = employer.1.clone().max(day);
            employer.2.push(id.clone());
        }
    }
    for (issuer, (first, last, sources)) in employers {
        facts.add(
            YOU,
            "employer",
            json!(Kind::Organization.id(&issuer)),
            sources,
            None,
            Tier::Observed,
        );
        facts.spanning(Some(first), Some(last));
    }
    for person in &state.settings.people {
        facts.yours(&person.id, "name", json!(person.name));
    }
    if let Some(insights) = &view.insights {
        for trip in &insights.trips.trips {
            facts.derived(
                &Kind::Trip.id(&trip.id),
                "destination",
                json!(trip.destination),
            );
            facts.spanning(Some(trip.start.to_string()), Some(trip.end.to_string()));
        }
    }
    for object in objects.iter().filter(|object| object.kind == Kind::Bill) {
        facts.derived(&object.id, "frequency", json!(object.subtitle));
        facts.derived(&object.id, "amount", json!(object.amount));
    }
    facts.0
}

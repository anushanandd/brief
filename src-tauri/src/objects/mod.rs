//! The object graph: everything Brief holds as typed objects with stable ids, the links
//! between them, and dated facts that name their sources. It is derived from what a read
//! sees (`insights::View`, with edits applied) and from the saved documents and settings, so
//! it changes only when they do. Search, the raw view, the readable folder, and later rules
//! read it; nothing writes to it but `graph`.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::{insights::View, quality::Grade, storage::FinanceState};

mod facts;
#[cfg(test)]
mod tests;

/// What an object is. An id is `kind:key`, so the kind is always readable from the id.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ObjectKind")]
pub enum Kind {
    Account,
    Transaction,
    Holding,
    Trade,
    Document,
    Statement,
    Person,
    /// A merchant, employer, institution, or document issuer: one per name.
    Organization,
    Place,
    Trip,
    /// A recurring bill, subscription, or deposit.
    Bill,
    /// A card benefit.
    Benefit,
    /// A news story about a holding. Search finds stories in the news cache; they are not
    /// part of the finance graph.
    Story,
    /// A span of your life with a cause: a job, a home, accounts opened together.
    Chapter,
    /// A published rule, such as a bank's engagement bonus, with its source.
    Program,
}

impl Kind {
    pub const ALL: [Kind; 15] = [
        Kind::Account,
        Kind::Transaction,
        Kind::Holding,
        Kind::Trade,
        Kind::Document,
        Kind::Statement,
        Kind::Person,
        Kind::Organization,
        Kind::Place,
        Kind::Trip,
        Kind::Bill,
        Kind::Benefit,
        Kind::Story,
        Kind::Chapter,
        Kind::Program,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Account => "account",
            Kind::Transaction => "transaction",
            Kind::Holding => "holding",
            Kind::Trade => "trade",
            Kind::Document => "document",
            Kind::Statement => "statement",
            Kind::Person => "person",
            Kind::Organization => "organization",
            Kind::Place => "place",
            Kind::Trip => "trip",
            Kind::Bill => "bill",
            Kind::Benefit => "benefit",
            Kind::Story => "story",
            Kind::Chapter => "chapter",
            Kind::Program => "program",
        }
    }

    pub fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The object id for a key: `kind:key`.
    pub fn id(self, key: &str) -> String {
        format!("{}:{key}", self.name())
    }
}

/// The kind an object id names.
pub fn kind_of(id: &str) -> Option<Kind> {
    Kind::from_name(id.split_once(':')?.0)
}

/// One object: what lists and search show of it, and its complete record.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "GraphObject")]
pub struct Object {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subtitle: Option<String>,
    /// The day it happened or began, `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub amount: Option<f64>,
    /// Further words search matches, beyond the title and subtitle.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[ts(skip)]
    pub text: String,
    /// Everything stored about it, as saved, for the raw view.
    #[ts(type = "Record<string, unknown>")]
    pub record: Value,
}

/// How one object relates to another.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "LinkRelation")]
pub enum Relation {
    /// The account it belongs to.
    Account,
    /// Who was paid, or paid.
    Counterparty,
    /// Where it happened.
    Place,
    /// Someone it was with.
    Person,
    /// The other leg of a transfer between two accounts.
    TransferPair,
    /// The purchase a refund repays.
    RefundOf,
    /// The institution an account is at.
    Institution,
    /// Who issued a document.
    Issuer,
    /// A transaction that is one occurrence of a recurring bill.
    Occurrence,
    /// A transaction a card benefit counted.
    Benefit,
    /// A transaction on a trip's days.
    Trip,
    /// A chapter a record belongs to, by a record that ties it there.
    Chapter,
    /// A record that defines a chapter.
    Anchor,
    /// A rule a chapter enables.
    Program,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "GraphLink")]
pub struct Link {
    pub from: String,
    pub relation: Relation,
    pub to: String,
}

/// How a fact is known; the four are never mixed.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "FactTier")]
pub enum Tier {
    /// A source states it, or you set it.
    Observed,
    /// Arithmetic or a date read from observed facts.
    Derived,
    /// A cited rule whose every condition observed records meet; it carries its proof.
    Explained,
    /// A pattern no rule settles, or a rule with a condition no record shows; yours to
    /// confirm or dismiss.
    Suggested,
}

/// One dated statement about an object, with where it came from.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "GraphFact")]
pub struct Fact {
    pub subject: String,
    /// What is stated, such as `category` or `employer`.
    pub predicate: String,
    #[ts(type = "unknown")]
    pub value: Value,
    /// The first day its sources show it holding, `YYYY-MM-DD`; absent when undated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<String>,
    /// The last day its sources show it holding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to: Option<String>,
    /// What states it: `you`, a provider (`plaid`, `snaptrade`), `derived`, or object ids.
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub grade: Option<Grade>,
    pub tier: Tier,
    /// Why a rule's conclusion holds, for an Explained or Suggested fact a rule reached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub proof: Option<crate::evidence::Proof>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Graph {
    /// Sorted by id; ids are unique.
    pub objects: Vec<Object>,
    /// Sorted and unique; both ends are objects in the graph.
    pub links: Vec<Link>,
    pub facts: Vec<Fact>,
}

impl Graph {
    /// The SHA-256 of the graph, which says whether its tables hold it.
    pub fn hash(&self) -> Result<String, String> {
        serde_json::to_vec(self)
            .map(|bytes| crate::database::sha256(&bytes))
            .map_err(|error| error.to_string())
    }
}

/// Everything known about one object, for its raw view.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ObjectDetail")]
pub struct Detail {
    pub object: Object,
    pub links: Vec<Link>,
    pub facts: Vec<Fact>,
    /// The record as its provider sent it, for accounts and transactions a provider reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, unknown>")]
    pub provider: Option<Value>,
}

/// The record a provider sent for an account or transaction, as saved in its cache.
pub fn provider_record(state: &FinanceState, object: &Object) -> Option<Value> {
    let key = object.id.split_once(':')?.1;
    let find = |records: &[Value], field: &str, id: &str| {
        records
            .iter()
            .find(|record| record.get(field).and_then(Value::as_str) == Some(id))
            .cloned()
    };
    let plaid = state.provider_data.plaid.as_ref();
    let snaptrade = state.provider_data.snaptrade.as_ref();
    match (object.kind, key.split_once(':')) {
        (Kind::Account, Some(("plaid", id))) => plaid.and_then(|data| {
            find(&data.accounts, "account_id", id)
                .or_else(|| find(&data.investment_accounts, "account_id", id))
        }),
        (Kind::Account, Some(("snaptrade", id))) => {
            snaptrade.and_then(|data| find(&data.accounts, "id", id))
        }
        (Kind::Transaction, Some(("snaptrade", rest))) => {
            let (account, activity) = rest.split_once(':')?;
            snaptrade.and_then(|data| find(data.activities.get(account)?, "id", activity))
        }
        (Kind::Transaction, None) => {
            plaid.and_then(|data| find(&data.transactions, "transaction_id", key))
        }
        _ => None,
    }
}

/// The key an organization's name gives: its words, lowercased, joined by `-`. The same name
/// however a source writes it is one organization, and its id survives every refresh.
pub fn organization_key(name: &str) -> Option<String> {
    let words = name
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    (!words.is_empty()).then(|| words.join("-"))
}

/// The record of a serializable value, or null when it can't be written.
fn record<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// Builds objects and links, and then facts about them.
struct Builder {
    objects: BTreeMap<String, Object>,
    links: BTreeSet<Link>,
    /// Organizations by id: their name as first seen, and what named them.
    organizations: BTreeMap<String, (String, BTreeSet<&'static str>)>,
}

impl Builder {
    fn add(&mut self, object: Object) {
        self.objects.entry(object.id.clone()).or_insert(object);
    }

    fn link(&mut self, from: &str, relation: Relation, to: String) {
        self.links.insert(Link {
            from: from.to_owned(),
            relation,
            to,
        });
    }

    /// The organization a name gives, noting what named it.
    fn organization(&mut self, name: &str, role: &'static str) -> Option<String> {
        let id = Kind::Organization.id(&organization_key(name)?);
        self.organizations
            .entry(id.clone())
            .or_insert_with(|| (name.trim().to_owned(), BTreeSet::new()))
            .1
            .insert(role);
        Some(id)
    }
}

/// The graph of what `view` shows and `state` saves.
pub fn graph(state: &FinanceState, view: &View) -> Graph {
    let mut builder = Builder {
        objects: BTreeMap::new(),
        links: BTreeSet::new(),
        organizations: BTreeMap::new(),
    };
    let snapshot = &view.snapshot;
    for account in &snapshot.accounts {
        let id = Kind::Account.id(&account.id);
        let name = view.account_name(&account.id).unwrap_or(&account.name);
        builder.add(Object {
            id: id.clone(),
            kind: Kind::Account,
            title: name.to_owned(),
            subtitle: Some(account.institution.clone()),
            date: None,
            amount: account.value,
            text: account.r#type.clone(),
            record: record(account),
        });
        if let Some(institution) = builder.organization(&account.institution, "institution") {
            builder.link(&id, Relation::Institution, institution);
        }
    }
    for transaction in &snapshot.transactions {
        let id = Kind::Transaction.id(&transaction.id);
        builder.add(Object {
            id: id.clone(),
            kind: Kind::Transaction,
            title: transaction.merchant.clone(),
            subtitle: Some(transaction.category.clone()),
            date: Some(transaction.date.clone()),
            amount: Some(transaction.amount),
            text: [
                transaction.description.as_deref(),
                transaction.original_description.as_deref(),
                transaction.note.as_deref(),
                Some(&transaction.account),
            ]
            .into_iter()
            .flatten()
            .chain(transaction.tags.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" "),
            record: record(transaction),
        });
        if let Some(account) = &transaction.account_id {
            builder.link(&id, Relation::Account, Kind::Account.id(account));
        }
        if let Some(organization) = builder.organization(&transaction.merchant, "merchant") {
            builder.link(&id, Relation::Counterparty, organization);
        }
        if let Some(business) = &transaction.business {
            let place = Kind::Place.id(&business.id);
            builder.add(Object {
                id: place.clone(),
                kind: Kind::Place,
                title: business.name.clone(),
                subtitle: business.address.clone(),
                date: None,
                amount: None,
                text: business.category.clone().unwrap_or_default(),
                record: record(business),
            });
            builder.link(&id, Relation::Place, place);
        }
        for person in &transaction.people {
            builder.link(&id, Relation::Person, person.clone());
        }
        if let Some(pair) = &transaction.transfer_pair_id {
            builder.link(&id, Relation::TransferPair, Kind::Transaction.id(pair));
        }
        if let Some(purchase) = &transaction.refund_of {
            builder.link(&id, Relation::RefundOf, Kind::Transaction.id(purchase));
        }
        if let Some(recurring) = &transaction.recurring_id {
            builder.link(&id, Relation::Occurrence, Kind::Bill.id(recurring));
        }
    }
    for holding in &snapshot.holdings {
        let id = Kind::Holding.id(&format!("{}:{}", holding.account_id, holding.ticker));
        builder.add(Object {
            id: id.clone(),
            kind: Kind::Holding,
            title: holding.ticker.clone(),
            subtitle: Some(holding.name.clone()),
            date: None,
            amount: holding.value,
            text: String::new(),
            record: record(holding),
        });
        builder.link(
            &id,
            Relation::Account,
            Kind::Account.id(&holding.account_id),
        );
    }
    for trade in &snapshot.trades {
        let id = Kind::Trade.id(&trade.id);
        builder.add(Object {
            id: id.clone(),
            kind: Kind::Trade,
            title: trade.ticker.clone().unwrap_or_else(|| trade.r#type.clone()),
            subtitle: Some(trade.r#type.clone()),
            date: Some(trade.date.clone()),
            amount: Some(trade.amount),
            text: trade.description.clone().unwrap_or_default(),
            record: record(trade),
        });
        builder.link(&id, Relation::Account, Kind::Account.id(&trade.account_id));
    }
    for document in &state.documents {
        let id = Kind::Document.id(&document.id);
        builder.add(Object {
            id: id.clone(),
            kind: Kind::Document,
            title: document.file_name.clone(),
            subtitle: document.form.clone().or_else(|| document.issuer.clone()),
            date: document
                .date
                .clone()
                .or_else(|| document.period_end.clone()),
            amount: None,
            text: [document.issuer.as_deref(), document.form.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" "),
            record: record(document),
        });
        if let Some(account) = &document.account_id {
            builder.link(&id, Relation::Account, Kind::Account.id(account));
        }
        if let Some(issuer) = document
            .issuer
            .as_deref()
            .and_then(|issuer| builder.organization(issuer, "issuer"))
        {
            builder.link(&id, Relation::Issuer, issuer);
        }
    }
    for statement in &state.statements {
        let id = Kind::Statement.id(&statement.id);
        // The file's bytes are the statement itself; the record names them by size.
        builder.add(Object {
            id: id.clone(),
            kind: Kind::Statement,
            title: statement.file_name.clone(),
            subtitle: view.account_name(&statement.account_id).map(str::to_owned),
            date: statement.imported_at.get(..10).map(str::to_owned),
            amount: None,
            text: String::new(),
            record: json!({
                "id": statement.id,
                "accountId": statement.account_id,
                "fileName": statement.file_name,
                "importedAt": statement.imported_at,
                "bytes": statement.contents.len(),
                "confirmedRows": statement.confirmed_rows,
            }),
        });
        builder.link(
            &id,
            Relation::Account,
            Kind::Account.id(&statement.account_id),
        );
    }
    // You, the subject of facts such as your employer.
    builder.add(Object {
        id: facts::YOU.to_owned(),
        kind: Kind::Person,
        title: "You".into(),
        subtitle: None,
        date: None,
        amount: None,
        text: String::new(),
        record: json!({ "id": facts::YOU }),
    });
    for person in &state.settings.people {
        builder.add(Object {
            id: person.id.clone(),
            kind: Kind::Person,
            title: person.name.clone(),
            subtitle: None,
            date: None,
            amount: None,
            text: person.match_names.join(" "),
            record: record(person),
        });
    }
    if let Some(insights) = &view.insights {
        for trip in &insights.trips.trips {
            let id = Kind::Trip.id(&trip.id);
            builder.add(Object {
                id: id.clone(),
                kind: Kind::Trip,
                title: trip.destination.clone(),
                subtitle: Some(format!("{} to {}", trip.start, trip.end)),
                date: Some(trip.start.to_string()),
                amount: Some(trip.total),
                text: String::new(),
                record: record(trip),
            });
            for transaction in &trip.transaction_ids {
                builder.link(
                    &Kind::Transaction.id(transaction),
                    Relation::Trip,
                    id.clone(),
                );
            }
            for person in &trip.companions {
                builder.link(&id, Relation::Person, person.clone());
            }
        }
        for program in &insights.evidence.programs {
            builder.add(Object {
                id: Kind::Program.id(&program.id),
                kind: Kind::Program,
                title: program.name.clone(),
                subtitle: Some(program.organization.clone()),
                date: Some(program.reviewed.to_string()),
                amount: None,
                text: program.summary.clone(),
                record: record(program),
            });
        }
        for chapter in &insights.evidence.chapters {
            let id = Kind::Chapter.id(&chapter.id);
            builder.add(Object {
                id: id.clone(),
                kind: Kind::Chapter,
                title: chapter.title.clone(),
                subtitle: Some(match chapter.end {
                    Some(end) => format!("{} to {end}", chapter.start),
                    None => format!("Since {}", chapter.start),
                }),
                date: Some(chapter.start.to_string()),
                amount: None,
                text: format!("{:?}", chapter.kind).to_lowercase(),
                record: record(chapter),
            });
            for anchor in &chapter.anchors {
                for evidence in &anchor.evidence {
                    builder.link(&id, Relation::Anchor, evidence.clone());
                }
            }
            // A member only timing suggests stays in the chapter's record, not a link.
            for member in chapter
                .members
                .iter()
                .filter(|member| member.tier == Tier::Observed)
            {
                builder.link(&member.id, Relation::Chapter, id.clone());
            }
            for program in &chapter.programs {
                builder.link(&id, Relation::Program, Kind::Program.id(program));
            }
        }
        let mut bills = BTreeSet::new();
        for event in &insights.expected {
            if !bills.insert(event.recurring_id.clone()) {
                continue;
            }
            builder.add(Object {
                id: Kind::Bill.id(&event.recurring_id),
                kind: Kind::Bill,
                title: event.title.clone(),
                subtitle: Some(event.frequency.to_owned()),
                date: Some(event.date.to_string()),
                amount: Some(event.amount),
                text: String::new(),
                record: record(event),
            });
        }
        for card in insights
            .benefits
            .iter()
            .chain(&insights.other_card_benefits)
        {
            for benefit in &card.history {
                let id = Kind::Benefit.id(&format!("{}:{}", card.account_id, benefit.id));
                builder.add(Object {
                    id,
                    kind: Kind::Benefit,
                    title: benefit.name.clone(),
                    subtitle: Some(card.product_name.clone()),
                    date: None,
                    amount: None,
                    text: String::new(),
                    record: record(benefit),
                });
            }
            // Each activity row is a transaction the benefit counted.
            for activity in &card.activity {
                builder.link(
                    &Kind::Transaction.id(&activity.id),
                    Relation::Benefit,
                    Kind::Benefit.id(&format!("{}:{}", card.account_id, activity.benefit_id)),
                );
            }
        }
    }
    let organizations = std::mem::take(&mut builder.organizations);
    for (id, (name, roles)) in organizations {
        builder.add(Object {
            id: id.clone(),
            kind: Kind::Organization,
            title: name,
            subtitle: Some(roles.iter().copied().collect::<Vec<_>>().join(", ")),
            date: None,
            amount: None,
            text: String::new(),
            record: json!({ "id": id, "roles": roles }),
        });
    }
    let Builder { objects, links, .. } = builder;
    // A link to something the graph doesn't hold (an account closed since, a pair outside
    // the window) would lead nowhere.
    let links = links
        .into_iter()
        .filter(|link| objects.contains_key(&link.from) && objects.contains_key(&link.to))
        .collect();
    let objects = objects.into_values().collect::<Vec<_>>();
    let facts = facts::facts(state, view, &objects);
    Graph {
        objects,
        links,
        facts,
    }
}

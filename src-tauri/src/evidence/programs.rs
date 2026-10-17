//! Program rules: published terms (a fee waiver, a bonus) as data. Each names its source and
//! the dates its terms hold, and lists conditions every one of which must be met by what Brief
//! observed before a conclusion is Explained. Built-in rules live in `programs.json`; rules the
//! user approves from research join them through settings, checked the same way.
use std::{collections::BTreeMap, sync::LazyLock};

use chrono::NaiveDate;
use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export, rename = "ProgramSource")]
pub struct Source {
    pub url: String,
    pub title: String,
}

/// What the terms say for a span of dates. A value is a list when the terms allow several (a
/// bonus that depends on when the card opened).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export, rename = "ProgramTerm")]
pub struct Term {
    pub from: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to: Option<NaiveDate>,
    pub values: BTreeMap<String, Vec<f64>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ProgramDirection")]
pub enum Direction {
    /// Money into the account.
    In,
    /// Money out of it.
    Out,
}

/// A transaction within `days` before another binding's.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export, rename = "ProgramBefore")]
pub struct Before {
    pub of: String,
    pub days: i64,
}

/// One condition. `as` names what it binds, so later conditions and the conclusion can refer
/// to it; `says` is how a proof states it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, rename = "ProgramCondition")]
pub enum Condition {
    /// An account whose name and institution match.
    #[serde(rename_all = "camelCase")]
    Account {
        #[serde(rename = "as")]
        alias: String,
        says: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        institution: Option<String>,
    },
    /// A credit card whose issuer-reported official name matches.
    #[serde(rename_all = "camelCase")]
    Card {
        #[serde(rename = "as")]
        alias: String,
        says: String,
        official_name: String,
    },
    /// A posted transaction in a bound account whose description matches.
    #[serde(rename_all = "camelCase")]
    Transaction {
        #[serde(rename = "as")]
        alias: String,
        says: String,
        account: String,
        description: String,
        direction: Direction,
        /// Its amount must be one the terms give under this name, on its date.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        amount_term: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        before: Option<Before>,
    },
    /// You worked at an organization, by pay statements, a W-2, or payroll deposits.
    #[serde(rename_all = "camelCase")]
    Employment {
        #[serde(rename = "as")]
        alias: String,
        says: String,
        organization: String,
    },
    /// The months a bound account received at least the terms' deposits, while a bound job
    /// lasted.
    #[serde(rename_all = "camelCase")]
    MonthlyDeposits {
        #[serde(rename = "as")]
        alias: String,
        says: String,
        account: String,
        at_least_term: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        during: Option<String>,
    },
    /// A condition no record Brief reads can show, so a conclusion that needs it stays
    /// Suggested.
    #[serde(rename_all = "camelCase")]
    Unobservable { says: String },
}

impl Condition {
    pub fn says(&self) -> &str {
        match self {
            Condition::Account { says, .. }
            | Condition::Card { says, .. }
            | Condition::Transaction { says, .. }
            | Condition::Employment { says, .. }
            | Condition::MonthlyDeposits { says, .. }
            | Condition::Unobservable { says } => says,
        }
    }

    pub fn alias(&self) -> Option<&str> {
        match self {
            Condition::Account { alias, .. }
            | Condition::Card { alias, .. }
            | Condition::Transaction { alias, .. }
            | Condition::Employment { alias, .. }
            | Condition::MonthlyDeposits { alias, .. } => Some(alias),
            Condition::Unobservable { .. } => None,
        }
    }
}

/// What the rule concludes about what its conditions bound.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export, rename = "ProgramConclusion")]
pub struct Conclusion {
    pub subject: String,
    pub predicate: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub object: Option<String>,
    pub title: String,
    /// The statement, with `{alias}` for a binding's amount and `{alias.date}` for its date.
    pub says: String,
    /// A monthly condition whose runs of met months the conclusion holds over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub spans: Option<String>,
}

/// A binding that should follow another and hasn't: a renewal's bonus.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export, rename = "ProgramExpect")]
pub struct Expect {
    pub missing: String,
    pub after: String,
    pub days: i64,
    pub says: String,
}

/// A monthly condition to watch: its latest complete month falling short is worth a note.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export, rename = "ProgramWatch")]
pub struct Watch {
    pub condition: String,
    pub says: String,
}

/// What changes when a bound job ends, for its chapter's checklist.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export, rename = "ProgramEnds")]
pub struct Ends {
    pub with: String,
    pub says: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export)]
pub struct Program {
    pub id: String,
    pub name: String,
    pub organization: String,
    pub summary: String,
    pub source: Source,
    /// When the source was last read against these terms.
    pub reviewed: NaiveDate,
    pub terms: Vec<Term>,
    pub conditions: Vec<Condition>,
    pub conclusion: Conclusion,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expect: Option<Expect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub watch: Option<Watch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ends: Option<Ends>,
}

/// A rule whose source hasn't been read against it in this long flags itself for review.
pub const STALE_DAYS: i64 = 365;

impl Program {
    /// The values the terms give under `name` on `date`; none when no terms cover the date,
    /// since a missing year is never extrapolated.
    pub fn values(&self, name: &str, date: NaiveDate) -> Option<&[f64]> {
        self.terms
            .iter()
            .filter(|term| term.from <= date && term.to.is_none_or(|to| date <= to))
            .find_map(|term| term.values.get(name).map(Vec::as_slice))
    }

    pub fn stale(&self, today: NaiveDate) -> bool {
        (today - self.reviewed).num_days() > STALE_DAYS
    }

    /// Checks a rule before it is used: an HTTPS source, patterns that compile, conditions
    /// that refer only to bindings made before them, and a conclusion about bound things.
    pub fn validate(&self) -> Result<(), String> {
        let invalid = |reason: &str| Err(format!("Program {}: {reason}", self.id));
        if self.id.is_empty() || self.id.len() > 100 || self.name.trim().is_empty() {
            return invalid("needs an id and a name");
        }
        if !self.source.url.starts_with("https://") {
            return invalid("its source must be an HTTPS page");
        }
        if self.terms.is_empty() || self.conditions.is_empty() {
            return invalid("needs terms and conditions");
        }
        let mut bound = Vec::<&str>::new();
        let known = |bound: &[&str], alias: &str| bound.contains(&alias);
        for condition in &self.conditions {
            let patterns = match condition {
                Condition::Account {
                    name, institution, ..
                } => [name.as_deref(), institution.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect(),
                Condition::Card { official_name, .. } => vec![official_name.as_str()],
                Condition::Transaction {
                    account,
                    description,
                    before,
                    ..
                } => {
                    if !known(&bound, account)
                        || before
                            .as_ref()
                            .is_some_and(|before| !known(&bound, &before.of))
                    {
                        return invalid("a transaction refers to something not bound before it");
                    }
                    vec![description.as_str()]
                }
                Condition::Employment { organization, .. } => vec![organization.as_str()],
                Condition::MonthlyDeposits {
                    account, during, ..
                } => {
                    if !known(&bound, account)
                        || during
                            .as_deref()
                            .is_some_and(|during| !known(&bound, during))
                    {
                        return invalid(
                            "monthly deposits refer to something not bound before them",
                        );
                    }
                    Vec::new()
                }
                Condition::Unobservable { .. } => Vec::new(),
            };
            for pattern in patterns {
                if pattern.trim().is_empty() || matcher(pattern).is_none() {
                    return invalid("a pattern is empty or doesn't compile");
                }
            }
            if let Some(alias) = condition.alias() {
                bound.push(alias);
            }
        }
        let conclusion = &self.conclusion;
        if !known(&bound, &conclusion.subject)
            || conclusion
                .object
                .as_deref()
                .is_some_and(|object| !known(&bound, object))
            || conclusion
                .spans
                .as_deref()
                .is_some_and(|spans| !known(&bound, spans))
        {
            return invalid("its conclusion refers to something no condition binds");
        }
        Ok(())
    }
}

/// A case-insensitive pattern, or none when it doesn't compile.
pub fn matcher(pattern: &str) -> Option<Regex> {
    Regex::new(&format!("(?i){pattern}")).ok()
}

#[derive(Deserialize)]
struct File {
    programs: Vec<Program>,
}

static BUILT_IN: LazyLock<Vec<Program>> = LazyLock::new(|| {
    let file: File = serde_json::from_str(include_str!("programs.json"))
        .expect("programs.json is valid program data");
    for program in &file.programs {
        program.validate().expect("built-in programs are valid");
    }
    file.programs
});

/// The built-in rules, then those the user approved, which replace a built-in rule of the
/// same id. A user rule that fails its checks is left out.
pub fn programs(approved: &[Program]) -> Vec<Program> {
    let mut programs = BUILT_IN
        .iter()
        .filter(|program| !approved.iter().any(|approved| approved.id == program.id))
        .cloned()
        .collect::<Vec<_>>();
    programs.extend(
        approved
            .iter()
            .filter(|program| program.validate().is_ok())
            .cloned(),
    );
    programs
}

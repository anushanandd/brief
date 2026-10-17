//! Opt-in research: finding the published terms that would explain what Brief sees, and
//! keeping approved terms current. It is off until the user turns it on, and only runs when
//! they ask.
//!
//! 1. A gap is something the graph knows with no rule about it: a card product, an employer,
//!    a bank account type.
//! 2. The query names only public things: the product, the employer, the institution. Never
//!    an amount, an account number, a date, or the user's name, and nothing with a digit in
//!    it that the user's records supplied.
//! 3. Each query is logged before it runs and completed after, with the pages that came back.
//! 4. The user drafts a rule from a template beside those pages, citing one. The draft is
//!    validated like a built-in rule and saved in settings only when the user approves it.
//! 5. Monthly, each rule's source is read again; a changed page flags its rule for review.
//!
//! Pages are data, never instructions: research proposes rules and never changes records.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    evidence::{
        chapters::{Chapter, ChapterKind},
        programs::{Conclusion, Condition, Direction, Ends, Expect, Program, Source, Term, Watch},
    },
    finance_contract::Account,
    insights::benefits,
    objects::organization_key,
    providers::exa::Page,
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ResearchEntryKind")]
pub enum EntryKind {
    /// A search for a gap's terms.
    Search,
    /// A rule's source read again.
    Recheck,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ResearchStatus")]
pub enum Status {
    Running,
    Done,
    Failed,
}

/// One logged query: exactly what left the Mac, and what came back.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ResearchEntry")]
pub struct Entry {
    pub id: String,
    pub kind: EntryKind,
    /// The query text, or the URL read again.
    pub query: String,
    /// The gap or program it was for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub subject: Option<String>,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub finished_at: Option<String>,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    pub pages: Vec<Page>,
}

impl Entry {
    pub fn begin(
        kind: EntryKind,
        query: String,
        subject: Option<String>,
        now: DateTime<Utc>,
    ) -> Self {
        let started_at = now.to_rfc3339();
        Self {
            id: crate::database::sha256(format!("{started_at}\n{query}").as_bytes())[..16]
                .to_owned(),
            kind,
            query,
            subject,
            started_at,
            finished_at: None,
            status: Status::Running,
            error: None,
            pages: Vec::new(),
        }
    }

    pub fn finish(&mut self, result: Result<Vec<Page>, String>, now: DateTime<Utc>) {
        self.finished_at = Some(now.to_rfc3339());
        match result {
            Ok(pages) => {
                self.status = Status::Done;
                self.pages = pages;
            }
            Err(error) => {
                self.status = Status::Failed;
                self.error = Some(error);
            }
        }
    }
}

/// What a rule's source said when last read.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "SourceCheck")]
pub struct SourceCheck {
    pub url: String,
    /// SHA-256 of the page's text, with spacing and case evened out.
    pub hash: String,
    pub checked_at: String,
    /// When a read found the page different from the one before; cleared once reviewed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub changed_at: Option<String>,
}

/// How often each rule's source is read again.
pub const RECHECK_DAYS: i64 = 30;

/// The page's text as compared between reads.
pub fn page_hash(text: &str) -> String {
    let even = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    crate::database::sha256(even.as_bytes())
}

/// Whether a source is due to be read again.
pub fn due(check: Option<&SourceCheck>, now: DateTime<Utc>) -> bool {
    check
        .and_then(|check| DateTime::parse_from_rfc3339(&check.checked_at).ok())
        .is_none_or(|checked| (now - checked.to_utc()).num_days() >= RECHECK_DAYS)
}

/// The check after reading a source: the first read records it; a later read that differs
/// marks it changed, and a mark stays until the user reviews it.
pub fn checked(
    previous: Option<&SourceCheck>,
    url: &str,
    text: &str,
    now: DateTime<Utc>,
) -> SourceCheck {
    let hash = page_hash(text);
    let changed_at = match previous {
        Some(previous) if previous.hash != hash => Some(now.to_rfc3339()),
        Some(previous) => previous.changed_at.clone(),
        None => None,
    };
    SourceCheck {
        url: url.to_owned(),
        hash,
        checked_at: now.to_rfc3339(),
        changed_at,
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ResearchGapKind")]
pub enum GapKind {
    Card,
    Employer,
    Account,
}

/// Something with no rule about it, and the public query that would look for one.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ResearchGap")]
pub struct Gap {
    pub id: String,
    pub kind: GapKind,
    /// The public name it is known by.
    pub name: String,
    pub query: String,
}

/// A public name with whatever a record could add stripped: words with digits (account
/// masks, store numbers), symbols, and repeats. None when nothing public is left.
pub fn public_name(name: &str) -> Option<String> {
    let mut seen = BTreeSet::new();
    let words = name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '/' || c == '|')
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphabetic() || matches!(c, '&' | '\'' | '.'))
                .collect::<String>()
        })
        .filter(|word| {
            !word.is_empty()
                && !name.split_whitespace().any(|raw| {
                    raw.contains(word.as_str()) && raw.chars().any(|c| c.is_ascii_digit())
                })
        })
        .filter(|word| seen.insert(word.to_lowercase()))
        .take(8)
        .collect::<Vec<_>>();
    let joined = words
        .join(" ")
        .trim_matches(|c: char| c == '.' || c == '\'')
        .to_owned();
    (joined.chars().filter(|c| c.is_alphabetic()).count() >= 2).then_some(joined)
}

/// Whether a known rule's organization or summary already covers a name.
fn covered(programs: &[Program], name: &str) -> bool {
    let Some(key) = organization_key(name) else {
        return true;
    };
    programs.iter().any(|program| {
        organization_key(&program.organization)
            .is_some_and(|organization| key.contains(&organization) || organization.contains(&key))
    })
}

/// The gaps in what rules cover: cards with neither benefit rules nor a program, employers no
/// program names, and deposit accounts at institutions no program names.
pub fn gaps(accounts: &[Account], chapters: &[Chapter], programs: &[Program]) -> Vec<Gap> {
    let mut gaps = BTreeMap::<String, Gap>::new();
    let mut add = |kind: GapKind, name: String, query: String| {
        let id = format!(
            "{}:{}",
            match kind {
                GapKind::Card => "card",
                GapKind::Employer => "employer",
                GapKind::Account => "account",
            },
            organization_key(&name).unwrap_or_default()
        );
        gaps.entry(id.clone()).or_insert(Gap {
            id,
            kind,
            name,
            query,
        });
    };
    for account in accounts {
        match account.r#type.as_str() {
            "credit" => {
                let Some(official) = account.official_name.as_deref() else {
                    continue;
                };
                if benefits::named_product(Some(official)).is_some() {
                    continue;
                }
                if let Some(name) = public_name(official).filter(|name| !covered(programs, name)) {
                    let query =
                        format!("{name} card benefits, annual fee, and statement credit terms");
                    add(GapKind::Card, name, query);
                }
            }
            "cash" => {
                let product = account.official_name.as_deref().unwrap_or(&account.name);
                let (Some(institution), Some(product)) =
                    (public_name(&account.institution), public_name(product))
                else {
                    continue;
                };
                if covered(programs, &institution) {
                    continue;
                }
                let name = if product.to_lowercase().contains(&institution.to_lowercase()) {
                    product
                } else {
                    format!("{institution} {product}")
                };
                let query = format!("{name} monthly fee waiver requirements and bonus terms");
                add(GapKind::Account, name, query);
            }
            _ => {}
        }
    }
    for chapter in chapters
        .iter()
        .filter(|chapter| chapter.kind == ChapterKind::Work)
    {
        let Some(name) = public_name(&chapter.title) else {
            continue;
        };
        if covered(programs, &name) {
            continue;
        }
        let query = format!(
            "{name} employee benefits: retirement match, stock purchase plan, and banking perks for employees"
        );
        add(GapKind::Employer, name, query);
    }
    gaps.into_values().collect()
}

/// The kinds of rule research can draft, matching what the rule engine can check.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ProgramTemplate")]
pub enum Template {
    /// A deposit, such as an engagement bonus, that repays a card's annual fee.
    BonusOffsetsFee,
    /// An account's monthly fee, waived for an employer's staff with enough deposits.
    EmployeeDepositWaiver,
}

/// What the user fills in beside the pages research found. Names are matched as plain words,
/// never as patterns.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ProgramDraft")]
pub struct Draft {
    pub template: Template,
    pub name: String,
    pub organization: String,
    pub summary: String,
    pub source_url: String,
    pub source_title: String,
    /// The day the terms begin; earlier dates are never covered.
    pub effective: NaiveDate,
    /// The account's name as it shows, such as "Platinum CashPlus".
    pub account_name: String,
    pub institution: String,
    /// For a bonus: the card's official name, as Plaid reports it.
    #[serde(default)]
    #[ts(optional)]
    pub card_name: Option<String>,
    /// For a bonus: the words its deposit is described by.
    #[serde(default)]
    #[ts(optional)]
    pub bonus_words: Option<String>,
    /// The bonus amount, or the monthly deposits a waiver needs.
    pub amount: f64,
    /// For a waiver: the monthly fee it waives.
    #[serde(default)]
    #[ts(optional)]
    pub fee: Option<f64>,
    /// For a waiver: a condition no record shows, such as online enrollment.
    #[serde(default)]
    #[ts(optional)]
    pub unobservable: Option<String>,
}

/// Plain words as a case-insensitive pattern that matches only them.
fn words(text: &str) -> Result<String, String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || text.len() > 120 {
        return Err("Each name needs between 1 and 120 characters".into());
    }
    Ok(regex::escape(&text.to_lowercase()))
}

fn slug(text: &str) -> String {
    organization_key(text).unwrap_or_default().replace(' ', "-")
}

fn money(amount: f64) -> String {
    if amount.fract() == 0.0 {
        format!("${amount:.0}")
    } else {
        format!("${amount:.2}")
    }
}

/// The rule a draft describes, reviewed today, and checked as a built-in rule is.
pub fn draft(draft: &Draft, today: NaiveDate) -> Result<Program, String> {
    if !(draft.amount.is_finite() && draft.amount > 0.0) {
        return Err("The amount must be more than zero".into());
    }
    if draft.effective > today {
        return Err("Terms can't begin after today".into());
    }
    let organization = draft.organization.trim();
    let account = Condition::Account {
        alias: "account".into(),
        says: format!(
            "A {} account at {}",
            draft.account_name.trim(),
            draft.institution.trim()
        ),
        name: Some(words(&draft.account_name)?),
        institution: Some(words(&draft.institution)?),
    };
    let (id, terms, conditions, conclusion, expect, watch, ends) = match draft.template {
        Template::BonusOffsetsFee => {
            let card_name = draft.card_name.as_deref().ok_or("Name the card")?;
            let bonus_words = draft.bonus_words.as_deref().ok_or("Name the bonus")?;
            (
                format!("approved-{}-{}-bonus", slug(organization), slug(&draft.account_name)),
                vec![Term {
                    from: draft.effective,
                    to: None,
                    values: BTreeMap::from([("bonus".to_owned(), vec![draft.amount])]),
                }],
                vec![
                    account,
                    Condition::Card {
                        alias: "card".into(),
                        says: format!("The {}, by the name the issuer reports", card_name.trim()),
                        official_name: words(card_name)?,
                    },
                    Condition::Transaction {
                        alias: "bonus".into(),
                        says: format!("A deposit to that account named {}, of the amount the terms give", bonus_words.trim()),
                        account: "account".into(),
                        description: words(bonus_words)?,
                        direction: Direction::In,
                        amount_term: Some("bonus".into()),
                        before: None,
                    },
                    Condition::Transaction {
                        alias: "fee".into(),
                        says: "The card's annual membership fee, charged in the 13 months before the bonus".into(),
                        account: "card".into(),
                        description: "membership fee|annual fee".into(),
                        direction: Direction::Out,
                        amount_term: None,
                        before: Some(crate::evidence::programs::Before { of: "bonus".into(), days: 396 }),
                    },
                ],
                Conclusion {
                    subject: "bonus".into(),
                    predicate: "offsets".into(),
                    object: Some("fee".into()),
                    title: "Card fee offset".into(),
                    says: "The {bonus} bonus offsets the card's {fee} annual fee".into(),
                    spans: None,
                },
                Some(Expect {
                    missing: "bonus".into(),
                    after: "fee".into(),
                    days: 60,
                    says: "The bonus usually follows the card's annual fee; none has since the fee on {fee.date}.".into(),
                }),
                None,
                None,
            )
        }
        Template::EmployeeDepositWaiver => {
            let fee = draft
                .fee
                .filter(|fee| fee.is_finite() && *fee > 0.0)
                .ok_or("Give the monthly fee")?;
            let mut conditions = vec![
                account,
                Condition::Employment {
                    alias: "job".into(),
                    says: format!("You work at {organization}, by your pay statements or payroll"),
                    organization: words(organization)?,
                },
                Condition::MonthlyDeposits {
                    alias: "months".into(),
                    says: "At least the terms' deposits reached the account in the month, while you worked there".into(),
                    account: "account".into(),
                    at_least_term: "deposits".into(),
                    during: Some("job".into()),
                },
            ];
            if let Some(says) = draft
                .unobservable
                .as_deref()
                .map(str::trim)
                .filter(|says| !says.is_empty())
            {
                conditions.push(Condition::Unobservable {
                    says: says.to_owned(),
                });
            }
            (
                format!("approved-{}-{}-waiver", slug(organization), slug(&draft.account_name)),
                vec![Term {
                    from: draft.effective,
                    to: None,
                    values: BTreeMap::from([
                        ("deposits".to_owned(), vec![draft.amount]),
                        ("fee".to_owned(), vec![fee]),
                    ]),
                }],
                conditions,
                Conclusion {
                    subject: "account".into(),
                    predicate: "feeWaived".into(),
                    object: None,
                    title: "Monthly fee waived".into(),
                    says: format!("The {} monthly fee is waived under the employee criteria", money(fee)),
                    spans: Some("months".into()),
                },
                None,
                Some(Watch {
                    condition: "months".into(),
                    says: format!(
                        "Deposits to {} were under {{deposits}} in {{month}}; the {} monthly fee may apply.",
                        draft.account_name.trim(),
                        money(fee)
                    ),
                }),
                Some(Ends {
                    with: "job".into(),
                    says: format!("When work at {organization} ends, the employee waiver on {} lapses.", draft.account_name.trim()),
                }),
            )
        }
    };
    let program = Program {
        id,
        name: draft.name.trim().to_owned(),
        organization: organization.to_owned(),
        summary: draft.summary.trim().to_owned(),
        source: Source {
            url: draft.source_url.trim().to_owned(),
            title: draft.source_title.trim().to_owned(),
        },
        reviewed: today,
        terms,
        conditions,
        conclusion,
        notes: Vec::new(),
        expect,
        watch,
        ends,
    };
    program.validate()?;
    Ok(program)
}

#[cfg(test)]
mod tests;

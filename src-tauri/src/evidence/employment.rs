//! Who paid you, and when: employers from pay statements (exact days, and the deposits each
//! names), W-2s (a tax year and the state that withheld), and payroll deposits. A bank
//! descriptor joins an employer when a pay statement names that deposit, or when its words
//! begin the employer's words, as banks truncate them ("UNIVERSITY OF DE" for "University
//! of Denver"). Payroll that matches no document stands as its own employer, named by its
//! descriptor.
use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate};

use crate::{
    documents::{Document, Kind},
    finance_contract::Transaction,
    transaction_policy::Kind as Classified,
};

/// How exactly a date is known.
#[derive(Clone, Copy, Debug, serde::Serialize, PartialEq, Eq, PartialOrd, Ord, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "DatePrecision")]
pub enum Precision {
    Day,
    Month,
    Year,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Employer {
    /// The organization key of its name, as the object graph keys organizations.
    pub key: String,
    pub name: String,
    pub first: NaiveDate,
    pub last: NaiveDate,
    pub precision: Precision,
    /// Pay statements and W-2s, by document id.
    pub documents: Vec<String>,
    /// Payroll deposits, by transaction id, oldest first.
    pub deposits: Vec<(NaiveDate, f64, String)>,
    /// States its W-2s name, by tax year.
    pub states: BTreeMap<i32, String>,
    /// Paid recently enough to still be your employer.
    pub ongoing: bool,
    /// Known only from deposits, not from any document.
    pub inferred: bool,
}

/// Words in payroll descriptors that name no one.
const NOISE: [&str; 16] = [
    "payroll", "direct", "dep", "deposit", "ppd", "ccd", "id", "inc", "llc", "corp", "co", "the",
    "ach", "credit", "salary", "pay",
];
/// Words in legal names that descriptors drop.
const SUFFIXES: [&str; 8] = [
    "inc",
    "llc",
    "corp",
    "corporation",
    "co",
    "group",
    "services",
    "the",
];

fn words(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty() && !word.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_lowercase)
        .collect()
}

/// A descriptor's naming words as written: references, codes, noise, and repeats removed.
/// References follow the name ("PPD ID: 123", "CCD ID:"), and codes mix letters and digits.
fn descriptor_words(transaction: &Transaction) -> Vec<String> {
    let text = transaction
        .original_description
        .as_deref()
        .or(transaction.description.as_deref())
        .unwrap_or(&transaction.merchant);
    let name = text
        .split(" PPD ")
        .next()
        .unwrap_or(text)
        .split(" CCD ")
        .next()
        .unwrap_or(text);
    let mut kept = Vec::<String>::new();
    for word in name.split(|character: char| !character.is_alphanumeric()) {
        let lower = word.to_lowercase();
        let code = word.chars().any(|c| c.is_ascii_digit());
        if word.is_empty()
            || code
            || NOISE.contains(&lower.as_str())
            || kept.iter().any(|known| known.to_lowercase() == lower)
        {
            continue;
        }
        kept.push(word.to_owned());
        if kept.len() == 4 {
            break;
        }
    }
    kept
}

fn descriptor_key(transaction: &Transaction) -> Option<String> {
    let key = descriptor_words(transaction)
        .iter()
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>();
    (!key.is_empty()).then(|| key.join(" "))
}

/// A payroll group's name from its descriptor: as written when it mixes case, else in title
/// case ("Handshake AI Sol", "Example Analytics").
fn descriptor_name(transaction: &Transaction) -> String {
    descriptor_words(transaction)
        .into_iter()
        .map(|word| {
            if word.chars().any(char::is_lowercase) {
                word
            } else {
                let mut letters = word.chars();
                letters
                    .next()
                    .map(|first| {
                        first
                            .to_uppercase()
                            .chain(letters.flat_map(char::to_lowercase))
                            .collect()
                    })
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Small deposits that verify an account, which no employer pays.
fn verification(transaction: &Transaction) -> bool {
    let text = format!(
        "{} {}",
        transaction.merchant,
        transaction.description.as_deref().unwrap_or_default()
    )
    .to_lowercase();
    transaction.amount < 1.0
        || ["acctverify", "verify", "trial deposit", "micro"]
            .iter()
            .any(|word| text.contains(word))
}

fn payroll_words(transaction: &Transaction) -> bool {
    let text = format!(
        "{} {}",
        transaction.merchant,
        transaction.description.as_deref().unwrap_or_default()
    )
    .to_lowercase();
    ["payroll", "direct dep", "salary", "wages"]
        .iter()
        .any(|word| text.contains(word))
}

/// An employer's distinctive words, legal suffixes dropped.
fn name_words(name: &str) -> Vec<String> {
    words(name)
        .into_iter()
        .filter(|word| !SUFFIXES.contains(&word.as_str()))
        .collect()
}

/// Whether a descriptor names an employer: its words begin the employer's words in order
/// ("university of de" for "university of denver"), or the employer's whole name appears in
/// it ("book transfer b/o forge global inc"). One short word alone never matches.
pub fn names(descriptor: &str, employer: &str) -> bool {
    let theirs = name_words(employer);
    let ours = words(descriptor)
        .into_iter()
        .filter(|word| !NOISE.contains(&word.as_str()))
        .collect::<Vec<_>>();
    if theirs.is_empty() || ours.is_empty() {
        return false;
    }
    let enough =
        |matched: &[String]| matched.len() >= 2 || matched.iter().any(|word| word.len() >= 5);
    let leading = ours.len().min(theirs.len());
    let prefixed = ours
        .iter()
        .zip(&theirs)
        .take(leading)
        .all(|(our, their)| their.starts_with(our.as_str()) || our.starts_with(their.as_str()));
    if prefixed && enough(&ours[..leading]) {
        return true;
    }
    enough(&theirs)
        && ours
            .windows(theirs.len())
            .any(|window| window.iter().zip(&theirs).all(|(our, their)| our == their))
}

fn date(text: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text?.get(..10)?, "%Y-%m-%d").ok()
}

/// A pay statement or W-2's employer, merged into the list by name.
fn employer_for<'a>(
    employers: &'a mut Vec<Employer>,
    name: &str,
    day: NaiveDate,
) -> &'a mut Employer {
    let key = crate::objects::organization_key(name).unwrap_or_default();
    if let Some(index) = employers.iter().position(|employer| employer.key == key) {
        return &mut employers[index];
    }
    employers.push(Employer {
        key,
        name: name.trim().to_owned(),
        first: day,
        last: day,
        precision: Precision::Day,
        documents: Vec::new(),
        deposits: Vec::new(),
        states: BTreeMap::new(),
        ongoing: false,
        inferred: false,
    });
    employers.last_mut().expect("just pushed")
}

/// A paycheck counts as recent within this many days of today: a monthly payroll's gap.
const ONGOING_DAYS: i64 = 40;

pub fn employers(
    documents: &[Document],
    transactions: &[Transaction],
    today: NaiveDate,
) -> Vec<Employer> {
    let mut employers = Vec::<Employer>::new();
    let deposits = transactions
        .iter()
        .filter(|transaction| {
            !transaction.pending
                && transaction.amount > 0.0
                && !verification(transaction)
                && transaction
                    .classification
                    .as_ref()
                    .is_some_and(|classification| classification.kind == Classified::Income)
        })
        .filter_map(|transaction| Some((date(Some(transaction.posted_date()))?, transaction)))
        .collect::<Vec<_>>();
    // Descriptors each employer is known by, learned from the deposits pay statements name.
    let mut aliases = BTreeMap::<String, String>::new();
    for document in documents
        .iter()
        .filter(|document| document.kind == Kind::Payslip)
    {
        let (Some(issuer), Some(day)) = (
            document.issuer.as_deref(),
            date(document.period_end.as_deref()).or(date(document.date.as_deref())),
        ) else {
            continue;
        };
        let start = date(document.period_start.as_deref()).unwrap_or(day);
        let paid = date(document.date.as_deref()).unwrap_or(day);
        let employer = employer_for(&mut employers, issuer, start);
        employer.first = employer.first.min(start);
        employer.last = employer.last.max(paid.max(day));
        employer.documents.push(format!("document:{}", document.id));
        for named in &document.deposits {
            let found = deposits.iter().find(|(on, transaction)| {
                (transaction.amount - named.amount).abs() < 0.005
                    && (*on - paid).num_days().abs() <= 5
            });
            if let Some((on, transaction)) = found {
                employer
                    .deposits
                    .push((*on, transaction.amount, transaction.id.clone()));
                if let Some(key) = descriptor_key(transaction) {
                    aliases.insert(key, employer.key.clone());
                }
            }
        }
    }
    for document in documents.iter().filter(|document| {
        document.kind == Kind::TaxForm
            && document
                .form
                .as_deref()
                .is_some_and(|form| form.contains("W-2"))
    }) {
        let (Some(issuer), Some(year)) = (document.issuer.as_deref(), document.tax_year) else {
            continue;
        };
        let (Some(start), Some(end)) = (
            NaiveDate::from_ymd_opt(year, 1, 1),
            NaiveDate::from_ymd_opt(year, 12, 31),
        ) else {
            continue;
        };
        let known = employers.iter().any(|employer| {
            employer.key == crate::objects::organization_key(issuer).unwrap_or_default()
        });
        let employer = employer_for(&mut employers, issuer, start);
        if !known {
            // A W-2 alone dates the job only to its year, until deposits narrow it.
            employer.first = start;
            employer.last = end;
            employer.precision = Precision::Year;
        }
        employer.documents.push(format!("document:{}", document.id));
        if let Some(state) = document
            .field("state")
            .and_then(|field| field.text.clone())
            .filter(|state| state.len() == 2)
        {
            employer.states.insert(year, state.to_uppercase());
        }
    }
    // Every other income deposit joins the employer it names, or a payroll group of its own.
    let mut groups = BTreeMap::<String, Vec<(NaiveDate, &Transaction)>>::new();
    for (on, transaction) in &deposits {
        if employers.iter().any(|employer| {
            employer
                .deposits
                .iter()
                .any(|(_, _, id)| id == &transaction.id)
        }) {
            continue;
        }
        let Some(key) = descriptor_key(transaction) else {
            continue;
        };
        let named = aliases.get(&key).cloned().or_else(|| {
            let text = transaction
                .original_description
                .as_deref()
                .or(transaction.description.as_deref())
                .unwrap_or(&transaction.merchant);
            employers
                .iter()
                .find(|employer| names(text, &employer.name))
                .map(|employer| employer.key.clone())
        });
        match named.and_then(|key| employers.iter_mut().find(|employer| employer.key == key)) {
            Some(employer) => {
                employer
                    .deposits
                    .push((*on, transaction.amount, transaction.id.clone()))
            }
            None => groups.entry(key).or_default().push((*on, transaction)),
        }
    }
    for (key, members) in groups {
        // Payroll by its words and repetition: two or more deposits a payroll descriptor names,
        // or three across at least two months.
        let months = members
            .iter()
            .map(|(on, _)| on.format("%Y-%m").to_string())
            .collect::<std::collections::BTreeSet<_>>();
        let payroll = members
            .iter()
            .any(|(_, transaction)| payroll_words(transaction));
        if !(payroll && members.len() >= 2 || members.len() >= 3 && months.len() >= 2) {
            continue;
        }
        let first = members.iter().map(|(on, _)| *on).min().unwrap_or(today);
        let last = members.iter().map(|(on, _)| *on).max().unwrap_or(today);
        let name = members
            .first()
            .map(|(_, transaction)| descriptor_name(transaction))
            .unwrap_or_else(|| key.clone());
        employers.push(Employer {
            key: crate::objects::organization_key(&name).unwrap_or(key),
            name,
            first,
            last,
            precision: Precision::Day,
            documents: Vec::new(),
            deposits: members
                .iter()
                .map(|(on, transaction)| (*on, transaction.amount, transaction.id.clone()))
                .collect(),
            states: BTreeMap::new(),
            ongoing: false,
            inferred: true,
        });
    }
    for employer in &mut employers {
        employer
            .deposits
            .sort_by(|left, right| left.0.cmp(&right.0).then(left.2.cmp(&right.2)));
        employer.deposits.dedup_by(|left, right| left.2 == right.2);
        if let (Some(first), Some(last)) = (employer.deposits.first(), employer.deposits.last()) {
            if employer.precision == Precision::Year {
                // Deposits date a W-2's job to the day.
                employer.first = first.0;
                employer.last = last.0;
                employer.precision = Precision::Day;
            } else {
                employer.first = employer.first.min(first.0);
                employer.last = employer.last.max(last.0);
            }
        }
        employer.documents.sort();
        employer.documents.dedup();
        employer.ongoing = employer.precision == Precision::Day
            && today - employer.last <= Duration::days(ONGOING_DAYS);
    }
    employers.sort_by(|left, right| left.first.cmp(&right.first).then(left.key.cmp(&right.key)));
    employers
}

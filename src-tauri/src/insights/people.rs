//! People: who a transaction or trip was with. A payment to or from a person is tagged with
//! whoever's match name its descriptor contains; anything chosen by hand, nobody included,
//! stays as chosen. Tagging never changes an amount, a category, or a classification.
use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::Serialize;

use super::trips::Trip;
use crate::finance_contract::Transaction;
use crate::numeric::minor_units;
use crate::settings::{match_text, Person, TripCompanions};
use crate::transaction_policy::{PeopleSource, PEOPLE_CATEGORY};

/// Unmatched names offered as people, most transactions first.
const MAX_SUGGESTIONS: usize = 20;
/// A name read from a descriptor runs at most this many words.
const MAX_NAME_WORDS: usize = 4;
/// Words that end a name in a Zelle descriptor: the reference and date that follow it.
const NAME_STOPS: &[&str] = &[
    "CONF",
    "CONFIRMATION",
    "REF",
    "ON",
    "ID",
    "FOR",
    "MEMO",
    "VIA",
    "JPM",
    "TRANS",
    "PAYMENT",
    "TRANSFER",
    "ZELLE",
];

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct People {
    /// Each person with anything tagged, most recent activity first; people with nothing
    /// tagged follow by name.
    pub people: Vec<PersonSummary>,
    /// Names Zelle descriptors carry that match nobody.
    pub suggestions: Vec<NameSuggestion>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersonSummary {
    pub id: String,
    pub transaction_count: u32,
    /// Money sent to them through a payment app, signed as transactions are (negative).
    pub sent: f64,
    /// Money received from them through a payment app (positive).
    pub received: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<NaiveDate>,
    /// Trips they went on, newest first.
    pub trip_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NameSuggestion {
    /// The name as people write it, such as `Jane Doe`.
    pub name: String,
    /// The name as the bank prints it, for matching: `JANE DOE`.
    pub match_name: String,
    /// Newest first.
    pub transaction_ids: Vec<String>,
    /// People already saved it could be, likeliest first.
    pub matches: Vec<PersonFit>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersonFit {
    pub id: String,
    pub fit: Fit,
}

/// At most this many people are offered for one name.
const MAX_MATCHES: usize = 3;

/// The saved people a name could be, by their name or any match name, likeliest first.
fn person_fits(paid: &str, people: &[Person]) -> Vec<PersonFit> {
    let mut fits = people
        .iter()
        .filter_map(|person| {
            let fit = std::iter::once(person.name.as_str())
                .chain(person.match_names.iter().map(String::as_str))
                .filter_map(|name| name_fit(paid, name))
                .max()?;
            Some((fit, person))
        })
        .collect::<Vec<_>>();
    fits.sort_by(|(left_fit, left), (right_fit, right)| {
        right_fit
            .cmp(left_fit)
            .then_with(|| left.name.cmp(&right.name))
    });
    fits.into_iter()
        .take(MAX_MATCHES)
        .map(|(fit, person)| PersonFit {
            id: person.id.clone(),
            fit,
        })
        .collect()
}

/// Whether money moved to or from a person: Zelle, a payment app, or Payments To People.
pub fn person_to_person(t: &Transaction) -> bool {
    let text = format!(" {} ", match_text(&t.descriptors()));
    t.classification.as_ref().is_some_and(|c| c.zelle)
        || t.category == PEOPLE_CATEGORY
        || t.provider_category.as_deref() == Some(PEOPLE_CATEGORY)
        || t.counterparties
            .iter()
            .any(|counterparty| counterparty.r#type.as_deref() == Some("payment_app"))
        || [
            " ZELLE ",
            " VENMO ",
            " PAYPAL ",
            " CASH APP ",
            " CASHAPP ",
            " APPLE CASH ",
        ]
        .iter()
        .any(|app| text.contains(app))
}

/// Tags each payment to or from a person that nobody chose by hand with the person whose
/// longest match name its descriptor contains. Two people tied for the longest leave it
/// untagged. Ids of people since deleted drop out of hand choices too.
pub fn assign(transactions: &mut [Transaction], people: &[Person]) {
    let known = people
        .iter()
        .map(|person| person.id.as_str())
        .collect::<BTreeSet<_>>();
    for t in transactions.iter_mut() {
        if t.people_source == Some(PeopleSource::Manual) {
            t.people.retain(|id| known.contains(id.as_str()));
            continue;
        }
        t.people.clear();
        t.people_source = None;
        if !person_to_person(t) {
            continue;
        }
        if let Some(id) = best_match(&t.descriptors(), people) {
            t.people.push(id.to_owned());
            t.people_source = Some(PeopleSource::Matched);
        }
    }
}

fn best_match<'a>(descriptors: &str, people: &'a [Person]) -> Option<&'a str> {
    let text = format!(" {} ", match_text(descriptors));
    let mut best: Option<(usize, &str)> = None;
    let mut tied = false;
    for person in people {
        let Some(length) = person
            .match_names
            .iter()
            .filter(|name| text.contains(&format!(" {name} ")))
            .map(|name| name.len())
            .max()
        else {
            continue;
        };
        match best {
            Some((longest, _)) if length < longest => {}
            Some((longest, _)) if length == longest => tied = true,
            _ => {
                best = Some((length, person.id.as_str()));
                tied = false;
            }
        }
    }
    best.filter(|_| !tied).map(|(_, id)| id)
}

/// The counterparty a Zelle descriptor names: the words after `Zelle … to` or `from`, up to
/// the reference, the date, or the first word that isn't a name: one with a digit or other
/// punctuation, such as text a statement import ran on with (`*start*`).
pub fn zelle_name(descriptors: &str) -> Option<String> {
    let raw = descriptors.split_whitespace().collect::<Vec<_>>();
    let words = raw.iter().map(|word| match_text(word)).collect::<Vec<_>>();
    let name_word = |index: usize| {
        let word = raw[index];
        !words[index].is_empty()
            && !NAME_STOPS.contains(&words[index].as_str())
            && word
                .chars()
                .all(|c| c.is_alphabetic() || matches!(c, '\'' | '’' | '-' | '.'))
    };
    // The merchant may be a bare "Zelle" ahead of the descriptor that names someone.
    words
        .iter()
        .enumerate()
        .filter(|(_, word)| *word == "ZELLE")
        .find_map(|(start, _)| {
            let direction = words[start + 1..]
                .iter()
                .take(3)
                .position(|word| word == "TO" || word == "FROM")?;
            let first = start + 2 + direction;
            let name = (first..words.len())
                .take_while(|&index| name_word(index))
                .take(MAX_NAME_WORDS)
                .map(|index| words[index].as_str())
                .collect::<Vec<_>>();
            // A lone initial is a fragment, not a name.
            (name.iter().map(|word| word.chars().count()).sum::<usize>() >= 2)
                .then(|| name.join(" "))
        })
}

/// How well a name a payment carries fits someone's full name.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Only part of it: a first name alone, or a last name with a first name that starts
    /// the same way (Edmund and Eddie).
    Possible,
    /// Every word of their name is in it, or all of it is in theirs.
    Likely,
}

/// Whether `paid` (as a bank prints it) could be `name`. Words compare as banks print them,
/// so case and punctuation never matter.
pub fn name_fit(paid: &str, name: &str) -> Option<Fit> {
    let paid = match_text(paid);
    let name = match_text(name);
    let paid = paid
        .split(' ')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let name = name
        .split(' ')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let (Some(paid_first), Some(name_first)) = (paid.first(), name.first()) else {
        return None;
    };
    let within = |part: &[&str], whole: &[&str]| part.iter().all(|word| whole.contains(word));
    let shared = paid.iter().filter(|word| name.contains(word)).count();
    if shared >= 2 && (within(&name, &paid) || within(&paid, &name)) {
        return Some(Fit::Likely);
    }
    if paid_first == name_first && (paid.len() == 1 || name.len() == 1) {
        return Some(Fit::Possible);
    }
    let prefix = |word: &str| word.chars().take(2).collect::<String>();
    let same_last = paid.len() > 1 && name.len() > 1 && paid.last() == name.last();
    (same_last
        && paid_first.chars().count() > 1
        && name_first.chars().count() > 1
        && prefix(paid_first) == prefix(name_first))
    .then_some(Fit::Possible)
}

fn title_case(name: &str) -> String {
    name.split(' ')
        .map(|word| {
            let mut characters = word.chars();
            characters
                .next()
                .map(|first| first.to_string() + &characters.as_str().to_lowercase())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Each trip's companions: everyone saved for any trip sharing a transaction with it.
pub fn trip_companions(trips: &mut [Trip], saved: &[TripCompanions], people: &[Person]) {
    let known = people
        .iter()
        .map(|person| person.id.as_str())
        .collect::<BTreeSet<_>>();
    for trip in trips {
        let ids = trip
            .transaction_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut companions = Vec::<String>::new();
        for entry in saved.iter().filter(|entry| {
            entry
                .transaction_ids
                .iter()
                .any(|id| ids.contains(id.as_str()))
        }) {
            for person in &entry.people {
                if known.contains(person.as_str()) && !companions.contains(person) {
                    companions.push(person.clone());
                }
            }
        }
        trip.companions = companions;
    }
}

pub fn people(transactions: &[Transaction], people: &[Person], trips: &[Trip]) -> People {
    struct Tally {
        count: u32,
        sent: i64,
        received: i64,
        first: Option<NaiveDate>,
        last: Option<NaiveDate>,
    }
    let mut tallies = people
        .iter()
        .map(|person| {
            (
                person.id.as_str(),
                Tally {
                    count: 0,
                    sent: 0,
                    received: 0,
                    first: None,
                    last: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut suggestions = BTreeMap::<String, Vec<(NaiveDate, &str)>>::new();
    let known_names = people
        .iter()
        .flat_map(|person| &person.match_names)
        .collect::<BTreeSet<_>>();
    for t in transactions {
        let date = NaiveDate::parse_from_str(t.posted_date(), "%Y-%m-%d").ok();
        let paid = person_to_person(t);
        for id in &t.people {
            let Some(tally) = tallies.get_mut(id.as_str()) else {
                continue;
            };
            tally.count += 1;
            if paid {
                let cents = minor_units(t.amount);
                if cents < 0 {
                    tally.sent += cents;
                } else {
                    tally.received += cents;
                }
            }
            if let Some(date) = date {
                tally.first = Some(tally.first.map_or(date, |first| first.min(date)));
                tally.last = Some(tally.last.map_or(date, |last| last.max(date)));
            }
        }
        if t.people_source.is_none() && t.classification.as_ref().is_some_and(|c| c.zelle) {
            if let (Some(name), Some(date)) = (zelle_name(&t.descriptors()), date) {
                if !known_names.contains(&name) {
                    suggestions.entry(name).or_default().push((date, &t.id));
                }
            }
        }
    }
    let mut summaries = people
        .iter()
        .map(|person| {
            let tally = &tallies[person.id.as_str()];
            PersonSummary {
                id: person.id.clone(),
                transaction_count: tally.count,
                sent: tally.sent as f64 / 100.0,
                received: tally.received as f64 / 100.0,
                first: tally.first,
                last: tally.last,
                trip_ids: trips
                    .iter()
                    .filter(|trip| trip.companions.contains(&person.id))
                    .map(|trip| trip.id.clone())
                    .collect(),
            }
        })
        .collect::<Vec<_>>();
    let names = people
        .iter()
        .map(|person| (person.id.as_str(), person.name.to_lowercase()))
        .collect::<BTreeMap<_, _>>();
    summaries.sort_by(|left, right| {
        right
            .last
            .cmp(&left.last)
            .then_with(|| names[left.id.as_str()].cmp(&names[right.id.as_str()]))
    });
    let mut suggestions = suggestions
        .into_iter()
        .map(|(match_name, mut found)| {
            found.sort_by(|left, right| right.cmp(left));
            NameSuggestion {
                name: title_case(&match_name),
                matches: person_fits(&match_name, people),
                match_name,
                transaction_ids: found.into_iter().map(|(_, id)| id.to_owned()).collect(),
            }
        })
        .collect::<Vec<_>>();
    suggestions.sort_by(|left, right| {
        right
            .transaction_ids
            .len()
            .cmp(&left.transaction_ids.len())
            .then_with(|| left.name.cmp(&right.name))
    });
    suggestions.truncate(MAX_SUGGESTIONS);
    People {
        people: summaries,
        suggestions,
    }
}

#[cfg(test)]
mod tests;

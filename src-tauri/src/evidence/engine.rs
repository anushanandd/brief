//! Runs program rules over what Brief observed. Conditions bind in order, as a Datalog rule's
//! body joins its atoms, and every binding that meets them all yields a conclusion with its
//! proof: each condition, what met it, and the rule's source. A conclusion is Explained only
//! when every condition is met by observed records and the terms cover its dates; one with a
//! condition no record can show, or a date the terms don't cover, stays Suggested. Nothing is
//! concluded from a condition that failed.
use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};

use super::{
    employment::Employer,
    programs::{Condition, Direction, Program},
    Alert, AlertKind, Explanation, Proof, Step,
};
use crate::{
    finance_contract::{Account, Transaction},
    objects::{Kind, Tier},
    transaction_policy::Kind as Classified,
};

pub struct Facts<'a> {
    pub transactions: &'a [Transaction],
    pub accounts: &'a [Account],
    pub employers: &'a [Employer],
    pub today: NaiveDate,
}

/// One month's deposits to an account, and whether they reached the terms' amount.
#[derive(Clone, Debug)]
struct Month {
    first: NaiveDate,
    total: f64,
    threshold: f64,
    transactions: Vec<String>,
}

#[derive(Clone, Debug)]
enum Bound<'a> {
    Account(&'a Account),
    Transaction(&'a Transaction, NaiveDate),
    Employer(&'a Employer),
    Months(Vec<Month>),
}

impl Bound<'_> {
    fn id(&self) -> Option<String> {
        match self {
            Bound::Account(account) => Some(Kind::Account.id(&account.id)),
            Bound::Transaction(transaction, _) => Some(Kind::Transaction.id(&transaction.id)),
            Bound::Employer(employer) => Some(Kind::Organization.id(&employer.key)),
            Bound::Months(_) => None,
        }
    }

    fn date(&self) -> Option<NaiveDate> {
        match self {
            Bound::Transaction(_, date) => Some(*date),
            Bound::Employer(employer) => Some(employer.first),
            _ => None,
        }
    }
}

/// A partial match: what is bound, and the proof so far.
#[derive(Clone, Debug)]
struct Match<'a> {
    bound: BTreeMap<String, Bound<'a>>,
    steps: Vec<Step>,
    unchecked: Vec<String>,
    /// A date the terms didn't cover, which keeps the conclusion Suggested.
    uncovered: bool,
}

fn date_of(transaction: &Transaction) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(transaction.posted_date().get(..10)?, "%Y-%m-%d").ok()
}

fn money(amount: f64) -> String {
    let cents = (amount.abs() * 100.0).round() as i64;
    let dollars = cents / 100;
    let mut grouped = dollars.to_string();
    let mut index = grouped.len() as i64 - 3;
    while index > 0 {
        grouped.insert(index as usize, ',');
        index -= 3;
    }
    if cents % 100 == 0 {
        format!("${grouped}")
    } else {
        format!("${grouped}.{:02}", cents % 100)
    }
}

fn text(transaction: &Transaction) -> String {
    [
        Some(transaction.merchant.as_str()),
        transaction.description.as_deref(),
        transaction.original_description.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
}

/// A month's first day, and the first day of the next.
fn month_of(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
}

fn next_month(date: NaiveDate) -> NaiveDate {
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(date)
}

/// A deposit that counts toward monthly deposits: income, or a transfer from an outside
/// institution; never interest, a refund, a bonus, or a move between the same bank's
/// accounts.
fn counts_as_deposit(transaction: &Transaction, account: &Account, accounts: &[Account]) -> bool {
    let Some(classification) = &transaction.classification else {
        return false;
    };
    if transaction.amount <= 0.0 || transaction.pending {
        return false;
    }
    match classification.kind {
        Classified::Income => true,
        Classified::Transfer => !transaction.transfer_pair_id.as_ref().is_some_and(|pair| {
            // The other leg's account, when Brief tracks it, must be at another institution.
            accounts.iter().any(|other| {
                other.institution.eq_ignore_ascii_case(&account.institution)
                    && other.id != account.id
                    && pair.contains(other.id.trim_start_matches("plaid:"))
            })
        }),
        _ => false,
    }
}

fn evaluate_condition<'a>(
    program: &Program,
    condition: &Condition,
    facts: &Facts<'a>,
    current: Match<'a>,
) -> Vec<Match<'a>> {
    let step = |current: &Match<'a>, alias: &str, bound: Bound<'a>, detail: Option<String>| {
        let mut next = current.clone();
        next.steps.push(Step {
            says: condition.says().to_owned(),
            evidence: bound.id().into_iter().collect(),
            detail,
            met: true,
        });
        next.bound.insert(alias.to_owned(), bound);
        next
    };
    match condition {
        Condition::Account {
            alias,
            name,
            institution,
            ..
        } => {
            let name = name.as_deref().and_then(super::programs::matcher);
            let institution = institution.as_deref().and_then(super::programs::matcher);
            facts
                .accounts
                .iter()
                .filter(|account| account.id != "all")
                .filter(|account| {
                    name.as_ref().is_none_or(|name| {
                        name.is_match(&account.name)
                            || account
                                .official_name
                                .as_deref()
                                .is_some_and(|official| name.is_match(official))
                    }) && institution
                        .as_ref()
                        .is_none_or(|institution| institution.is_match(&account.institution))
                })
                .map(|account| {
                    step(
                        &current,
                        alias,
                        Bound::Account(account),
                        Some(account.name.clone()),
                    )
                })
                .collect()
        }
        Condition::Card {
            alias,
            official_name,
            ..
        } => {
            let Some(pattern) = super::programs::matcher(official_name) else {
                return Vec::new();
            };
            facts
                .accounts
                .iter()
                .filter(|account| account.r#type == "credit")
                .filter_map(|account| {
                    let official = account.official_name.as_deref()?;
                    pattern.is_match(official).then(|| {
                        step(
                            &current,
                            alias,
                            Bound::Account(account),
                            Some(official.to_owned()),
                        )
                    })
                })
                .collect()
        }
        Condition::Transaction {
            alias,
            account,
            description,
            direction,
            amount_term,
            before,
            ..
        } => {
            let (Some(Bound::Account(account)), Some(pattern)) = (
                current.bound.get(account),
                super::programs::matcher(description),
            ) else {
                return Vec::new();
            };
            let anchor = before.as_ref().and_then(|before| {
                current
                    .bound
                    .get(&before.of)
                    .and_then(Bound::date)
                    .map(|date| (date, before.days))
            });
            facts
                .transactions
                .iter()
                .filter(|transaction| {
                    !transaction.pending
                        && transaction.account_id.as_deref() == Some(account.id.as_str())
                        && match direction {
                            Direction::In => transaction.amount > 0.0,
                            Direction::Out => transaction.amount < 0.0,
                        }
                        && pattern.is_match(&text(transaction))
                })
                .filter_map(|transaction| {
                    let date = date_of(transaction)?;
                    if let Some((anchor, days)) = anchor {
                        if date > anchor || (anchor - date).num_days() > days {
                            return None;
                        }
                    }
                    let mut uncovered = false;
                    if let Some(term) = amount_term {
                        match program.values(term, date) {
                            Some(values) => {
                                if !values
                                    .iter()
                                    .any(|value| (value - transaction.amount.abs()).abs() < 0.005)
                                {
                                    return None;
                                }
                            }
                            None => uncovered = true,
                        }
                    }
                    let detail = format!("{} on {date}", money(transaction.amount));
                    let mut next = step(
                        &current,
                        alias,
                        Bound::Transaction(transaction, date),
                        Some(detail),
                    );
                    if uncovered {
                        next.uncovered = true;
                        if let Some(last) = next.steps.last_mut() {
                            last.detail = Some(format!(
                                "{} on {date}; the terms Brief has don't cover that date",
                                money(transaction.amount)
                            ));
                        }
                    }
                    Some(next)
                })
                .collect()
        }
        Condition::Employment {
            alias,
            organization,
            ..
        } => {
            let Some(pattern) = super::programs::matcher(organization) else {
                return Vec::new();
            };
            facts
                .employers
                .iter()
                .filter(|employer| pattern.is_match(&employer.name))
                .map(|employer| {
                    let detail = format!(
                        "{} from {}{}",
                        employer.name,
                        employer.first,
                        if employer.ongoing {
                            String::new()
                        } else {
                            format!(" to {}", employer.last)
                        }
                    );
                    let mut next = step(&current, alias, Bound::Employer(employer), Some(detail));
                    if let Some(last) = next.steps.last_mut() {
                        last.evidence.extend(employer.documents.iter().cloned());
                    }
                    next
                })
                .collect()
        }
        Condition::MonthlyDeposits {
            alias,
            account,
            at_least_term,
            during,
            ..
        } => {
            let Some(Bound::Account(account)) = current.bound.get(account) else {
                return Vec::new();
            };
            let (start, end) = match during
                .as_deref()
                .and_then(|during| current.bound.get(during))
            {
                Some(Bound::Employer(employer)) => (
                    employer.first,
                    if employer.ongoing {
                        facts.today
                    } else {
                        employer.last
                    },
                ),
                _ => (
                    facts
                        .transactions
                        .iter()
                        .filter_map(date_of)
                        .min()
                        .unwrap_or(facts.today),
                    facts.today,
                ),
            };
            // Not before the account shows activity, and only complete months.
            let opened = facts
                .transactions
                .iter()
                .filter(|transaction| {
                    transaction.account_id.as_deref() == Some(account.id.as_str())
                })
                .filter_map(date_of)
                .min();
            let start = opened.map_or(start, |opened| start.max(opened));
            let this_month = month_of(facts.today);
            let mut months = Vec::new();
            let mut month = month_of(start);
            while month < this_month && month <= end {
                let Some(threshold) = program
                    .values(at_least_term, month)
                    .and_then(|values| values.iter().copied().reduce(f64::max))
                else {
                    month = next_month(month);
                    continue;
                };
                let deposits = facts
                    .transactions
                    .iter()
                    .filter(|transaction| {
                        transaction.account_id.as_deref() == Some(account.id.as_str())
                    })
                    .filter(|transaction| {
                        date_of(transaction).is_some_and(|date| month_of(date) == month)
                    })
                    .filter(|transaction| counts_as_deposit(transaction, account, facts.accounts))
                    .collect::<Vec<_>>();
                months.push(Month {
                    first: month,
                    total: deposits.iter().map(|transaction| transaction.amount).sum(),
                    threshold,
                    transactions: deposits
                        .iter()
                        .map(|transaction| Kind::Transaction.id(&transaction.id))
                        .collect(),
                });
                month = next_month(month);
            }
            let met = months
                .iter()
                .filter(|month| month.total + 0.005 >= month.threshold)
                .count();
            if met == 0 {
                // The watch below can still note a month that fell short.
                let mut next = current.clone();
                next.bound.insert(alias.clone(), Bound::Months(months));
                next.steps.push(Step {
                    says: condition.says().to_owned(),
                    evidence: Vec::new(),
                    detail: Some("No month reached it".into()),
                    met: false,
                });
                return vec![next];
            }
            let detail = format!("{met} of {} months", months.len());
            let mut next = current.clone();
            next.steps.push(Step {
                says: condition.says().to_owned(),
                evidence: months
                    .iter()
                    .filter(|month| month.total + 0.005 >= month.threshold)
                    .flat_map(|month| month.transactions.iter().cloned())
                    .collect(),
                detail: Some(detail),
                met: true,
            });
            next.bound.insert(alias.clone(), Bound::Months(months));
            vec![next]
        }
        Condition::Unobservable { says } => {
            let mut next = current;
            next.unchecked.push(says.clone());
            vec![next]
        }
    }
}

/// `{alias}` as its amount and `{alias.date}` as its date, in a rule's statement.
fn fill(template: &str, bound: &BTreeMap<String, Bound<'_>>, extra: &[(&str, String)]) -> String {
    let mut out = template.to_owned();
    for (alias, value) in bound {
        if let Bound::Transaction(transaction, date) = value {
            out = out
                .replace(&format!("{{{alias}.date}}"), &date.to_string())
                .replace(&format!("{{{alias}}}"), &money(transaction.amount));
        }
    }
    for (name, value) in extra {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// Runs of consecutive met months, as (first day, last day, months).
fn runs(months: &[Month]) -> Vec<(NaiveDate, NaiveDate, Vec<&Month>)> {
    let mut runs: Vec<(NaiveDate, NaiveDate, Vec<&Month>)> = Vec::new();
    for month in months
        .iter()
        .filter(|month| month.total + 0.005 >= month.threshold)
    {
        let last_day = next_month(month.first) - chrono::Duration::days(1);
        match runs.last_mut() {
            Some(run) if next_month(month_of(run.1)) == month.first => {
                run.1 = last_day;
                run.2.push(month);
            }
            _ => runs.push((month.first, last_day, vec![month])),
        }
    }
    runs
}

pub fn evaluate(program: &Program, facts: &Facts<'_>) -> (Vec<Explanation>, Vec<Alert>) {
    let mut matches = vec![Match {
        bound: BTreeMap::new(),
        steps: Vec::new(),
        unchecked: Vec::new(),
        uncovered: false,
    }];
    // The partial matches just before the condition a rule expects to follow, for checking
    // it followed.
    let mut before_missing = Vec::new();
    for condition in &program.conditions {
        if program
            .expect
            .as_ref()
            .is_some_and(|expect| condition.alias() == Some(expect.missing.as_str()))
        {
            before_missing = matches.clone();
        }
        let mut next = Vec::new();
        for current in matches {
            next.extend(evaluate_condition(program, condition, facts, current));
        }
        matches = next;
    }
    let mut explanations = Vec::new();
    let mut alerts = Vec::new();
    let conclusion = &program.conclusion;
    for found in &matches {
        let id_of = |alias: &str| found.bound.get(alias).and_then(Bound::id);
        let Some(subject) = id_of(&conclusion.subject) else {
            continue;
        };
        let object = conclusion.object.as_deref().and_then(id_of);
        let all_met = found.steps.iter().all(|step| step.met);
        let tier = if all_met && found.unchecked.is_empty() && !found.uncovered {
            Tier::Explained
        } else {
            Tier::Suggested
        };
        let proof = Proof {
            program: program.id.clone(),
            name: program.name.clone(),
            source: program.source.clone(),
            reviewed: program.reviewed,
            steps: found.steps.clone(),
            unchecked: found.unchecked.clone(),
            notes: program.notes.clone(),
        };
        let spans = conclusion
            .spans
            .as_deref()
            .and_then(|alias| match found.bound.get(alias) {
                Some(Bound::Months(months)) => Some(runs(months)),
                _ => None,
            });
        if let Some(Bound::Months(months)) = program
            .watch
            .as_ref()
            .and_then(|watch| found.bound.get(&watch.condition))
        {
            // The latest complete month falling short of the terms is worth a note.
            if let (Some(watch), Some(last)) = (&program.watch, months.last()) {
                if last.total + 0.005 < last.threshold {
                    alerts.push(Alert {
                        id: format!("{}:watch:{}", program.id, last.first.format("%Y-%m")),
                        program: program.id.clone(),
                        kind: AlertKind::Watch,
                        says: fill(
                            &watch.says,
                            &found.bound,
                            &[
                                ("deposits", money(last.threshold)),
                                ("month", last.first.format("%B %Y").to_string()),
                            ],
                        ),
                        date: last.first,
                        subject: subject.clone(),
                    });
                }
            }
        }
        if let Some(ends) = &program.ends {
            if let Some(Bound::Employer(employer)) = found.bound.get(&ends.with) {
                if !employer.ongoing {
                    alerts.push(Alert {
                        id: format!("{}:ends:{}", program.id, employer.key),
                        program: program.id.clone(),
                        kind: AlertKind::Ends,
                        says: ends.says.clone(),
                        date: employer.last,
                        subject: Kind::Organization.id(&employer.key),
                    });
                }
            }
        }
        if !all_met {
            continue;
        }
        let says = fill(&conclusion.says, &found.bound, &[]);
        let key = found
            .bound
            .values()
            .filter_map(Bound::id)
            .collect::<Vec<_>>()
            .join("|");
        match spans {
            Some(spans) => {
                for (from, to, _) in spans {
                    explanations.push(Explanation {
                        id: format!("{}:{}:{from}", program.id, subject),
                        program: program.id.clone(),
                        title: conclusion.title.clone(),
                        says: says.clone(),
                        tier,
                        subject: subject.clone(),
                        predicate: conclusion.predicate.clone(),
                        object: object.clone(),
                        from: Some(from),
                        to: Some(to),
                        proof: proof.clone(),
                    });
                }
            }
            None => explanations.push(Explanation {
                id: format!(
                    "{}:{}",
                    program.id,
                    crate::database::sha256(key.as_bytes())[..16].to_owned()
                ),
                program: program.id.clone(),
                title: conclusion.title.clone(),
                says,
                tier,
                subject,
                predicate: conclusion.predicate.clone(),
                object,
                from: found.bound.get(&conclusion.subject).and_then(Bound::date),
                to: None,
                proof,
            }),
        }
    }
    if let Some(expect) = &program.expect {
        // What should have followed and didn't: the latest anchor (a renewal's fee), long
        // enough ago, with nothing the missing condition binds on or after it.
        let missing = program
            .conditions
            .iter()
            .find(|condition| condition.alias() == Some(expect.missing.as_str()));
        for partial in &before_missing {
            let (Some(missing), Some((transaction, date))) = (
                missing,
                anchor_after(program, facts, partial, &expect.after),
            ) else {
                continue;
            };
            let followed = evaluate_condition(program, missing, facts, partial.clone())
                .iter()
                .any(|found| {
                    found
                        .bound
                        .get(&expect.missing)
                        .and_then(Bound::date)
                        .is_some_and(|followed| followed >= date)
                });
            if !followed && (facts.today - date).num_days() >= expect.days {
                let mut bound = partial.bound.clone();
                bound.insert(expect.after.clone(), Bound::Transaction(transaction, date));
                alerts.push(Alert {
                    id: format!("{}:expect:{}", program.id, transaction.id),
                    program: program.id.clone(),
                    kind: AlertKind::Expect,
                    says: fill(&expect.says, &bound, &[]),
                    date,
                    subject: Kind::Transaction.id(&transaction.id),
                });
            }
        }
    }
    explanations.sort_by(|left, right| left.id.cmp(&right.id));
    explanations.dedup_by(|left, right| left.id == right.id);
    alerts.sort_by(|left, right| left.id.cmp(&right.id));
    alerts.dedup_by(|left, right| left.id == right.id);
    (explanations, alerts)
}

/// The latest transaction a later condition `alias` would bind, when the conditions before
/// the missing one were met: the fee a bonus should follow.
fn anchor_after<'a>(
    program: &Program,
    facts: &Facts<'a>,
    partial: &Match<'a>,
    alias: &str,
) -> Option<(&'a Transaction, NaiveDate)> {
    let condition = program
        .conditions
        .iter()
        .find(|condition| condition.alias() == Some(alias))?;
    // Bind it without its relation to the missing binding.
    let unanchored = match condition {
        Condition::Transaction {
            alias,
            says,
            account,
            description,
            direction,
            amount_term,
            ..
        } => Condition::Transaction {
            alias: alias.clone(),
            says: says.clone(),
            account: account.clone(),
            description: description.clone(),
            direction: *direction,
            amount_term: amount_term.clone(),
            before: None,
        },
        _ => return None,
    };
    evaluate_condition(program, &unanchored, facts, partial.clone())
        .into_iter()
        .filter_map(|found| match found.bound.get(alias) {
            Some(Bound::Transaction(transaction, date)) => Some((*transaction, *date)),
            _ => None,
        })
        .max_by_key(|(_, date)| *date)
}

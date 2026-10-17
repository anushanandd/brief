//! Life chapters: spans with a cause (a job, a home, a burst of new accounts, a shift in
//! income or spending), detected on each read from what Brief observed. Each names the
//! records that define it (anchors), the things that belong to it (members), the rules that
//! depend on it, how monthly income and spending changed at its start, and what to do when it
//! ends. Detected chapters are Suggested until the user confirms one; the user's title and
//! dates override, a dismissal is kept so the same chapter is never suggested again, and
//! chapters the user plans ahead stand beside the detected ones.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use ts_rs::TS;

use super::employment::{Employer, Precision};
use crate::{
    finance_contract::{Account, Transaction},
    objects::{Kind, Tier},
    settings::{ChapterChoice, ChapterStatus, PlannedChapter},
    transaction_policy::Kind as Classified,
};

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ChapterKind {
    Work,
    Home,
    /// Accounts opened together, a large purchase.
    Milestone,
    /// A step change in income or spending no source states.
    Shift,
    /// One the user planned ahead.
    Planned,
}

/// A record that defines a chapter, as a proof step states it.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ChapterAnchor")]
pub struct Anchor {
    pub says: String,
    /// Object ids.
    pub evidence: Vec<String>,
}

/// Something that belongs to a chapter, and how surely.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ChapterMember")]
pub struct Member {
    pub id: String,
    /// Observed when a record ties it to the chapter; Suggested when only its timing does.
    pub tier: Tier,
    pub says: String,
}

/// A monthly average before and after a chapter began.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ChapterEffect")]
pub struct Effect {
    /// `income` or `spending`.
    pub measure: String,
    pub before: f64,
    pub after: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Chapter {
    /// Stable across reads: the kind and what it is about, never its dates alone.
    pub id: String,
    pub kind: ChapterKind,
    pub title: String,
    pub start: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end: Option<NaiveDate>,
    pub precision: Precision,
    /// Suggested until confirmed; a planned chapter is the user's own, so Observed.
    pub tier: Tier,
    pub confirmed: bool,
    /// For a home, the US state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub state: Option<String>,
    pub anchors: Vec<Anchor>,
    pub members: Vec<Member>,
    /// Program rules that depend on it, by id.
    pub programs: Vec<String>,
    pub effects: Vec<Effect>,
    /// What changes or needs doing as it ends.
    pub checklist: Vec<String>,
    /// For a planned chapter, its expected monthly costs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planned_costs: Vec<crate::settings::PlannedCost>,
}

pub struct Inputs<'a> {
    pub transactions: &'a [Transaction],
    pub accounts: &'a [Account],
    pub employers: &'a [Employer],
    /// Transaction ids made away from home, which say nothing about where home is.
    pub away: &'a BTreeSet<String>,
    /// Which program rules each employer's chapter enables, by employer key.
    pub programs: &'a BTreeMap<String, Vec<String>>,
    /// What each employer's end changes, by employer key.
    pub endings: &'a BTreeMap<String, Vec<String>>,
    pub choices: &'a [ChapterChoice],
    pub planned: &'a [PlannedChapter],
    pub today: NaiveDate,
}

fn date_of(transaction: &Transaction) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(transaction.posted_date().get(..10)?, "%Y-%m-%d").ok()
}

fn month_of(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
}

fn add_months(date: NaiveDate, months: i32) -> NaiveDate {
    let index = date.year() * 12 + date.month0() as i32 + months;
    NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1)
        .unwrap_or(date)
}

fn end_of_month(month: NaiveDate) -> NaiveDate {
    add_months(month, 1) - Duration::days(1)
}

/// Monthly income and spending, by month, over complete months.
pub(super) fn monthly(
    transactions: &[Transaction],
    today: NaiveDate,
) -> BTreeMap<NaiveDate, (f64, f64)> {
    let mut months = BTreeMap::<NaiveDate, (f64, f64)>::new();
    let current = month_of(today);
    for transaction in transactions
        .iter()
        .filter(|transaction| !transaction.pending)
    {
        let (Some(date), Some(classification)) =
            (date_of(transaction), &transaction.classification)
        else {
            continue;
        };
        let month = month_of(date);
        if month >= current {
            continue;
        }
        let entry = months.entry(month).or_default();
        if classification.kind == Classified::Income && transaction.amount > 0.0 {
            entry.0 += transaction.amount;
        }
        if classification.spending {
            entry.1 -= transaction.amount;
        }
    }
    months
}

/// Where you are now, for Home's one line: "Employer · State · month N".
#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ChapterNow")]
pub struct Now {
    /// The open work chapter that began most recently.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub work: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub work_title: Option<String>,
    /// The open home chapter's state name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub home: Option<String>,
    /// Which month of the work chapter this is, the first being 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub month: Option<u32>,
}

/// The open chapters you are in today.
pub fn now(chapters: &[Chapter], today: NaiveDate) -> Now {
    let open = |kind: ChapterKind| {
        chapters
            .iter()
            .filter(move |chapter| chapter.kind == kind && chapter.start <= today)
            .filter(|chapter| chapter.end.is_none_or(|end| end >= today))
            .max_by_key(|chapter| (chapter.confirmed, chapter.start))
    };
    let work = open(ChapterKind::Work);
    Now {
        work: work.map(|chapter| chapter.id.clone()),
        work_title: work.map(|chapter| chapter.title.clone()),
        home: open(ChapterKind::Home).and_then(|chapter| {
            chapter
                .state
                .as_deref()
                .and_then(state_name)
                .map(str::to_owned)
        }),
        month: work.map(|chapter| {
            let months = (today.year() - chapter.start.year()) * 12 + today.month() as i32
                - chapter.start.month() as i32
                - i32::from(today.day() < chapter.start.day());
            months.max(0) as u32 + 1
        }),
    }
}

/// Where you lived during part of a tax year, from home chapters, which makes a part-year
/// return visible.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Residency {
    pub year: i32,
    pub state: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub days: i64,
    /// The home chapter's tier: Suggested until you confirm where you lived.
    pub tier: Tier,
    pub chapter: String,
}

/// Each year's residency, by the home chapters that overlap it, through today.
pub fn residency(chapters: &[Chapter], today: NaiveDate) -> Vec<Residency> {
    let mut spans = Vec::new();
    for chapter in chapters
        .iter()
        .filter(|chapter| chapter.kind == ChapterKind::Home)
    {
        let Some(state) = chapter.state.clone() else {
            continue;
        };
        let end = chapter.end.unwrap_or(today).min(today);
        for year in chapter.start.year()..=end.year() {
            let (Some(january), Some(december)) = (
                NaiveDate::from_ymd_opt(year, 1, 1),
                NaiveDate::from_ymd_opt(year, 12, 31),
            ) else {
                continue;
            };
            let (from, to) = (chapter.start.max(january), end.min(december));
            if from <= to {
                spans.push(Residency {
                    year,
                    state: state.clone(),
                    from,
                    to,
                    days: (to - from).num_days() + 1,
                    tier: chapter.tier,
                    chapter: chapter.id.clone(),
                });
            }
        }
    }
    spans.sort_by_key(|span| (span.year, span.from));
    spans
}

/// Where the current normal starts: the latest start, by today, of a job, home, or shift you
/// confirmed. Typical spending and usual amounts read only from then, so an old rent or
/// salary never skews the new normal. Suggestions never move a baseline.
pub fn baseline(chapters: &[Chapter], today: NaiveDate) -> Option<NaiveDate> {
    chapters
        .iter()
        .filter(|chapter| chapter.confirmed)
        .filter(|chapter| {
            matches!(
                chapter.kind,
                ChapterKind::Work | ChapterKind::Home | ChapterKind::Shift
            )
        })
        .map(|chapter| chapter.start)
        .filter(|start| *start <= today)
        .max()
}

/// A month with fewer records than this is thin: history Brief barely has, not a quiet life.
const COVERED_RECORDS: usize = 10;

/// The months from the first three in a row that are well covered: before them, a low total
/// means missing history, which a shift would misread as a change in life.
fn covered(
    months: &BTreeMap<NaiveDate, (f64, f64)>,
    transactions: &[Transaction],
) -> BTreeMap<NaiveDate, (f64, f64)> {
    let mut counts = BTreeMap::<NaiveDate, usize>::new();
    for date in transactions.iter().filter_map(date_of) {
        *counts.entry(month_of(date)).or_default() += 1;
    }
    let keys = months.keys().copied().collect::<Vec<_>>();
    let well =
        |month: &NaiveDate| counts.get(month).copied().unwrap_or_default() >= COVERED_RECORDS;
    let start = keys
        .windows(3)
        .find(|run| run.iter().all(well) && add_months(run[0], 2) == run[2])
        .map(|run| run[0]);
    match start {
        Some(start) => months
            .range(start..)
            .map(|(month, value)| (*month, *value))
            .collect(),
        None => BTreeMap::new(),
    }
}

/// Average income and spending over up to three complete months before a date and the three
/// from it; none when either side has no month.
fn effects(months: &BTreeMap<NaiveDate, (f64, f64)>, start: NaiveDate) -> Vec<Effect> {
    let start = month_of(start);
    let side = |from: NaiveDate, to: NaiveDate| {
        let values = months
            .range(from..to)
            .map(|(_, value)| *value)
            .collect::<Vec<_>>();
        (!values.is_empty()).then(|| {
            let count = values.len() as f64;
            (
                values.iter().map(|value| value.0).sum::<f64>() / count,
                values.iter().map(|value| value.1).sum::<f64>() / count,
            )
        })
    };
    let (Some(before), Some(after)) = (
        side(add_months(start, -3), start),
        side(start, add_months(start, 3)),
    ) else {
        return Vec::new();
    };
    let round = |value: f64| (value * 100.0).round() / 100.0;
    vec![
        Effect {
            measure: "income".into(),
            before: round(before.0),
            after: round(after.0),
        },
        Effect {
            measure: "spending".into(),
            before: round(before.1),
            after: round(after.1),
        },
    ]
}

/// When each account first shows activity, leaving out accounts whose history begins with
/// Brief's own window, which says nothing about when they opened.
fn first_activity<'a>(
    transactions: &[Transaction],
    accounts: &'a [Account],
) -> Vec<(NaiveDate, &'a Account)> {
    let earliest = transactions.iter().filter_map(date_of).min();
    let mut first = BTreeMap::<&str, NaiveDate>::new();
    for transaction in transactions {
        if let (Some(account), Some(date)) =
            (transaction.account_id.as_deref(), date_of(transaction))
        {
            let entry = first.entry(account).or_insert(date);
            *entry = (*entry).min(date);
        }
    }
    accounts
        .iter()
        .filter_map(|account| {
            let date = *first.get(account.id.as_str())?;
            // Within a month of the earliest record anywhere is the window's edge.
            (earliest.is_some_and(|earliest| date - earliest > Duration::days(30)))
                .then_some((date, account))
        })
        .collect()
}

const STATES: [(&str, &str); 51] = [
    ("AL", "Alabama"),
    ("AK", "Alaska"),
    ("AZ", "Arizona"),
    ("AR", "Arkansas"),
    ("CA", "California"),
    ("CO", "Colorado"),
    ("CT", "Connecticut"),
    ("DE", "Delaware"),
    ("DC", "Washington, D.C."),
    ("FL", "Florida"),
    ("GA", "Georgia"),
    ("HI", "Hawaii"),
    ("ID", "Idaho"),
    ("IL", "Illinois"),
    ("IN", "Indiana"),
    ("IA", "Iowa"),
    ("KS", "Kansas"),
    ("KY", "Kentucky"),
    ("LA", "Louisiana"),
    ("ME", "Maine"),
    ("MD", "Maryland"),
    ("MA", "Massachusetts"),
    ("MI", "Michigan"),
    ("MN", "Minnesota"),
    ("MS", "Mississippi"),
    ("MO", "Missouri"),
    ("MT", "Montana"),
    ("NE", "Nebraska"),
    ("NV", "Nevada"),
    ("NH", "New Hampshire"),
    ("NJ", "New Jersey"),
    ("NM", "New Mexico"),
    ("NY", "New York"),
    ("NC", "North Carolina"),
    ("ND", "North Dakota"),
    ("OH", "Ohio"),
    ("OK", "Oklahoma"),
    ("OR", "Oregon"),
    ("PA", "Pennsylvania"),
    ("RI", "Rhode Island"),
    ("SC", "South Carolina"),
    ("SD", "South Dakota"),
    ("TN", "Tennessee"),
    ("TX", "Texas"),
    ("UT", "Utah"),
    ("VT", "Vermont"),
    ("VA", "Virginia"),
    ("WA", "Washington"),
    ("WV", "West Virginia"),
    ("WI", "Wisconsin"),
    ("WY", "Wyoming"),
];

pub fn state_name(code: &str) -> Option<&'static str> {
    STATES
        .iter()
        .find(|(abbreviation, _)| *abbreviation == code)
        .map(|(_, name)| *name)
}

/// A month counts toward a home when at least this many in-person purchases name a state,
/// and the most common has at least this share of them.
const HOME_PURCHASES: usize = 6;
const HOME_SHARE: f64 = 0.6;

fn work(
    inputs: &Inputs,
    months: &BTreeMap<NaiveDate, (f64, f64)>,
    opened: &[(NaiveDate, &Account)],
) -> Vec<Chapter> {
    inputs
        .employers
        .iter()
        .map(|employer| {
            let mut anchors = Vec::new();
            if !employer.documents.is_empty() {
                anchors.push(Anchor {
                    says: format!("Pay statements and tax forms from {}", employer.name),
                    evidence: employer.documents.clone(),
                });
            }
            if let Some((date, amount, id)) = employer.deposits.first() {
                anchors.push(Anchor {
                    says: format!("The first payroll deposit, ${amount:.2} on {date}"),
                    evidence: vec![Kind::Transaction.id(id)],
                });
            }
            let mut members = vec![Member {
                id: Kind::Organization.id(&employer.key),
                tier: Tier::Observed,
                says: "The employer".into(),
            }];
            members.extend(employer.documents.iter().map(|id| Member {
                id: id.clone(),
                tier: Tier::Observed,
                says: "A document from this employer".into(),
            }));
            // Accounts opened around a job's start may be for it, which only timing suggests;
            // each goes to the documented job whose start is nearest.
            let nearest = |date: NaiveDate| {
                inputs
                    .employers
                    .iter()
                    .filter(|other| !other.inferred)
                    .filter(|other| {
                        date >= other.first - Duration::days(30)
                            && date <= other.first + Duration::days(60)
                    })
                    .min_by_key(|other| (date - other.first).num_days().abs())
                    .map(|other| other.key.as_str())
            };
            members.extend(
                opened
                    .iter()
                    .filter(|(date, _)| nearest(*date) == Some(employer.key.as_str()))
                    .map(|(date, account)| Member {
                        id: Kind::Account.id(&account.id),
                        tier: Tier::Suggested,
                        says: format!("Opened {date}, near the start of this job"),
                    }),
            );
            let mut checklist = inputs
                .endings
                .get(&employer.key)
                .cloned()
                .unwrap_or_default();
            if !employer.ongoing && !employer.inferred {
                checklist.push(format!(
                    "Check the final paycheck from {} against the last pay statement or W-2.",
                    employer.name
                ));
            }
            Chapter {
                id: format!("work:{}", employer.key),
                kind: ChapterKind::Work,
                title: employer.name.clone(),
                start: employer.first,
                end: (!employer.ongoing).then_some(employer.last),
                precision: employer.precision,
                tier: Tier::Suggested,
                confirmed: false,
                state: None,
                anchors,
                members,
                programs: inputs
                    .programs
                    .get(&employer.key)
                    .cloned()
                    .unwrap_or_default(),
                effects: effects(months, employer.first),
                checklist,
                planned_costs: Vec::new(),
            }
        })
        .collect()
}

fn homes(inputs: &Inputs) -> Vec<Chapter> {
    // Each month's state: by in-person purchases made at home, then by the state that withheld
    // on a W-2 for a job that month.
    let mut votes = BTreeMap::<NaiveDate, BTreeMap<String, Vec<String>>>::new();
    for transaction in inputs.transactions {
        if transaction.payment_channel.as_deref() != Some("in store")
            || inputs.away.contains(&transaction.id)
        {
            continue;
        }
        let (Some(date), Some(region)) = (
            date_of(transaction),
            transaction
                .location
                .as_ref()
                .and_then(|location| location.region.clone())
                .filter(|region| state_name(region).is_some()),
        ) else {
            continue;
        };
        votes
            .entry(month_of(date))
            .or_default()
            .entry(region)
            .or_default()
            .push(Kind::Transaction.id(&transaction.id));
    }
    let mut month_state = BTreeMap::<NaiveDate, (String, Anchor)>::new();
    for (month, regions) in &votes {
        let total = regions.values().map(Vec::len).sum::<usize>();
        let Some((region, ids)) = regions.iter().max_by_key(|(_, ids)| ids.len()) else {
            continue;
        };
        if total >= HOME_PURCHASES && ids.len() as f64 / total as f64 >= HOME_SHARE {
            month_state.insert(
                *month,
                (
                    region.clone(),
                    Anchor {
                        says: format!(
                            "{} of {total} in-person purchases in {} were in {region}",
                            ids.len(),
                            month.format("%B %Y")
                        ),
                        evidence: ids.iter().take(5).cloned().collect(),
                    },
                ),
            );
        }
    }
    for employer in inputs.employers {
        for (year, state) in &employer.states {
            let mut month = month_of(
                employer
                    .first
                    .max(NaiveDate::from_ymd_opt(*year, 1, 1).unwrap_or(employer.first)),
            );
            let last = employer
                .last
                .min(NaiveDate::from_ymd_opt(*year, 12, 31).unwrap_or(employer.last));
            while month <= last {
                month_state.entry(month).or_insert_with(|| {
                    (
                        state.clone(),
                        Anchor {
                            says: format!(
                                "{}'s W-2 for {year} withheld for {state}",
                                employer.name
                            ),
                            evidence: employer.documents.clone(),
                        },
                    )
                });
                month = add_months(month, 1);
            }
        }
    }
    // Runs of months in one state, broken by a different state; a gap without evidence joins
    // the run when the state on both sides agrees.
    let mut chapters: Vec<Chapter> = Vec::new();
    for (month, (state, anchor)) in month_state {
        if let Some(last) = chapters.last_mut() {
            if last.state.as_deref() == Some(state.as_str()) {
                last.end = Some(end_of_month(month));
                if !last.anchors.iter().any(|known| known.says == anchor.says)
                    && last.anchors.len() < 6
                {
                    last.anchors.push(anchor);
                }
                continue;
            }
        }
        let name = state_name(&state).unwrap_or(&state).to_owned();
        chapters.push(Chapter {
            id: format!("home:{}:{}", state.to_lowercase(), month.format("%Y-%m")),
            kind: ChapterKind::Home,
            title: format!("Lived in {name}"),
            start: month,
            end: Some(end_of_month(month)),
            precision: Precision::Month,
            tier: Tier::Suggested,
            confirmed: false,
            state: Some(state),
            anchors: vec![anchor],
            members: Vec::new(),
            programs: Vec::new(),
            effects: Vec::new(),
            checklist: Vec::new(),
            planned_costs: Vec::new(),
        });
    }
    // The latest home is where you live now, if its evidence reaches this or last month.
    if let Some(last) = chapters.last_mut() {
        if last
            .end
            .is_some_and(|end| end >= add_months(month_of(inputs.today), -1))
        {
            last.end = None;
        }
    }
    chapters
}

fn milestones(inputs: &Inputs, opened: &[(NaiveDate, &Account)]) -> Vec<Chapter> {
    let mut chapters = Vec::new();
    // Accounts opened in a run, each within a month of the one before.
    let mut sorted = opened.to_vec();
    sorted.sort_by_key(|(date, _)| *date);
    let mut index = 0;
    while index < sorted.len() {
        let start = sorted[index].0;
        let mut cluster = vec![&sorted[index]];
        for next in &sorted[index + 1..] {
            if next.0 - cluster.last().map_or(start, |(date, _)| *date) > Duration::days(31) {
                break;
            }
            cluster.push(next);
        }
        if cluster.len() >= 2 {
            chapters.push(Chapter {
                id: format!("milestone:accounts:{}", start.format("%Y-%m")),
                kind: ChapterKind::Milestone,
                title: format!("Opened {} accounts", cluster.len()),
                start,
                end: cluster.last().map(|(date, _)| *date),
                precision: Precision::Day,
                tier: Tier::Suggested,
                confirmed: false,
                state: None,
                anchors: cluster
                    .iter()
                    .map(|(date, account)| Anchor {
                        says: format!("{} first shows activity on {date}", account.name),
                        evidence: vec![Kind::Account.id(&account.id)],
                    })
                    .collect(),
                members: cluster
                    .iter()
                    .map(|(_, account)| Member {
                        id: Kind::Account.id(&account.id),
                        tier: Tier::Observed,
                        says: "Opened in this span".into(),
                    })
                    .collect(),
                programs: Vec::new(),
                effects: Vec::new(),
                checklist: Vec::new(),
                planned_costs: Vec::new(),
            });
        }
        index += cluster.len().max(1);
    }
    // A purchase far above everyday spending: at least $2,500 and ten times the median.
    let mut purchases = inputs
        .transactions
        .iter()
        .filter(|transaction| {
            !transaction.pending
                && transaction.amount < 0.0
                && transaction
                    .classification
                    .as_ref()
                    .is_some_and(|classification| {
                        classification.spending && classification.kind == Classified::Expense
                    })
        })
        .collect::<Vec<_>>();
    purchases.sort_by(|left, right| left.amount.total_cmp(&right.amount));
    let median = purchases
        .get(purchases.len() / 2)
        .map(|transaction| -transaction.amount);
    for transaction in purchases.iter().filter(|transaction| {
        -transaction.amount >= 2500.0
            && median.is_some_and(|median| -transaction.amount >= median * 10.0)
            && !transaction.category.to_lowercase().contains("rent")
    }) {
        let Some(date) = date_of(transaction) else {
            continue;
        };
        chapters.push(Chapter {
            id: format!("milestone:purchase:{}", transaction.id),
            kind: ChapterKind::Milestone,
            title: format!("A large purchase at {}", transaction.merchant),
            start: date,
            end: Some(date),
            precision: Precision::Day,
            tier: Tier::Suggested,
            confirmed: false,
            state: None,
            anchors: vec![Anchor {
                says: format!(
                    "${:.2} at {} on {date}",
                    -transaction.amount, transaction.merchant
                ),
                evidence: vec![Kind::Transaction.id(&transaction.id)],
            }],
            members: Vec::new(),
            programs: Vec::new(),
            effects: Vec::new(),
            checklist: Vec::new(),
            planned_costs: Vec::new(),
        });
    }
    chapters
}

/// Step changes in monthly income or spending: the split with the largest separation, at
/// least three complete months each side, a change of 30% and $750 a month, and a t-statistic
/// of 3. A shift a job's start explains belongs to that job instead.
fn shifts(months: &BTreeMap<NaiveDate, (f64, f64)>, work_starts: &[NaiveDate]) -> Vec<Chapter> {
    let mut chapters = Vec::new();
    let keys = months.keys().copied().collect::<Vec<_>>();
    for (measure, pick) in [("income", 0usize), ("spending", 1usize)] {
        let values = keys
            .iter()
            .map(|month| {
                let value = months[month];
                if pick == 0 {
                    value.0
                } else {
                    value.1
                }
            })
            .collect::<Vec<_>>();
        if values.len() < 6 {
            continue;
        }
        let mut best: Option<(usize, f64, f64, f64)> = None;
        for split in 3..=values.len() - 3 {
            let (before, after) = values.split_at(split);
            let mean = |side: &[f64]| side.iter().sum::<f64>() / side.len() as f64;
            let variance = |side: &[f64], mean: f64| {
                side.iter().map(|value| (value - mean).powi(2)).sum::<f64>()
                    / (side.len() as f64 - 1.0).max(1.0)
            };
            let (before_mean, after_mean) = (mean(before), mean(after));
            let error = (variance(before, before_mean) / before.len() as f64
                + variance(after, after_mean) / after.len() as f64)
                .sqrt();
            let t = if error > 0.0 {
                (after_mean - before_mean).abs() / error
            } else {
                f64::INFINITY
            };
            if best.is_none_or(|(_, _, _, best_t)| t > best_t) {
                best = Some((split, before_mean, after_mean, t));
            }
        }
        let Some((split, before, after, t)) = best else {
            continue;
        };
        let change = after - before;
        let start = keys[split];
        let explained = work_starts
            .iter()
            .any(|work| (month_of(*work) - start).num_days().abs() <= 31);
        if t >= 3.0 && change.abs() >= 750.0 && change.abs() >= before.abs() * 0.3 && !explained {
            let direction = if change > 0.0 { "rose" } else { "fell" };
            let round = |value: f64| (value * 100.0).round() / 100.0;
            chapters.push(Chapter {
                id: format!("shift:{measure}:{}", start.format("%Y-%m")),
                kind: ChapterKind::Shift,
                title: format!(
                    "{} {direction}",
                    if measure == "income" {
                        "Income"
                    } else {
                        "Spending"
                    }
                ),
                start,
                end: None,
                precision: Precision::Month,
                tier: Tier::Suggested,
                confirmed: false,
                state: None,
                anchors: vec![Anchor {
                    says: format!(
                        "Monthly {measure} averaged ${:.0} before {} and ${:.0} from then",
                        before,
                        start.format("%B %Y"),
                        after
                    ),
                    evidence: Vec::new(),
                }],
                members: Vec::new(),
                programs: Vec::new(),
                effects: vec![Effect {
                    measure: measure.into(),
                    before: round(before),
                    after: round(after),
                }],
                checklist: Vec::new(),
                planned_costs: Vec::new(),
            });
        }
    }
    chapters
}

fn planned(inputs: &Inputs) -> Vec<Chapter> {
    inputs
        .planned
        .iter()
        .map(|plan| Chapter {
            id: plan.id.clone(),
            kind: ChapterKind::Planned,
            title: plan.title.clone(),
            start: plan.start,
            end: plan.end,
            precision: Precision::Day,
            tier: Tier::Observed,
            confirmed: true,
            state: None,
            anchors: vec![Anchor {
                says: "Planned by you".into(),
                evidence: Vec::new(),
            }],
            members: Vec::new(),
            programs: Vec::new(),
            effects: Vec::new(),
            checklist: Vec::new(),
            planned_costs: plan.costs.clone(),
        })
        .collect()
}

pub fn chapters(inputs: &Inputs) -> Vec<Chapter> {
    let months = monthly(inputs.transactions, inputs.today);
    let opened = first_activity(inputs.transactions, inputs.accounts);
    let mut chapters = work(inputs, &months, &opened);
    chapters.extend(homes(inputs));
    chapters.extend(milestones(inputs, &opened));
    let work_starts = inputs
        .employers
        .iter()
        .map(|employer| employer.first)
        .collect::<Vec<_>>();
    chapters.extend(shifts(&covered(&months, inputs.transactions), &work_starts));
    for home in chapters
        .iter_mut()
        .filter(|chapter| chapter.kind == ChapterKind::Home)
    {
        home.effects = effects(&months, home.start);
    }
    // The user's choices: a dismissal drops a chapter for good, a confirmation and any title
    // or dates the user gave stand over what was detected.
    chapters.retain(|chapter| {
        !inputs
            .choices
            .iter()
            .any(|choice| choice.id == chapter.id && choice.status == ChapterStatus::Dismissed)
    });
    for chapter in &mut chapters {
        if let Some(choice) = inputs.choices.iter().find(|choice| choice.id == chapter.id) {
            chapter.confirmed = choice.status == ChapterStatus::Confirmed;
            if chapter.confirmed {
                chapter.tier = Tier::Observed;
            }
            if let Some(title) = &choice.title {
                chapter.title = title.clone();
            }
            if let Some(start) = choice.start {
                chapter.start = start;
            }
            if choice.end.is_some() {
                chapter.end = choice.end;
            }
        }
    }
    chapters.extend(planned(inputs));
    chapters.sort_by(|left, right| left.start.cmp(&right.start).then(left.id.cmp(&right.id)));
    chapters
}

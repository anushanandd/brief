//! Expected recurring activity: deterministic cadence detection over posted transactions.
//! Pending transactions are only unambiguous matches, never recurrence evidence; stale
//! snapshots and stale accounts produce no forecast rather than an invented one.
use std::{collections::HashMap, sync::LazyLock};

use chrono::{Datelike, Duration, NaiveDate};
use regex::Regex;
use serde::Serialize;

use super::benefits::PLATINUM;
use super::calendar::{self, Zone};
use super::AccountView;
use crate::finance_contract::Transaction;
use crate::numeric::js_round;
use crate::transaction_policy::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cadence {
    Weekly,
    Biweekly,
    SemiMonthly,
    Monthly,
    Quarterly,
    Annual,
}

const CADENCES: [Cadence; 6] = [
    Cadence::SemiMonthly,
    Cadence::Biweekly,
    Cadence::Weekly,
    Cadence::Monthly,
    Cadence::Quarterly,
    Cadence::Annual,
];

impl Cadence {
    fn label(self) -> &'static str {
        match self {
            Self::Weekly => "Weekly",
            Self::Biweekly => "Every two weeks",
            Self::SemiMonthly => "Twice monthly",
            Self::Monthly => "Monthly",
            Self::Quarterly => "Quarterly",
            Self::Annual => "Yearly",
        }
    }

    /// Occurrences in a year, for normalizing a series to a monthly cost.
    fn per_year(self) -> u32 {
        match self {
            Self::Weekly => 52,
            Self::Biweekly => 26,
            Self::SemiMonthly => 24,
            Self::Monthly => 12,
            Self::Quarterly => 4,
            Self::Annual => 1,
        }
    }

    fn months(self) -> i64 {
        match self {
            Self::Monthly => 1,
            Self::Quarterly => 3,
            _ => 12,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Credit,
    Income,
    Payment,
    Subscription,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EventState {
    Expected,
    Pending,
    AwaitingPost,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedEvent {
    pub id: String,
    pub recurring_id: String,
    pub title: String,
    pub kind: EventKind,
    pub date: NaiveDate,
    pub amount: f64,
    pub amount_low: f64,
    pub amount_high: f64,
    pub frequency: &'static str,
    pub observations: usize,
    pub last_seen: NaiveDate,
    pub state: EventState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_transaction_id: Option<String>,
    /// The latest posted transaction in the series, for its mark and account.
    pub source_transaction_id: String,
    /// Occurrences after `date` within the forecast horizon; not part of the contract.
    #[serde(skip)]
    pub later: Vec<NaiveDate>,
    /// Occurrences per year at the detected cadence; not part of the contract.
    #[serde(skip)]
    pub per_year: u32,
}

fn re(source: &str) -> Regex {
    Regex::new(source).expect("valid pattern")
}

static REFERENCE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(?-u:\b)(?:reference|ref|confirmation|transaction)\s*(?:number|no)?\s*:?\s*[a-z0-9-]{5,}(?-u:\b).*$",
    )
});
static MASKED_NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)(?-u:\b)[x*]{3,}\d{2,}(?-u:\b)"));
static LONG_NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"(?-u:\b)\d{5,}(?-u:\b)"));
static NON_ALPHANUMERIC: LazyLock<Regex> = LazyLock::new(|| re(r"[^a-z0-9]+"));
static PAYROLL: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)payroll|paycheck|salary|direct deposit"));
static BANK_DEBIT: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(?-u:\b)ach(?-u:\b)|direct debit|rent|mortgage|loan|utility|utilities|electric|water|insurance",
    )
});
static BANKING_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)(?-u:\b)ach(?-u:\b)|direct deposit|automatic payment|autopay|bill payment|banking|income|loan payments|loan_payments|rent and utilities|rent_and_utilities",
    )
});
static DEBT_PAYMENT: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)loan payments?|credit card payment|card payment|autopay payment"));
static SUBSCRIPTION: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)subscription|streaming|membership|recurring"));
/// Never a subscription however fixed the amount: people, transfers, and household bills.
static NOT_SUBSCRIPTION: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)transfer|payments to people|zelle|venmo|(?-u:\b)ach(?-u:\b)|direct debit|(?-u:\b)rent(?-u:\b)|rent and utilities|rent_and_utilities|mortgage|lease|housing|loan|(?-u:\b)tax(?-u:\b)|(?-u:\b)fees?(?-u:\b)",
    )
});
static VARIABLE_BILL: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)utility|utilities|electric|water|internet|phone|insurance"));
static HABITUAL: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)grocer|supermarket|food and drink|food_and_drink|restaurant|dining|coffee|gas station|fuel|rideshare|taxi|transit|general merchandise|general_merchandise|shopping|department store|retail",
    )
});

/// The hostname of a website value, without `www.`.
pub fn website_domain(value: &str) -> Option<String> {
    let rest = value.split_once("://").map_or(value, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?.to_ascii_lowercase();
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    {
        return None;
    }
    let host = host.strip_prefix("www.").unwrap_or(&host);
    (!host.is_empty()).then(|| host.to_owned())
}

fn recurring_merchant(transaction: &Transaction) -> String {
    if let Some(domain) = transaction.website.as_deref().and_then(website_domain) {
        return format!("domain:{domain}");
    }
    let lower = transaction.merchant.to_lowercase();
    let text = REFERENCE_SUFFIX.replace(&lower, " ");
    let text = MASKED_NUMBER.replace_all(&text, " ");
    let text = LONG_NUMBER.replace_all(&text, " ");
    NON_ALPHANUMERIC.replace_all(&text, " ").trim().to_owned()
}

/// A stable series identity: account, direction, and merchant (or recognized benefit).
pub fn recurring_key(transaction: &Transaction, zone: &dyn Zone) -> String {
    let benefit = (transaction.amount > 0.0)
        .then(|| PLATINUM.credit_identity(transaction, zone))
        .flatten();
    let string = |value: &str| serde_json::to_string(value).expect("strings serialize");
    format!(
        "{{\"version\":3,\"account\":{},\"direction\":{},\"merchant\":{}}}",
        string(
            transaction
                .account_id
                .as_deref()
                .unwrap_or(&transaction.account)
        ),
        string(if transaction.amount < 0.0 {
            "out"
        } else {
            "in"
        }),
        string(&match benefit {
            Some((id, _)) => format!("platinum-benefit:{id}"),
            None => recurring_merchant(transaction),
        }),
    )
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2]
}

fn median_day(values: &[u32]) -> u32 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn money(value: f64) -> f64 {
    js_round(value * 100.0) / 100.0
}

fn matches_cadence(left: NaiveDate, right: NaiveDate, cadence: Cadence) -> bool {
    let days = calendar::days_between(left, right);
    match cadence {
        Cadence::Weekly => (days - 7).abs() <= 2,
        Cadence::Biweekly => (days - 14).abs() <= 3,
        Cadence::SemiMonthly => (12..=19).contains(&days),
        _ => {
            calendar::month_index(right) - calendar::month_index(left) == cadence.months()
                && ((right.day() as i64 - left.day() as i64).abs() <= 4
                    || (calendar::is_month_end(left) && calendar::is_month_end(right)))
        }
    }
}

struct Anchors {
    first: u32,
    second: u32,
    end_of_month: bool,
}

fn semi_monthly_anchors(dates: &[NaiveDate]) -> Option<Anchors> {
    if dates.len() < 4 {
        return None;
    }
    let days = dates.iter().map(|date| date.day()).collect::<Vec<_>>();
    let groups = [
        days.iter().step_by(2).copied().collect::<Vec<_>>(),
        days.iter().skip(1).step_by(2).copied().collect::<Vec<_>>(),
    ];
    let anchors = [median_day(&groups[0]), median_day(&groups[1])];
    let first = anchors[0].min(anchors[1]);
    let second = anchors[0].max(anchors[1]);
    if second - first < 12
        || groups.iter().zip(anchors).any(|(group, anchor)| {
            group
                .iter()
                .any(|day| (*day as i64 - anchor as i64).abs() > 3)
        })
    {
        return None;
    }
    Some(Anchors {
        first,
        second,
        end_of_month: dates
            .iter()
            .filter(|date| (date.day() as i64 - second as i64).abs() <= 3)
            .all(|date| calendar::is_month_end(*date)),
    })
}

fn next_month_date(date: NaiveDate, months: i64, day: u32, end_of_month: bool) -> NaiveDate {
    let first = calendar::add_months_first(date, months);
    let last = calendar::month_end(first.year(), first.month()).day();
    first + Duration::days(if end_of_month { last } else { day.min(last) } as i64 - 1)
}

fn evidence(transaction: &Transaction, with_code: bool) -> String {
    format!(
        "{} {} {} {}{}",
        transaction.merchant,
        transaction.category,
        transaction.category_detail.as_deref().unwrap_or(""),
        transaction.description.as_deref().unwrap_or(""),
        if with_code {
            format!(" {}", transaction.transaction_code.as_deref().unwrap_or(""))
        } else {
            String::new()
        }
    )
}

/// A merchant charge of the same amount every month, quarter, or year reads as a
/// subscription even when the provider category doesn't say so.
/// Charges at one price throughout, or at one price and then, after a single change, at
/// another: a fixed charge that was repriced is still a fixed charge.
fn repriced_at(transactions: &[&Transaction]) -> Option<Option<usize>> {
    let amounts = transactions
        .iter()
        .map(|transaction| money(transaction.amount))
        .collect::<Vec<_>>();
    let changes = (1..amounts.len())
        .filter(|index| amounts[*index] != amounts[index - 1])
        .collect::<Vec<_>>();
    match changes.as_slice() {
        [] => Some(None),
        [index] => Some(Some(*index)),
        _ => None,
    }
}

fn fixed_charge(transactions: &[&Transaction], cadence: Cadence, traits: &Traits) -> bool {
    let last = transactions[transactions.len() - 1];
    matches!(
        cadence,
        Cadence::Monthly | Cadence::Quarterly | Cadence::Annual
    ) && last.amount < 0.0
        && last.kind() == Some(Kind::Expense)
        && !traits.variable_bill
        && !traits.habitual
        && !NOT_SUBSCRIPTION.is_match(&evidence(last, false))
        && repriced_at(transactions).is_some()
}

fn next_occurrence(dates: &[NaiveDate], cadence: Cadence, transaction: &Transaction) -> NaiveDate {
    let last = *dates.last().expect("a cadence has dates");
    let next = match cadence {
        Cadence::Weekly => last + Duration::days(7),
        Cadence::Biweekly => last + Duration::days(14),
        Cadence::SemiMonthly => {
            let anchors = semi_monthly_anchors(dates).expect("semi-monthly runs have anchors");
            if last.day() <= anchors.first + 3 {
                next_month_date(last, 0, anchors.second, anchors.end_of_month)
            } else {
                next_month_date(last, 1, anchors.first, false)
            }
        }
        _ => next_month_date(
            last,
            cadence.months(),
            median_day(&dates.iter().map(|date| date.day()).collect::<Vec<_>>()),
            dates.iter().all(|date| calendar::is_month_end(*date)),
        ),
    };
    let evidence = evidence(transaction, true);
    if PAYROLL.is_match(&evidence) {
        calendar::shift_to_business_day(next, -1)
    } else if BANK_DEBIT.is_match(&evidence) {
        calendar::shift_to_business_day(next, 1)
    } else {
        next
    }
}

struct Traits {
    benefit_credit: Option<(String, String)>,
    credit: bool,
    debt_payment: bool,
    income: bool,
    subscription: bool,
    variable_bill: bool,
    habitual: bool,
}

fn traits(transaction: &Transaction, zone: &dyn Zone) -> Traits {
    let evidence = evidence(transaction, false);
    let kind = transaction.kind();
    let benefit_credit = (transaction.amount > 0.0)
        .then(|| PLATINUM.credit_identity(transaction, zone))
        .flatten();
    let debt_payment = transaction.amount < 0.0
        && kind == Some(Kind::Transfer)
        && DEBT_PAYMENT.is_match(&evidence);
    Traits {
        credit: benefit_credit.is_some()
            || transaction
                .classification
                .as_ref()
                .is_some_and(|classification| classification.credit),
        benefit_credit,
        debt_payment,
        income: matches!(kind, Some(Kind::Income | Kind::Dividend | Kind::Interest)),
        subscription: SUBSCRIPTION.is_match(&evidence),
        variable_bill: debt_payment || VARIABLE_BILL.is_match(&evidence),
        habitual: HABITUAL.is_match(&format!(
            "{} {}",
            transaction.category,
            transaction.category_detail.as_deref().unwrap_or("")
        )),
    }
}

fn title(transaction: &Transaction, benefit_name: Option<&str>) -> String {
    if let Some(name) = benefit_name {
        return name.to_owned();
    }
    let description = transaction
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let banking = BANKING_TITLE.is_match(&format!(
        "{} {} {} {}",
        transaction.merchant,
        transaction.category,
        transaction.category_detail.as_deref().unwrap_or(""),
        transaction.transaction_code.as_deref().unwrap_or("")
    ));
    match (banking, description) {
        (true, Some(description)) => description.to_owned(),
        _ => transaction.merchant.clone(),
    }
}

fn amount_clusters<'a>(values: &[&'a Transaction]) -> Vec<Vec<&'a Transaction>> {
    let mut clusters: Vec<Vec<&Transaction>> = Vec::new();
    for transaction in values {
        let amount = transaction.amount.abs();
        let best = clusters
            .iter()
            .enumerate()
            .filter_map(|(index, entries)| {
                let typical = median(
                    &entries
                        .iter()
                        .map(|entry| entry.amount.abs())
                        .collect::<Vec<_>>(),
                );
                ((amount - typical).abs() <= 2f64.max(typical * 0.08))
                    .then_some((index, (amount - typical).abs()))
            })
            // The first of equally close clusters wins, like a stable sort.
            .fold(None, |best: Option<(usize, f64)>, candidate| match best {
                Some((_, distance)) if distance <= candidate.1 => best,
                _ => Some(candidate),
            });
        match best {
            Some((index, _)) => clusters[index].push(transaction),
            None => clusters.push(vec![transaction]),
        }
    }
    clusters
}

struct Run<'a> {
    cadence: Cadence,
    transactions: Vec<&'a Transaction>,
    dates: Vec<NaiveDate>,
}

fn cadence_for<'a>(
    values: &[&'a Transaction],
    updated_at: &str,
    strong_annual_evidence: bool,
    zone: &dyn Zone,
) -> Option<Run<'a>> {
    // Cadence follows the charge date: a bank posts a month-end charge on the 30th one
    // month and the 1st the next, which would break the run or skip a cycle.
    let mut ordered = values
        .iter()
        .map(|transaction| {
            (
                calendar::transaction_date(&transaction.date, updated_at, zone),
                *transaction,
            )
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(date, _)| *date);
    let dates = ordered
        .iter()
        .map(|(date, _)| *date)
        .collect::<Option<Vec<_>>>()?;
    let mut selected: Option<(Cadence, usize)> = None;
    for cadence in CADENCES {
        let mut start = ordered.len() - 1;
        while start > 0 && matches_cadence(dates[start - 1], dates[start], cadence) {
            start -= 1;
        }
        let run = &dates[start..];
        let minimum = match cadence {
            Cadence::SemiMonthly => 4,
            Cadence::Annual => 2,
            _ => 3,
        };
        if run.len() < minimum || (cadence == Cadence::Annual && !strong_annual_evidence) {
            continue;
        }
        if cadence == Cadence::SemiMonthly && semi_monthly_anchors(run).is_none() {
            continue;
        }
        if selected.is_none_or(|(_, selected_start)| run.len() > ordered.len() - selected_start) {
            selected = Some((cadence, start));
        }
    }
    let (cadence, start) = selected?;
    Some(Run {
        cadence,
        transactions: ordered[start..]
            .iter()
            .map(|(_, transaction)| *transaction)
            .collect(),
        dates: dates[start..].to_vec(),
    })
}

fn date_of(transaction: &Transaction, updated_at: &str, zone: &dyn Zone) -> Option<NaiveDate> {
    calendar::transaction_date(transaction.posted_date(), updated_at, zone)
}

/// Forecasts the next occurrence of each recurring series within 30 days of `today`.
pub fn expected_events(
    transactions: &[Transaction],
    accounts: &[AccountView],
    updated_at: &str,
    today: NaiveDate,
    zone: &dyn Zone,
) -> Vec<ExpectedEvent> {
    forecast_within(transactions, accounts, updated_at, today, zone, 30)
}

/// Forecasts each recurring series whose next occurrence falls within `horizon_days`, with
/// the occurrences after it up to the same horizon in `later`.
pub fn forecast_within(
    transactions: &[Transaction],
    accounts: &[AccountView],
    updated_at: &str,
    today: NaiveDate,
    zone: &dyn Zone,
    horizon_days: i64,
) -> Vec<ExpectedEvent> {
    // An old saved snapshot cannot establish that a missing payment has stopped.
    if updated_at
        .get(..10)
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .is_some_and(|saved| calendar::days_between(saved, today) > 3)
    {
        return Vec::new();
    }
    let horizon = Horizon {
        updated_at,
        today,
        zone,
        horizon_days,
    };
    let mut events = Vec::new();
    for (key, values) in recurring_groups(transactions, zone) {
        let mut ordered = values
            .iter()
            .filter(|transaction| !transaction.pending)
            .map(|transaction| (date_of(transaction, updated_at, zone), *transaction))
            .collect::<Vec<_>>();
        ordered.sort_by_key(|(date, _)| *date);
        let ordered = ordered[ordered.len().saturating_sub(12)..]
            .iter()
            .map(|(_, transaction)| *transaction)
            .collect::<Vec<_>>();
        let Some(latest) = ordered.last() else {
            continue;
        };
        if !source_current(latest, accounts, today) {
            continue;
        }
        let mut candidates = amount_series(&ordered, &horizon)
            .iter()
            .filter_map(|(series, amount_basis)| candidate(&key, series, amount_basis, &horizon))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            right
                .1
                .last_seen
                .cmp(&left.1.last_seen)
                .then_with(|| right.1.observations.cmp(&left.1.observations))
        });
        // Each transaction belongs to one forecast; the most recent, best-observed series wins.
        let mut used = std::collections::HashSet::new();
        let mut accepted = Vec::new();
        for (ids, event) in candidates {
            if ids.iter().any(|id| used.contains(id)) {
                continue;
            }
            used.extend(ids);
            accepted.push(event);
        }
        let single = accepted.len() == 1;
        let group_events = accepted
            .into_iter()
            .enumerate()
            .map(|(index, mut event)| {
                if !single {
                    event.id = format!(
                        "{key}:amount:{}:{index}",
                        js_round(event.amount.abs() * 100.0) as i64
                    );
                }
                event
            })
            .collect::<Vec<_>>();
        let pending = values
            .iter()
            .filter(|transaction| transaction.pending)
            .copied()
            .collect::<Vec<_>>();
        events.extend(match_pending(group_events, &pending, &horizon));
    }
    events.sort_by_key(|event| event.date);
    events
}

/// What a forecast is made against: the snapshot's save time, today, and how far ahead to look.
struct Horizon<'a> {
    updated_at: &'a str,
    today: NaiveDate,
    zone: &'a dyn Zone,
    horizon_days: i64,
}

/// Transactions grouped by recurring series, in the order each series first appears. Transfers
/// are left out unless they pay down debt.
fn recurring_groups<'a>(
    transactions: &'a [Transaction],
    zone: &dyn Zone,
) -> Vec<(String, Vec<&'a Transaction>)> {
    let mut order = Vec::<String>::new();
    let mut groups = HashMap::<String, Vec<&Transaction>>::new();
    for transaction in transactions {
        let traits = traits(transaction, zone);
        if (transaction.kind() == Some(Kind::Transfer) && !traits.debt_payment)
            || transaction.merchant.trim().is_empty()
        {
            continue;
        }
        let key = recurring_key(transaction, zone);
        groups
            .entry(key.clone())
            .or_insert_with(|| {
                order.push(key);
                Vec::new()
            })
            .push(transaction);
    }
    order
        .into_iter()
        .map(|key| {
            let values = groups.remove(&key).unwrap_or_default();
            (key, values)
        })
        .collect()
}

/// Whether the account behind a series was read recently enough that a missing payment means
/// something. A Plaid account never read has no say.
fn source_current(latest: &Transaction, accounts: &[AccountView], today: NaiveDate) -> bool {
    let account = latest
        .account_id
        .as_deref()
        .and_then(|id| accounts.iter().find(|account| account.id == id));
    let source_updated_at = account.and_then(|account| {
        account
            .activity_as_of
            .as_deref()
            .or(account.balance_fetched_at.as_deref())
    });
    if latest
        .account_id
        .as_deref()
        .is_some_and(|id| id.starts_with("plaid:"))
        && source_updated_at.is_none()
    {
        return false;
    }
    !source_updated_at.is_some_and(|source| {
        source
            .get(..10)
            .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
            .is_some_and(|date| calendar::days_between(date, today) > 3)
    })
}

/// The series a group may hold: one per amount cluster, or the whole group when its amounts
/// vary by nature, and each pair of clusters where one followed the other, as when a price
/// changed. Each comes with the transactions its amount is judged from.
fn amount_series<'a>(
    ordered: &[&'a Transaction],
    horizon: &Horizon,
) -> Vec<(Vec<&'a Transaction>, Vec<&'a Transaction>)> {
    let (updated_at, zone) = (horizon.updated_at, horizon.zone);
    let flexible_amounts = ordered.iter().all(|transaction| {
        let traits = traits(transaction, zone);
        traits.income || traits.credit
    }) || ordered
        .iter()
        .any(|transaction| traits(transaction, zone).variable_bill);
    let clusters = if flexible_amounts {
        vec![ordered.to_vec()]
    } else {
        amount_clusters(ordered)
    };
    let mut series = clusters
        .iter()
        .map(|cluster| (cluster.clone(), cluster.clone()))
        .collect::<Vec<_>>();
    for (older_index, older) in clusters.iter().enumerate() {
        for (newer_index, newer) in clusters.iter().enumerate() {
            if older_index == newer_index
                || older.len() < 2
                || newer.len() < 2
                || date_of(older[older.len() - 1], updated_at, zone)
                    >= date_of(newer[0], updated_at, zone)
            {
                continue;
            }
            series.push(([older.clone(), newer.clone()].concat(), newer.clone()));
        }
    }
    series
}

/// The forecast one series makes, with the transactions it explains, when it repeats on a
/// cadence and its next charge falls within the horizon.
fn candidate(
    key: &str,
    transactions: &[&Transaction],
    amount_basis: &[&Transaction],
    horizon: &Horizon,
) -> Option<(Vec<String>, ExpectedEvent)> {
    let (updated_at, today, zone, horizon_days) = (
        horizon.updated_at,
        horizon.today,
        horizon.zone,
        horizon.horizon_days,
    );
    let traits = traits(transactions[transactions.len() - 1], zone);
    if !traits.income && !traits.credit && traits.habitual && !traits.subscription {
        return None;
    }
    let run = cadence_for(
        transactions,
        updated_at,
        traits.income || traits.credit || traits.subscription || traits.variable_bill,
        zone,
    )?;
    let last = run.transactions[run.transactions.len() - 1];
    let expected = next_occurrence(&run.dates, run.cadence, last);
    // A late posting can be pending, but a missed occurrence never advances a cycle.
    if calendar::days_between(today, expected) > horizon_days
        || (expected < today && calendar::business_days_after(expected, today) > 5)
    {
        return None;
    }
    let mut later = Vec::new();
    let mut dates = run.dates.clone();
    dates.push(expected);
    // Bounded: a cadence advances at least a week per step.
    while later.len() < 60 {
        if run.cadence == Cadence::SemiMonthly && semi_monthly_anchors(&dates).is_none() {
            break;
        }
        let next = next_occurrence(&dates, run.cadence, last);
        if next <= dates[dates.len() - 1] || calendar::days_between(today, next) > horizon_days {
            break;
        }
        later.push(next);
        dates.push(next);
    }
    let amounts = run
        .transactions
        .iter()
        .map(|transaction| transaction.amount.abs())
        .collect::<Vec<_>>();
    let fixed = fixed_charge(&run.transactions, run.cadence, &traits);
    // A repriced fixed charge will next charge its new price.
    let typical = match repriced_at(&run.transactions).filter(|_| fixed) {
        Some(Some(_)) => last.amount.abs(),
        _ => median(
            &amount_basis
                .iter()
                .map(|transaction| transaction.amount.abs())
                .collect::<Vec<_>>(),
        ),
    };
    let ids = run
        .transactions
        .iter()
        .map(|transaction| transaction.id.clone())
        .collect::<Vec<_>>();
    Some((
        ids,
        ExpectedEvent {
            id: key.to_owned(),
            recurring_id: key.to_owned(),
            title: title(
                last,
                traits
                    .benefit_credit
                    .as_ref()
                    .map(|(_, name)| name.as_str()),
            ),
            kind: if traits.benefit_credit.is_some() {
                EventKind::Credit
            } else if traits.income {
                EventKind::Income
            } else if traits.subscription || fixed {
                EventKind::Subscription
            } else {
                EventKind::Payment
            },
            date: expected,
            amount: money(if last.amount > 0.0 { typical } else { -typical }),
            amount_low: money(amounts.iter().copied().fold(f64::INFINITY, f64::min)),
            amount_high: money(amounts.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
            frequency: run.cadence.label(),
            observations: run.transactions.len(),
            last_seen: run.dates[run.dates.len() - 1],
            state: if expected < today {
                EventState::AwaitingPost
            } else {
                EventState::Expected
            },
            account_id: last.account_id.clone(),
            matched_transaction_id: None,
            source_transaction_id: last.id.clone(),
            later,
            per_year: run.cadence.per_year(),
        },
    ))
}

/// Forecasts with the pending transaction each awaits, when exactly one matches it and no other
/// forecast in the group claims it.
fn match_pending(
    events: Vec<ExpectedEvent>,
    pending: &[&Transaction],
    horizon: &Horizon,
) -> Vec<ExpectedEvent> {
    let (updated_at, zone) = (horizon.updated_at, horizon.zone);
    let matches = events
        .iter()
        .map(|event| {
            pending
                .iter()
                .filter(|transaction| {
                    let Some(date) = date_of(transaction, updated_at, zone) else {
                        return false;
                    };
                    let amount = transaction.amount.abs();
                    let tolerance = 5f64.max(event.amount.abs() * 0.25);
                    calendar::days_between(event.date, date).abs() <= 5
                        && amount >= event.amount_low - tolerance
                        && amount <= event.amount_high + tolerance
                })
                .copied()
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    events
        .into_iter()
        .enumerate()
        .map(|(index, mut event)| {
            let matched = (matches[index].len() == 1).then(|| matches[index][0]);
            let unambiguous = matched.filter(|matched| {
                matches.iter().enumerate().all(|(other, candidates)| {
                    other == index
                        || !candidates
                            .iter()
                            .any(|candidate| candidate.id == matched.id)
                })
            });
            if let Some(matched) = unambiguous {
                event.amount = money(matched.amount);
                event.state = EventState::Pending;
                event.matched_transaction_id = Some(matched.id.clone());
            }
            event
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn website_domains_ignore_scheme_port_credentials_and_www() {
        assert_eq!(
            website_domain("https://www.Example.com:8443/path").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            website_domain("user@shop.example").as_deref(),
            Some("shop.example")
        );
        assert_eq!(website_domain("not a host"), None);
    }
}

//! Retirement: what this year's contributions count against, how the investments are taxed,
//! and how many years of spending they already cover. Every figure is measured. The one look
//! ahead is a workplace plan's paychecks left this year, at the cadence its deferrals show.
//!
//! Limits come from `retirement-limits.json`, copied from the IRS notice for each year; a year
//! without one has no limits. A workplace plan's export names each contribution's source, and
//! only the employee's own deferrals, pre-tax and Roth together, count against the deferral
//! limit, across every plan. IRA contributions count against one limit across every IRA, by
//! the tax year they're made for, which can be the year before until its filing deadline. An
//! inflow that may be a rollover, or that falls in that overlap, waits for the user. Amounts
//! sum in whole cents.
use std::collections::BTreeMap;
use std::sync::LazyLock;

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use super::attention::{AttentionItem, Item, Severity};
use super::calendar;
use super::changes::Window;
use super::recap::{self, PeriodRecap};
use crate::documents::Kind as DocumentKind;
use crate::finance_contract::{Account, ContributionYear, Transaction};
use crate::numeric::{from_minor, minor_units, round2};
use crate::settings::FinanceSettings;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct YearLimits {
    elective_deferral: f64,
    catch_up50: f64,
    catch_up60_to63: f64,
    ira: f64,
    ira_catch_up50: f64,
}

#[derive(Deserialize)]
struct Tables {
    years: BTreeMap<i32, YearLimits>,
}

static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    serde_json::from_str(include_str!("retirement-limits.json")).expect("limits are valid")
});

/// Which limit an account's contributions count against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Plan {
    /// A 401(k), 403(b), or the Thrift Savings Plan: the elective deferral limit.
    Workplace,
    /// A traditional or Roth IRA: the IRA limit.
    Ira,
}

/// Read from the account's names, including the one the user gave it. SEP and SIMPLE IRAs and
/// 457(b) plans have limits of their own, so they count against neither.
fn plan(account: &Account, name: Option<&str>) -> Option<Plan> {
    if account.r#type != "retirement" {
        return None;
    }
    let descriptor = [
        name,
        Some(account.name.as_str()),
        account.official_name.as_deref(),
        account.subtype.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
    .to_ascii_lowercase();
    let words = descriptor
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let word = |candidates: &[&str]| words.iter().any(|word| candidates.contains(word));
    if word(&["sep", "simple", "sepira", "simpleira", "457", "457b", "hsa"]) {
        None
    } else if word(&["tsp", "thrift"])
        || words
            .iter()
            .any(|word| word.starts_with("401") || word.starts_with("403"))
    {
        Some(Plan::Workplace)
    } else if word(&["ira", "rothira", "tradira", "traditionalira", "rolloverira"]) {
        Some(Plan::Ira)
    } else {
        None
    }
}

/// The last day to contribute to an IRA for `year`: the filing deadline, April 15 of the next
/// year, moved past a weekend or Washington's Emancipation Day.
pub fn ira_deadline(year: i32) -> NaiveDate {
    let day = |month, day| NaiveDate::from_ymd_opt(year + 1, month, day).expect("valid date");
    let holiday = match day(4, 16).weekday() {
        Weekday::Sat => day(4, 15),
        Weekday::Sun => day(4, 17),
        _ => day(4, 16),
    };
    let mut deadline = day(4, 15);
    while matches!(deadline.weekday(), Weekday::Sat | Weekday::Sun) || deadline == holiday {
        deadline += Duration::days(1);
    }
    deadline
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Retirement {
    pub year: i32,
    /// This year's deferrals across workplace plans; absent when no account is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workplace: Option<Workplace>,
    /// IRA contributions for this tax year, and for last year until its deadline passes.
    pub ira: Vec<IraYear>,
    /// IRA deposits since the first year in `ira` began, and any older one still waiting, so
    /// each placement can be checked and changed.
    pub deposits: Vec<Deposit>,
    pub buckets: Buckets,
    /// Each Roth IRA's contributions, which can come out at any age free of tax and penalty.
    pub roth_basis: Vec<RothBasis>,
    /// This year so far, and last year once history covers all of it.
    pub savings: Vec<SavingsYear>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
    /// Retirement accounts whose names say no plan Brief knows a limit for.
    pub uncounted: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workplace {
    pub account_ids: Vec<String>,
    /// Absent when this year's limits aren't in the table yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    /// The employee's own deferrals, pre-tax and Roth.
    pub deferred: f64,
    /// `deferred` is the pay statements' year-to-date figure, ahead of the plan's export.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub from_pay_statements: bool,
    /// Contributions the export doesn't say are the employee's or the employer's.
    pub unlabeled: f64,
    /// Matching and other employer money, which doesn't count against the limit.
    pub employer: f64,
    /// The last day an export shows a contribution this year.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub through: Option<NaiveDate>,
    /// Paychecks left this year at the cadence deferrals arrive; absent until three show it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paychecks_left: Option<u32>,
    /// Deferrals by year end if the latest one repeats each paycheck left.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projected: Option<f64>,
    /// What each paycheck left would need to defer to reach the limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_paycheck_to_max: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IraYear {
    pub tax_year: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    pub contributed: f64,
    pub deadline: NaiveDate,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Deposit {
    pub transaction_id: String,
    pub account_id: String,
    pub date: NaiveDate,
    pub amount: f64,
    pub flow: String,
    /// The tax years a contribution on this day could count toward.
    pub years: Vec<i32>,
    /// Absent when it isn't a contribution, or waits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts_for: Option<i32>,
    /// The user placed it, rather than Brief.
    pub chosen: bool,
    /// The brokerage's statement names the year, and Brief followed it.
    pub stated: bool,
    /// Brief can't place it alone, and the user hasn't: a deposit or transfer that may be a
    /// rollover, or a contribution made before the deadline for the year before.
    pub waiting: bool,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Buckets {
    /// Traditional 401(k)s and IRAs: taxed as income when withdrawn.
    pub pre_tax: f64,
    /// Roth accounts and HSAs.
    pub tax_free: f64,
    pub taxable: f64,
    /// False when an investment account has no value.
    pub complete: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RothBasis {
    pub account_id: String,
    /// The first day of the account's activity in Brief.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<NaiveDate>,
    /// What the user entered for contributions before `since`; absent until they do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earlier: Option<f64>,
    pub contributed: f64,
    pub withdrawn: f64,
    /// Contributions less withdrawals, which come out of contributions first.
    pub basis: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SavingsYear {
    pub year: i32,
    pub from: NaiveDate,
    pub through: NaiveDate,
    /// History starts after January 1, so `from` is later.
    pub partial: bool,
    /// Pay that reached the accounts, after taxes and paycheck deductions.
    pub take_home: f64,
    pub spending: f64,
    /// Workplace deferrals taken out before the pay arrived.
    pub deferred: f64,
    /// Employee stock purchases taken out before the pay arrived.
    pub espp: f64,
    /// Saved over take-home pay plus deferrals, in percent; absent without income.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    /// Brokerage and retirement accounts.
    pub invested: f64,
    pub complete: bool,
    /// The last 365 days.
    pub spending: f64,
    /// The planned monthly additions, for a year.
    pub planned: f64,
    /// `invested` over a year of `spending` and `planned`.
    pub years: f64,
}

/// An IRA inflow placed: the user's choice, else the year a contribution was made when it can
/// only be that one. Cash the provider doesn't call a contribution, deposit, or transfer is
/// none of these.
fn deposit(
    transaction: &Transaction,
    account_id: &str,
    date: NaiveDate,
    choices: &BTreeMap<String, Option<i32>>,
    stated: &[ContributionYear],
) -> Option<Deposit> {
    let flow = transaction
        .flow
        .as_deref()
        .filter(|flow| matches!(*flow, "contribution" | "deposit" | "transfer"))?;
    let years = if date <= ira_deadline(date.year() - 1) {
        vec![date.year() - 1, date.year()]
    } else {
        vec![date.year()]
    };
    let choice = choices.get(&transaction.id).copied();
    // The brokerage's own note for the same amount within a week names the year.
    let noted = stated
        .iter()
        .find(|memo| {
            memo.account_id == account_id
                && minor_units(memo.amount) == minor_units(transaction.amount)
                && NaiveDate::parse_from_str(&memo.date, "%Y-%m-%d")
                    .is_ok_and(|noted| (noted - date).num_days().abs() <= 7)
                && years.contains(&memo.tax_year)
        })
        .map(|memo| memo.tax_year);
    let automatic = noted.or((flow == "contribution" && years.len() == 1).then_some(date.year()));
    Some(Deposit {
        stated: choice.is_none() && noted.is_some(),
        transaction_id: transaction.id.clone(),
        account_id: account_id.to_owned(),
        date,
        amount: transaction.amount,
        flow: flow.to_owned(),
        years,
        counts_for: choice.unwrap_or(automatic),
        chosen: choice.is_some(),
        waiting: choice.is_none() && automatic.is_none(),
    })
}

/// The cadence paychecks arrive at, in days, from at least three deferral dates: the median
/// gap, when it reads as weekly through monthly.
fn cadence(dates: &[NaiveDate]) -> Option<i64> {
    if dates.len() < 3 {
        return None;
    }
    let mut gaps = dates
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).num_days())
        .collect::<Vec<_>>();
    gaps.sort_unstable();
    Some(gaps[gaps.len() / 2]).filter(|gap| (7..=31).contains(gap))
}

pub struct Inputs<'a> {
    pub transactions: &'a [Transaction],
    pub accounts: &'a [Account],
    pub settings: &'a FinanceSettings,
    pub recap: &'a [PeriodRecap],
    /// Brokerages' notes of the tax year each IRA contribution counts toward.
    pub stated: &'a [ContributionYear],
    /// Pay statements give deferrals and stock purchases the plan's export may not have yet.
    pub documents: &'a [crate::documents::Document],
}

pub fn retirement(inputs: Inputs<'_>, today: NaiveDate) -> Retirement {
    let Inputs {
        transactions,
        accounts,
        settings,
        recap,
        stated,
        documents,
    } = inputs;
    let profile = settings.retirement.clone().unwrap_or_default();
    let year = today.year();
    let age = |year: i32| profile.birth_year.map(|born| year - born);
    let limits = |year: i32| TABLES.years.get(&year).copied();
    let plans = accounts
        .iter()
        .filter_map(|account| {
            let name = settings.account_names.get(&account.id).map(String::as_str);
            plan(account, name).map(|plan| (account.id.as_str(), plan))
        })
        .collect::<BTreeMap<_, _>>();
    let posted = calendar::posted_through(transactions, today);
    // Last year stays open for IRA contributions until its deadline.
    let first_ira_year = if today <= ira_deadline(year - 1) {
        year - 1
    } else {
        year
    };

    // Workplace deferrals, summed by day: one paycheck may buy several funds.
    let mut deferrals = BTreeMap::<NaiveDate, i64>::new();
    let (mut unlabeled, mut employer) = (0_i64, 0_i64);
    let mut deferred_by_year = BTreeMap::<i32, i64>::new();
    let mut latest = None::<NaiveDate>;
    // IRA inflows and Roth withdrawals.
    let mut ira = BTreeMap::<i32, i64>::new();
    let mut deposits = Vec::new();
    let mut contributed = BTreeMap::<&str, i64>::new();
    let mut withdrawn = BTreeMap::<&str, i64>::new();
    let mut since = BTreeMap::<&str, NaiveDate>::new();
    for (date, transaction) in &posted {
        let Some((account_id, plan)) = transaction
            .account_id
            .as_deref()
            .and_then(|id| plans.get_key_value(id))
        else {
            continue;
        };
        since.entry(account_id).or_insert(*date);
        let cents = minor_units(transaction.amount);
        match plan {
            Plan::Workplace if cents > 0 => {
                let flow = transaction.flow.as_deref();
                if flow == Some("deferral") {
                    *deferred_by_year.entry(date.year()).or_default() += cents;
                }
                if date.year() != year {
                    continue;
                }
                match flow {
                    Some("deferral") => *deferrals.entry(*date).or_default() += cents,
                    Some("employer") => employer += cents,
                    Some("contribution") => unlabeled += cents,
                    _ => continue,
                }
                latest = Some(*date);
            }
            Plan::Workplace => {}
            Plan::Ira if cents > 0 => {
                let Some(deposit) = deposit(
                    transaction,
                    account_id,
                    *date,
                    &profile.contribution_years,
                    stated,
                ) else {
                    continue;
                };
                if let Some(tax_year) = deposit.counts_for {
                    *ira.entry(tax_year).or_default() += cents;
                    *contributed.entry(account_id).or_default() += cents;
                }
                if deposit.waiting || date.year() >= first_ira_year {
                    deposits.push(deposit);
                }
            }
            Plan::Ira => {
                if matches!(transaction.flow.as_deref(), Some("withdrawal" | "transfer")) {
                    *withdrawn.entry(account_id).or_default() -= cents;
                }
            }
        }
    }

    // Pay statements: by year, what each period took for the 401(k) and for stock purchases,
    // and each employer's latest year-to-date deferrals.
    let mut pay_periods = BTreeMap::<i32, (i64, i64)>::new();
    let mut latest_pay = BTreeMap::<String, (NaiveDate, i64)>::new();
    for document in documents
        .iter()
        .filter(|document| document.kind == DocumentKind::Payslip)
    {
        let Some(date) = document
            .date
            .as_deref()
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
            .filter(|date| *date <= today)
        else {
            continue;
        };
        let cents = |key: &str, ytd: bool| {
            let value = if ytd {
                document.ytd(key)
            } else {
                document.amount(key)
            };
            minor_units(value.unwrap_or_default())
        };
        let period = pay_periods.entry(date.year()).or_default();
        period.0 += cents("pretax401k", false) + cents("roth401k", false);
        period.1 += cents("espp", false);
        if date.year() == year {
            let employer = document.issuer.clone().unwrap_or_default();
            let deferred = cents("pretax401k", true) + cents("roth401k", true);
            if latest_pay
                .get(&employer)
                .is_none_or(|(known, _)| *known < date)
            {
                latest_pay.insert(employer, (date, deferred));
            }
        }
    }
    let paid_this_year = latest_pay
        .values()
        .map(|(_, deferred)| deferred)
        .sum::<i64>();
    let workplace = (plans.values().any(|plan| *plan == Plan::Workplace) || paid_this_year > 0)
        .then(|| {
            let limit = limits(year).map(|limits| {
                limits.elective_deferral
                    + match age(year) {
                        Some(60..=63) => limits.catch_up60_to63,
                        Some(age) if age >= 50 => limits.catch_up50,
                        _ => 0.0,
                    }
            });
            // Pay statements' year-to-date figure covers paychecks the plan export hasn't yet.
            let exported = deferrals.values().sum::<i64>();
            let deferred = exported.max(paid_this_year);
            let dates = deferrals.keys().copied().collect::<Vec<_>>();
            let year_end = NaiveDate::from_ymd_opt(year, 12, 31).expect("valid date");
            let paychecks_left = cadence(&dates).zip(dates.last()).map(|(gap, last)| {
                let mut count = 0;
                let mut next = *last + Duration::days(gap);
                while next <= year_end {
                    count += 1;
                    next += Duration::days(gap);
                }
                count
            });
            let last = dates.last().and_then(|date| deferrals.get(date)).copied();
            let projected = paychecks_left
                .zip(last)
                .map(|(left, last)| deferred + i64::from(left) * last);
            let per_paycheck_to_max = limit
                .zip(paychecks_left)
                .filter(|(limit, left)| *left > 0 && minor_units(*limit) > deferred)
                .map(|(limit, left)| (minor_units(limit) - deferred) / i64::from(left));
            Workplace {
                account_ids: plans
                    .iter()
                    .filter(|(_, plan)| **plan == Plan::Workplace)
                    .map(|(id, _)| (*id).to_owned())
                    .collect(),
                limit,
                deferred: from_minor(deferred),
                from_pay_statements: paid_this_year > exported,
                unlabeled: from_minor(unlabeled),
                employer: from_minor(employer),
                through: latest,
                paychecks_left,
                projected: projected.map(from_minor),
                per_paycheck_to_max: per_paycheck_to_max.map(from_minor),
            }
        });

    let ira_years = if plans.values().any(|plan| *plan == Plan::Ira) {
        (first_ira_year..=year)
            .map(|tax_year| IraYear {
                tax_year,
                limit: limits(tax_year).map(|limits| {
                    limits.ira
                        + if age(tax_year).is_some_and(|age| age >= 50) {
                            limits.ira_catch_up50
                        } else {
                            0.0
                        }
                }),
                contributed: from_minor(ira.get(&tax_year).copied().unwrap_or_default()),
                deadline: ira_deadline(tax_year),
            })
            .collect()
    } else {
        Vec::new()
    };

    // Pre-tax, tax-free, and taxable, in cents.
    let mut held = [0_i64; 3];
    let mut complete = true;
    for account in accounts
        .iter()
        .filter(|account| matches!(account.r#type.as_str(), "brokerage" | "retirement"))
    {
        let Some(value) = account.value else {
            complete = false;
            continue;
        };
        let bucket = if account.taxable() {
            2
        } else if account.tax_treatment.as_deref() == Some("taxFree") {
            1
        } else {
            0
        };
        held[bucket] += minor_units(value);
    }
    let invested = held.iter().sum::<i64>();
    let buckets = Buckets {
        pre_tax: from_minor(held[0]),
        tax_free: from_minor(held[1]),
        taxable: from_minor(held[2]),
        complete,
    };

    let roth_basis = accounts
        .iter()
        .filter(|account| {
            plans.get(account.id.as_str()) == Some(&Plan::Ira)
                && account.tax_treatment.as_deref() == Some("taxFree")
        })
        .map(|account| {
            let id = account.id.as_str();
            let earlier = profile.earlier_contributions.get(id).copied();
            let added = contributed.get(id).copied().unwrap_or_default();
            let taken = withdrawn.get(id).copied().unwrap_or_default();
            RothBasis {
                account_id: id.to_owned(),
                since: since.get(id).copied(),
                earlier,
                contributed: from_minor(added),
                withdrawn: from_minor(taken),
                basis: from_minor(
                    (earlier.map(minor_units).unwrap_or_default() + added - taken).max(0),
                ),
            }
        })
        .collect();

    let savings = match posted.first() {
        Some((first, _)) => [year, year - 1]
            .into_iter()
            .filter_map(|savings_year| {
                let start = NaiveDate::from_ymd_opt(savings_year, 1, 1)?;
                let end = NaiveDate::from_ymd_opt(savings_year, 12, 31)?.min(today);
                // A past year counts only when history covers all of it.
                if savings_year < year && *first > start {
                    return None;
                }
                let from = start.max(*first);
                let (spending, earned) = recap::spent_and_earned(&posted, from, end);
                let (paid_deferred, espp) =
                    pay_periods.get(&savings_year).copied().unwrap_or_default();
                // The plan's export and the pay statements report the same deferrals; whichever
                // covers more of the year counts.
                let deferred = deferred_by_year
                    .get(&savings_year)
                    .copied()
                    .unwrap_or_default()
                    .max(paid_deferred);
                let gross = earned + deferred + espp;
                Some(SavingsYear {
                    year: savings_year,
                    from,
                    through: end,
                    partial: from > start,
                    take_home: from_minor(earned),
                    spending: from_minor(spending),
                    deferred: from_minor(deferred),
                    espp: from_minor(espp),
                    rate: (gross > 0)
                        .then(|| round2((gross - spending) as f64 * 100.0 / gross as f64)),
                })
            })
            .collect(),
        None => Vec::new(),
    };

    let planned = profile
        .planned_monthly
        .map(|monthly| minor_units(monthly) * 12)
        .unwrap_or_default();
    let coverage = recap
        .iter()
        .find(|period| period.window == Window::Year && !period.partial)
        .map(|period| minor_units(period.spending) + planned)
        .filter(|annual| *annual > 0)
        .map(|annual| Coverage {
            invested: from_minor(invested),
            complete: buckets.complete,
            spending: from_minor(annual - planned),
            planned: from_minor(planned),
            years: round2(invested as f64 / annual as f64),
        });

    let uncounted = accounts
        .iter()
        .filter(|account| {
            account.r#type == "retirement" && !plans.contains_key(account.id.as_str())
        })
        .map(|account| account.id.clone())
        .collect();

    Retirement {
        year,
        workplace,
        ira: ira_years,
        deposits,
        buckets,
        roth_basis,
        savings,
        coverage,
        uncounted,
    }
}

/// What calls for the user: inflows to place, and limits passed or about to be.
pub fn needs(retirement: &Retirement) -> Vec<AttentionItem> {
    let mut needs = Vec::new();
    let mut waiting = BTreeMap::<&str, (usize, i64, &str)>::new();
    for inflow in retirement.deposits.iter().filter(|deposit| deposit.waiting) {
        let entry = waiting
            .entry(inflow.account_id.as_str())
            .or_insert((0, 0, ""));
        entry.0 += 1;
        entry.1 += minor_units(inflow.amount);
        // Named by the newest, so a new inflow raises it again.
        entry.2 = inflow.transaction_id.as_str();
    }
    for (account_id, (count, amount, newest)) in waiting {
        needs.push(AttentionItem {
            id: format!("contributions-to-confirm:{newest}"),
            severity: Severity::Attention,
            item: Item::ContributionsToConfirm {
                account_id: account_id.to_owned(),
                count,
                amount: from_minor(amount),
            },
        });
    }
    let mut limit = |plan: &'static str, tax_year: i32, limit: f64, amount: f64, on_pace| {
        needs.push(AttentionItem {
            id: format!(
                "contribution-limit:{plan}:{tax_year}:{}",
                if on_pace { "pace" } else { "over" }
            ),
            severity: if on_pace {
                Severity::Attention
            } else {
                Severity::Urgent
            },
            item: Item::ContributionLimit {
                plan,
                tax_year,
                limit,
                amount,
                on_pace,
            },
        })
    };
    if let Some(workplace) = &retirement.workplace {
        if let Some(cap) = workplace.limit {
            if minor_units(workplace.deferred) > minor_units(cap) {
                limit("workplace", retirement.year, cap, workplace.deferred, false);
            } else if let Some(projected) = workplace
                .projected
                .filter(|projected| minor_units(*projected) > minor_units(cap))
            {
                limit("workplace", retirement.year, cap, projected, true);
            }
        }
    }
    for year in &retirement.ira {
        if let Some(cap) = year
            .limit
            .filter(|cap| minor_units(year.contributed) > minor_units(*cap))
        {
            limit("ira", year.tax_year, cap, year.contributed, false);
        }
    }
    needs
}

#[cfg(test)]
mod tests;

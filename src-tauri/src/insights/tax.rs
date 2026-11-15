//! Taxes: what the year's 1099s will roughly say, and what it may cost. Brief sees interest,
//! dividends, and sales in taxable accounts; the tax estimate stacks these on the taxable
//! income the user enters and says it is an estimate. Without that income, amounts are
//! reported and no tax is guessed. Wages are seen only through pay statements: `withholding`
//! carries the year's federal withholding to December and sets it against the federal tax and
//! the estimated-tax safe harbors, and the typed income stays the basis without one.
//!
//! Gains come from the first-in-first-out estimate on saved trades. A loss sale is a wash
//! sale when the same ticker is bought in any account within 30 days either side; a
//! replacement in a tax-advantaged account makes the loss permanent. Replacement shares are
//! matched oldest first and each replaces at most one sale's shares. Unclassified dividends
//! make the estimate a range, from all qualified to all ordinary. Rates come from
//! `tax-tables.json`; a year without tables has no estimate. Amounts sum in whole cents.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use chrono::{Datelike, Duration, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::calendar;
use super::lots::PositionLots;
use crate::finance_contract::Transaction;
use crate::finance_contract::{Account, Trade};
use crate::numeric::round2;
use crate::numeric::{from_minor, minor_units};
use crate::settings::TaxProfile;
use crate::transaction_policy::Kind;

/// A purchase this close to a loss sale, before or after, replaces the shares sold.
pub const WASH_DAYS: i64 = 30;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tables {
    version: String,
    sources: Vec<String>,
    niit_rate: f64,
    niit_thresholds: BTreeMap<String, f64>,
    capital_loss_limit: BTreeMap<String, f64>,
    additional_medicare: AdditionalMedicare,
    safe_harbor: SafeHarborRules,
    years: BTreeMap<i32, YearTables>,
}

/// The Additional Medicare Tax on wages and self-employment earnings, not indexed.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdditionalMedicare {
    rate: f64,
    /// Per filing status: what the year's Medicare wages and earnings may reach untaxed.
    thresholds: BTreeMap<String, f64>,
    /// Each employer withholds it on Medicare wages above this, whatever the filing status.
    withholding_threshold: f64,
}

/// Net earnings from self-employment as a share of contractor pay (26 U.S.C. 1402(a)(12)).
const NET_EARNINGS: f64 = 0.9235;

/// Self-employment tax: 15.3% of net earnings, the Social Security part only up to what the
/// year's Social Security wages left of the wage base; none under $400.
pub fn self_employment_tax(year: i32, self_employment: f64, social_wages: f64) -> f64 {
    let net = self_employment * NET_EARNINGS;
    if net < 400.0 {
        return 0.0;
    }
    let room = social_security_wage_base(year).map_or(net, |base| (base - social_wages).max(0.0));
    round2(net.min(room) * 0.124 + net * 0.029)
}

/// The Additional Medicare Tax on Medicare wages and net self-employment earnings together
/// above the filing status's threshold.
fn additional_medicare(status: &str, medicare_wages: f64, self_employment: f64) -> Option<f64> {
    let rules = &TABLES.additional_medicare;
    let over = medicare_wages + self_employment * NET_EARNINGS - rules.thresholds.get(status)?;
    Some(round2(rules.rate / 100.0 * over.max(0.0)))
}

/// The estimated-tax penalty's safe harbors (26 U.S.C. 6654(d)(1) and (e)(1)), not indexed.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SafeHarborRules {
    current_year_percent: f64,
    prior_year_percent: f64,
    high_income_prior_year_percent: f64,
    /// Prior-year adjusted gross income above this takes the higher prior-year percent.
    high_income_agi: BTreeMap<String, f64>,
    /// Owing less than this after withholding is never penalized.
    small_balance: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct YearTables {
    /// Per filing status: each bracket's upper bound (`None` for the top) and rate.
    ordinary: BTreeMap<String, Vec<(Option<f64>, f64)>>,
    /// Per filing status: the top of the 0% band and of the 15% band.
    capital_gains: BTreeMap<String, (f64, f64)>,
    #[serde(default)]
    standard_deduction: BTreeMap<String, f64>,
    /// Earnings above this owe no Social Security tax.
    #[serde(default)]
    social_security_wage_base: Option<f64>,
}

/// The standard deduction for `year` and a filing status, when the tables have the year.
pub fn standard_deduction(year: i32, filing_status: &str) -> Option<f64> {
    TABLES
        .years
        .get(&year)?
        .standard_deduction
        .get(filing_status)
        .copied()
}

/// The year's Social Security wage base, when the tables have the year.
pub fn social_security_wage_base(year: i32) -> Option<f64> {
    TABLES.years.get(&year)?.social_security_wage_base
}

static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    serde_json::from_str(include_str!("tax-tables.json")).expect("tax tables are valid")
});

#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Dividends {
    pub total: f64,
    pub qualified: f64,
    pub non_qualified: f64,
    /// Dividends whose provider didn't say which; the estimate is a range because of them.
    pub unclassified: f64,
    /// Long-term capital gain distributions, 1099-DIV box 2a, outside `total` and taxed as
    /// long-term gains.
    pub capital_gain_distributions: f64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Realized {
    /// After wash-sale adjustments.
    pub short_term: f64,
    pub long_term: f64,
    /// Losses disallowed by wash sales, positive.
    pub wash_disallowed: f64,
    pub sales: usize,
    /// Sales whose gain the saved history can't estimate, and their proceeds.
    pub unestimated_sales: usize,
    pub unestimated_proceeds: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct WashSale {
    pub sale_id: String,
    pub ticker: String,
    pub account_id: String,
    pub date: NaiveDate,
    /// The sale's loss, negative.
    pub loss: f64,
    /// The part of the loss disallowed, positive.
    pub disallowed: f64,
    pub replacement_trade_ids: Vec<String>,
    /// A replacement in a tax-advantaged account: the loss is never recovered.
    pub permanent: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaxEstimate {
    /// Federal and state tax on what Brief sees: `low` treats unclassified dividends as
    /// qualified, `high` as ordinary. Equal when every dividend is classified.
    pub low: f64,
    pub high: f64,
    /// The parts of `high`.
    pub ordinary: f64,
    pub capital_gains: f64,
    pub niit: f64,
    pub state: f64,
    /// Rates the next dollar would pay, in percent.
    pub marginal_rate: f64,
    pub capital_gains_rate: f64,
    /// A net capital loss deducted from ordinary income, and what carries to next year.
    pub loss_deduction: f64,
    pub loss_carryover: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaxYear {
    pub year: i32,
    pub interest: f64,
    pub dividends: Dividends,
    pub realized: Realized,
    pub wash_sales: Vec<WashSale>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub estimate: Option<TaxEstimate>,
    pub tables_available: bool,
    /// Saved activity starts after January 1, so the year's amounts are incomplete.
    pub partial: bool,
    /// This year only: last year's interest and dividends still to come (`still_to_come`);
    /// absent when saved history doesn't cover all of last year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub still_to_come: Option<StillToCome>,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct StillToCome {
    pub interest: f64,
    pub dividends: Dividends,
}

/// A taxable position below cost, with what selling it might save.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct HarvestEstimate {
    pub ticker: String,
    pub account_id: String,
    /// Unrealized losses on lots held a year or less, and longer, negative.
    pub short_term_loss: f64,
    pub long_term_loss: f64,
    /// This year's estimate less the estimate with this sale alone, when there is a tax
    /// profile: a loss beyond this year's gains and the capital-loss limit carries over and
    /// isn't counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tax_savings: Option<f64>,
    /// A purchase in the last 30 days would make a sale before this day a wash sale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wash_until: Option<NaiveDate>,
}

/// A recent loss sale whose window is still open: buying the ticker on or before `until`
/// would disallow the loss.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct WashWindow {
    pub sale_id: String,
    pub ticker: String,
    pub account_id: String,
    pub sold_on: NaiveDate,
    pub until: NaiveDate,
    pub loss: f64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Taxes {
    /// This year first.
    pub years: Vec<TaxYear>,
    pub harvest: Vec<HarvestEstimate>,
    pub wash_windows: Vec<WashWindow>,
    pub has_profile: bool,
    pub tables_version: String,
    pub sources: Vec<String>,
    /// This year's federal withholding against the tax it projects, from pay statements;
    /// absent without a pay statement this year, when the typed taxable income is the basis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub withholding: Option<Withholding>,
}

/// Whether withholding meets the year's estimated-tax safe harbor, as federal tax only.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum HarborMet {
    /// At least 90% of this year's projected tax.
    CurrentYear,
    /// At least 100% (110% above the income threshold) of last year's tax.
    PriorYear,
    /// Less than $1,000 left to pay.
    SmallBalance,
    /// None of the tests is met by what Brief can see.
    None,
    /// A test Brief can't run might be met.
    Unknown,
}

/// One safe-harbor test: what withholding must reach and whether it does, each unavailable
/// when the tax it rests on is unknown.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Harbor {
    /// The share of the year's tax it takes, in percent.
    pub percent: f64,
    /// The tax it is a share of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tax: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub required: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub met: Option<bool>,
}

/// This year's federal withholding carried to December from each employer's latest pay
/// statement (`documents::pay_projections`), with the Additional Medicare Tax employers
/// withhold, against the federal tax on the projected wages and contractor pay less half its
/// self-employment tax and the standard deduction, plus this year's interest, dividends, and
/// contractor pay so far with what last year paid after this day, and its gains as realized
/// (unclassified dividends as ordinary), plus self-employment and Additional Medicare tax, at
/// the filing status of the tax profile. Federal only: no state tax, credits, other
/// deductions, or other income. Last year's tax is figured the same way from last year's
/// W-2s and contractor pay, only when every employer's W-2 for the year is in; it stands in
/// for the return's total tax, which is the figure the rule names.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Withholding {
    pub year: i32,
    /// The statements read, one per employer.
    pub basis: Vec<super::documents::PayProjection>,
    /// Employers seen only in payroll deposits, whose pay no statement projects.
    pub uncounted_employers: Vec<String>,
    /// Unavailable when a statement can't be carried forward or an employer is uncounted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub projected_wages: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub projected_withholding: Option<f64>,
    /// The filing status the tax uses, from the tax profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub filing_status: Option<String>,
    /// Unavailable without a filing status, a projection, or the year's tables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub projected_tax: Option<f64>,
    /// Projected withholding less projected tax: below zero is a shortfall.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub difference: Option<f64>,
    pub current_year: Harbor,
    pub prior_year: Harbor,
    pub met: HarborMet,
    /// The interest, dividends, and contractor pay still to come are what last year paid
    /// after this day; without saved history for all of last year, only what has come so far
    /// counts.
    pub from_last_year: bool,
}

fn ordinary_tax(brackets: &[(Option<f64>, f64)], income: f64) -> f64 {
    let mut tax = 0.0;
    let mut floor = 0.0;
    for (ceiling, rate) in brackets {
        let top = ceiling.unwrap_or(f64::INFINITY);
        if income > floor {
            tax += (income.min(top) - floor) * rate / 100.0;
        }
        floor = top;
    }
    tax
}

fn marginal(brackets: &[(Option<f64>, f64)], income: f64) -> f64 {
    brackets
        .iter()
        .find(|(ceiling, _)| ceiling.is_none_or(|ceiling| income < ceiling))
        .map(|(_, rate)| *rate)
        .unwrap_or(0.0)
}

/// Preferential income taxed at 0%, 15%, and 20%, stacked on top of ordinary income.
fn capital_gains_tax((zero, fifteen): (f64, f64), ordinary: f64, preferential: f64) -> f64 {
    let band = |low: f64, high: f64| {
        let start = ordinary.max(low);
        let end = (ordinary + preferential).min(high);
        (end - start).max(0.0)
    };
    band(zero, fifteen) * 0.15 + band(fifteen, f64::INFINITY) * 0.20
}

fn capital_gains_marginal((zero, fifteen): (f64, f64), income: f64) -> f64 {
    if income < zero {
        0.0
    } else if income < fifteen {
        15.0
    } else {
        20.0
    }
}

/// The year's investment income, in cents, for one scenario of the unclassified dividends.
struct Income {
    interest: i64,
    qualified: i64,
    ordinary_dividends: i64,
    short_term: i64,
    long_term: i64,
}

struct Scenario {
    total: f64,
    ordinary: f64,
    capital_gains: f64,
    niit: f64,
    state: f64,
    marginal_rate: f64,
    capital_gains_rate: f64,
    loss_deduction: f64,
    loss_carryover: f64,
}

fn scenario(profile: &TaxProfile, year: &YearTables, income: &Income) -> Option<Scenario> {
    let status = profile.filing_status.as_str();
    let brackets = year.ordinary.get(status)?;
    let bands = *year.capital_gains.get(status)?;
    let limit = *TABLES.capital_loss_limit.get(status)?;
    let threshold = *TABLES.niit_thresholds.get(status)?;
    let (short, long) = (from_minor(income.short_term), from_minor(income.long_term));
    // Net short against long, then deduct a net loss up to the limit.
    let net = short + long;
    let (ordinary_gain, preferential_gain, loss) = if short >= 0.0 && long >= 0.0 {
        (short, long, 0.0)
    } else if net >= 0.0 {
        if short < 0.0 {
            (0.0, net, 0.0)
        } else {
            (net, 0.0, 0.0)
        }
    } else {
        (0.0, 0.0, -net)
    };
    let deduction = loss.min(limit);
    let interest = from_minor(income.interest);
    let dividends = from_minor(income.qualified + income.ordinary_dividends);
    let base = profile.taxable_income;
    let ordinary_extra =
        interest + from_minor(income.ordinary_dividends) + ordinary_gain - deduction;
    let ordinary_total = base + ordinary_extra;
    let ordinary_income = ordinary_total.max(0.0);
    // A deduction ordinary income doesn't use up shelters preferential income next.
    let preferential_income = from_minor(income.qualified) + preferential_gain;
    let preferential = (preferential_income + ordinary_total.min(0.0)).max(0.0);
    let ordinary = ordinary_tax(brackets, ordinary_income) - ordinary_tax(brackets, base);
    let capital_gains = capital_gains_tax(bands, ordinary_income, preferential);
    // As on Form 8960, the capital loss deducted reduces net investment income.
    let investment_income =
        (interest + dividends + ordinary_gain + preferential_gain - deduction).max(0.0);
    // Modified AGI: taxable income with the standard deduction added back, which is low by
    // anything itemized above it.
    let standard = year.standard_deduction.get(status).copied().unwrap_or(0.0);
    let magi = ordinary_total + preferential_income + standard;
    let niit = TABLES.niit_rate / 100.0 * investment_income.min((magi - threshold).max(0.0));
    let state_rate = profile.state_rate.unwrap_or(0.0);
    let state = state_rate / 100.0
        * (interest + dividends + ordinary_gain + preferential_gain - deduction).max(0.0);
    let over_threshold = magi > threshold;
    let niit_marginal = if over_threshold {
        TABLES.niit_rate
    } else {
        0.0
    };
    Some(Scenario {
        total: round2(ordinary + capital_gains + niit + state),
        ordinary: round2(ordinary),
        capital_gains: round2(capital_gains),
        niit: round2(niit),
        state: round2(state),
        marginal_rate: marginal(brackets, ordinary_income) + niit_marginal + state_rate,
        capital_gains_rate: capital_gains_marginal(bands, ordinary_income + preferential)
            + niit_marginal
            + state_rate,
        loss_deduction: round2(deduction),
        loss_carryover: round2(loss - deduction),
    })
}

fn sale(trade: &Trade) -> bool {
    trade.r#type.to_ascii_uppercase().contains("SELL")
}

fn purchase(trade: &Trade) -> bool {
    let kind = trade.r#type.to_ascii_uppercase();
    kind.contains("BUY") || kind.contains("REINVEST")
}

fn ticker(trade: &Trade) -> Option<String> {
    trade
        .ticker
        .as_deref()
        .map(|ticker| ticker.trim().to_ascii_uppercase())
        .filter(|ticker| !ticker.is_empty())
}

fn trade_date(trade: &Trade) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&trade.date, "%Y-%m-%d").ok()
}

/// Wash sales among loss sales in taxable accounts, and the open windows of recent ones.
fn wash_sales(
    trades: &[Trade],
    taxable: &dyn Fn(&str) -> bool,
    today: NaiveDate,
) -> (Vec<WashSale>, Vec<WashWindow>) {
    let mut ordered = trades
        .iter()
        .filter_map(|trade| Some((trade_date(trade)?, ticker(trade)?, trade)))
        .collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.0.cmp(&right.0).then(left.2.id.cmp(&right.2.id)));
    // Each purchase's shares not yet matched to an earlier loss sale.
    let mut unused = ordered
        .iter()
        .map(|(_, _, trade)| trade.units.map(f64::abs).unwrap_or(0.0))
        .collect::<Vec<_>>();
    let mut washes = Vec::new();
    let mut windows = Vec::new();
    for (position, (date, symbol, trade)) in ordered.iter().enumerate() {
        let Some(gain) = trade.estimated_realized_gain.filter(|gain| *gain < 0.0) else {
            continue;
        };
        if !sale(trade) || !taxable(&trade.account_id) {
            continue;
        }
        let sold = trade.units.map(f64::abs).filter(|units| *units > 0.0);
        let Some(sold) = sold else {
            continue;
        };
        // Shares left in this account after the sale: FIFO sells the oldest, so recent
        // purchases are what remains, and only those can replace the shares sold.
        let remaining = ordered[..=position]
            .iter()
            .filter(|(_, other, candidate)| {
                other == symbol && candidate.account_id == trade.account_id
            })
            .map(|(_, _, candidate)| {
                let units = candidate.units.map(f64::abs).unwrap_or(0.0);
                if purchase(candidate) {
                    units
                } else if sale(candidate) {
                    -units
                } else {
                    0.0
                }
            })
            .sum::<f64>()
            .max(0.0);
        // Replacement shares are matched in the order bought, and each share replaces
        // at most one sale: what an earlier loss sale consumed is gone for this one.
        let mut needed = sold;
        let mut same_cap = remaining;
        let mut replacements = Vec::new();
        let mut permanent = false;
        for (index, (other_date, other, candidate)) in ordered.iter().enumerate() {
            if needed <= 0.0 {
                break;
            }
            if other != symbol
                || candidate.id == trade.id
                || !purchase(candidate)
                || (*other_date - *date).num_days().abs() > WASH_DAYS
            {
                continue;
            }
            let mut take = unused[index].min(needed);
            if candidate.account_id == trade.account_id && other_date <= date {
                take = take.min(same_cap);
                same_cap -= take;
            }
            if take <= 0.0 {
                continue;
            }
            unused[index] -= take;
            needed -= take;
            permanent |= !taxable(&candidate.account_id);
            replacements.push(candidate.id.clone());
        }
        let replaced = sold - needed;
        if replaced <= 0.0 {
            if (today - *date).num_days() <= WASH_DAYS && *date <= today {
                windows.push(WashWindow {
                    sale_id: trade.id.clone(),
                    ticker: symbol.clone(),
                    account_id: trade.account_id.clone(),
                    sold_on: *date,
                    until: *date + Duration::days(WASH_DAYS),
                    loss: gain,
                });
            }
            continue;
        }
        washes.push(WashSale {
            sale_id: trade.id.clone(),
            ticker: symbol.clone(),
            account_id: trade.account_id.clone(),
            date: *date,
            loss: gain,
            disallowed: round2(-gain * replaced / sold),
            replacement_trade_ids: replacements,
            permanent,
        });
    }
    (washes, windows)
}

/// What the tax figures are read from.
pub struct TaxInputs<'a> {
    pub transactions: &'a [Transaction],
    pub trades: &'a [Trade],
    pub accounts: &'a [Account],
    /// Each taxable lot's `estimated_tax` is filled in.
    pub positions: &'a mut [PositionLots],
    /// Taxable losses large enough to harvest, by ticker and account.
    pub harvestable: &'a [(String, String)],
    pub profile: Option<&'a TaxProfile>,
}

/// Whether last year's payment on `paid` is still to come by `today`, given the days this
/// year's payments from the same source arrived: one within ten days of its anniversary is
/// that payment landing early or late, and one more than ten days overdue isn't expected.
pub fn still_to_come(
    paid: NaiveDate,
    today: NaiveDate,
    this_year: impl IntoIterator<Item = NaiveDate>,
) -> bool {
    const DAYS: i64 = 10;
    paid.checked_add_months(Months::new(12))
        .is_some_and(|anniversary| {
            (anniversary - today).num_days() > -DAYS
                && !this_year
                    .into_iter()
                    .any(|date| (date - anniversary).num_days().abs() <= DAYS)
        })
}

/// Each year's 1099 amounts and estimate, harvest values, and open wash-sale windows.
pub fn taxes(inputs: TaxInputs, today: NaiveDate) -> Taxes {
    let TaxInputs {
        transactions,
        trades,
        accounts,
        positions,
        harvestable,
        profile,
    } = inputs;
    let treatments = accounts
        .iter()
        .map(|account| (account.id.as_str(), account.taxable()))
        .collect::<BTreeMap<_, _>>();
    // Accounts Brief doesn't know, like a statement-only account, are taxable.
    let taxable = |id: &str| treatments.get(id).copied().unwrap_or(true);
    let (washes, wash_windows) = wash_sales(trades, &taxable, today);
    let first_activity = transactions
        .iter()
        .filter_map(|transaction| calendar::transaction_date(&transaction.date))
        .chain(trades.iter().filter_map(trade_date))
        .min();

    let mut years = BTreeSet::from([today.year()]);
    for transaction in transactions {
        if matches!(transaction.kind(), Some(Kind::Interest | Kind::Dividend)) {
            if let Some(date) = calendar::transaction_date(&transaction.date) {
                years.insert(date.year());
            }
        }
    }
    years.extend(
        trades
            .iter()
            .filter(|trade| sale(trade))
            .filter_map(trade_date)
            .map(|date| date.year()),
    );
    // This year and last; older years have been filed.
    let years = years
        .into_iter()
        .filter(|year| *year >= today.year() - 1 && *year <= today.year())
        .rev()
        .collect::<Vec<_>>();

    let mut results = Vec::new();
    let mut this_year = None;
    for year in years {
        let in_year = |date: NaiveDate| date.year() == year && date <= today;
        let earned = Earned::in_year(transactions, &taxable, &|_, date| in_year(date));
        let (realized, short, long) = realized(trades, &washes, &taxable, &in_year);
        let tables = TABLES.years.get(&year);
        let estimate = profile
            .zip(tables)
            .and_then(|(profile, tables)| estimate(profile, tables, &earned, short, long));
        let partial = |year: i32| {
            first_activity
                .is_none_or(|first| first > NaiveDate::from_ymd_opt(year, 1, 1).unwrap_or(first))
        };
        let still_to_come = (year == today.year() && !partial(year - 1)).then(|| {
            let later = |transaction: &Transaction, date: NaiveDate| {
                date.year() == year - 1
                    && matches!(transaction.kind(), Some(Kind::Interest | Kind::Dividend))
                    && still_to_come(
                        date,
                        today,
                        transactions
                            .iter()
                            .filter(|other| {
                                !other.pending
                                    && other.account_id == transaction.account_id
                                    && other.merchant == transaction.merchant
                                    && other.kind() == transaction.kind()
                                    && other.dividend_type == transaction.dividend_type
                            })
                            .filter_map(|other| calendar::transaction_date(&other.date))
                            .filter(|other| other.year() == year && *other <= today),
                    )
            };
            let earned = Earned::in_year(transactions, &taxable, &later);
            StillToCome {
                interest: from_minor(earned.interest),
                dividends: earned.dividends(),
            }
        });
        results.push(TaxYear {
            year,
            interest: from_minor(earned.interest),
            dividends: earned.dividends(),
            realized,
            wash_sales: washes
                .iter()
                .filter(|wash| wash.date.year() == year)
                .cloned()
                .collect(),
            estimate,
            tables_available: tables.is_some(),
            partial: partial(year),
            still_to_come,
        });
        if year == today.year() {
            this_year = Some((earned, short, long));
        }
    }

    // This year's tax with a sale less without it, so a loss offsets this year's gains, then
    // other income only to the limit; what carries over saves nothing this year.
    let tables = TABLES.years.get(&today.year());
    let total = |earned: &Earned, short, long| {
        Some(scenario(profile?, tables?, &earned.income(false, short, long))?.total)
    };
    let without = this_year.as_ref().and_then(|(earned, short, long)| {
        Some((earned, *short, *long, total(earned, *short, *long)?))
    });
    let sale_tax = |short_gain: i64, long_gain: i64| {
        let (earned, short, long, without) = without?;
        Some(round2(
            total(earned, short + short_gain, long + long_gain)? - without,
        ))
    };
    for position in positions
        .iter_mut()
        .filter(|position| !position.tax_deferred)
    {
        for lot in &mut position.lots {
            let gain = lot.gain.map(minor_units);
            lot.estimated_tax = gain.and_then(|gain| {
                if lot.long_term {
                    sale_tax(0, gain)
                } else {
                    sale_tax(gain, 0)
                }
            });
        }
    }
    let saving = |short_loss: i64, long_loss: i64| sale_tax(short_loss, long_loss).map(|tax| -tax);
    let harvest = harvest(harvestable, positions, trades, &saving, today);

    Taxes {
        years: results,
        harvest,
        wash_windows,
        has_profile: profile.is_some(),
        tables_version: TABLES.version.clone(),
        sources: TABLES.sources.clone(),
        withholding: None,
    }
}

/// Federal tax on `taxable` ordinary income plus `investment` stacked on it, at `status`.
fn federal_tax(year: &YearTables, status: &str, taxable: f64, investment: &Income) -> Option<f64> {
    let profile = TaxProfile {
        filing_status: status.to_owned(),
        taxable_income: taxable,
        state_rate: None,
    };
    let stacked = scenario(&profile, year, investment)?;
    let wages = ordinary_tax(year.ordinary.get(status)?, taxable);
    Some(round2(
        wages + stacked.ordinary + stacked.capital_gains + stacked.niit,
    ))
}

/// Last year's investment income, as far as saved history knows it.
pub enum PriorInvestment<'a> {
    /// Saved history doesn't cover all of last year.
    Unknown,
    /// It does, and holds no interest, dividends, or sales.
    None,
    Year(&'a TaxYear),
}

/// What saved history knows of the year before `today`'s in taxable accounts.
pub fn prior_investment<'a>(
    taxes: &'a Taxes,
    transactions: &[Transaction],
    trades: &[Trade],
    today: NaiveDate,
) -> PriorInvestment<'a> {
    let last = today.year() - 1;
    match taxes.years.iter().find(|year| year.year == last) {
        Some(year) if !year.partial => PriorInvestment::Year(year),
        Some(_) => PriorInvestment::Unknown,
        None => {
            let first = transactions
                .iter()
                .filter_map(|transaction| calendar::transaction_date(&transaction.date))
                .chain(trades.iter().filter_map(trade_date))
                .min();
            match NaiveDate::from_ymd_opt(last, 1, 1) {
                Some(start) if first.is_some_and(|first| first <= start) => PriorInvestment::None,
                _ => PriorInvestment::Unknown,
            }
        }
    }
}

/// A tax year's investment income, in cents.
fn income_of(year: &TaxYear) -> Income {
    Income {
        interest: minor_units(year.interest),
        qualified: minor_units(year.dividends.qualified),
        ordinary_dividends: minor_units(year.dividends.non_qualified + year.dividends.unclassified),
        short_term: minor_units(year.realized.short_term),
        long_term: minor_units(year.realized.long_term)
            + minor_units(year.dividends.capital_gain_distributions),
    }
}

/// What investment income adds to AGI: interest, dividends, and net gains, a net loss
/// counting only to the filing status's capital-loss limit.
fn investment_agi(income: &Income, status: &str) -> Option<f64> {
    let limit = minor_units(*TABLES.capital_loss_limit.get(status)?);
    let gains = (income.short_term + income.long_term).max(-limit);
    Some(from_minor(
        income.interest + income.qualified + income.ordinary_dividends + gains,
    ))
}

/// A year's pay, for its federal tax.
struct Earnings {
    /// W-2 boxes 1 and 5.
    wages: f64,
    medicare_wages: f64,
    self_employment: f64,
    self_employment_tax: f64,
}

/// What `Withholding` describes, for `today`'s year: `projections` from its pay statements
/// (`None` when one can't be carried forward), `uncounted_employers` those paying without
/// one, which leave the projection unavailable too, `investment` its tax year so far,
/// `self_employment` its contractor pay so far and what last year's arrived after this day,
/// `prior_earnings` last year's income from its documents when every W-2 is in, and
/// `prior_investment` last year's investment income. Without the last two, the prior-year
/// test can't be settled.
#[allow(clippy::too_many_arguments)]
pub fn withholding(
    today: NaiveDate,
    projections: Option<Vec<super::documents::PayProjection>>,
    uncounted_employers: Vec<String>,
    investment: Option<&TaxYear>,
    (self_employment, self_employment_later): (f64, f64),
    prior_earnings: Option<&super::documents::Income>,
    prior_investment: PriorInvestment<'_>,
    filing_status: Option<&str>,
) -> Option<Withholding> {
    if projections.as_ref().is_some_and(Vec::is_empty) {
        return None;
    }
    let year = today.year();
    let rules = &TABLES.safe_harbor;
    let sum = |value: fn(&super::documents::PayProjection) -> f64| {
        let projections = projections
            .as_ref()
            .filter(|_| uncounted_employers.is_empty());
        projections.map(|projections| {
            from_minor(
                projections
                    .iter()
                    .map(|projection| minor_units(value(projection)))
                    .sum(),
            )
        })
    };
    let projected_wages = sum(|projection| projection.projected_wages);
    // Employers withhold the Additional Medicare Tax above their own threshold, and it counts
    // as paid.
    let projected_withholding = sum(|projection| {
        let rules = &TABLES.additional_medicare;
        let over = projection.projected_medicare_wages - rules.withholding_threshold;
        projection.projected_withholding + rules.rate / 100.0 * over.max(0.0)
    });
    // Interest, dividends, and contractor pay still to come are what last year paid after this
    // day, when saved history covers last year; gains count as realized.
    let still_to_come = investment.and_then(|year| year.still_to_come.as_ref());
    let income = investment.map(income_of).map(|income| match still_to_come {
        Some(later) => {
            let dividends = &later.dividends;
            Income {
                interest: income.interest + minor_units(later.interest),
                qualified: income.qualified + minor_units(dividends.qualified),
                ordinary_dividends: income.ordinary_dividends
                    + minor_units(dividends.non_qualified)
                    + minor_units(dividends.unclassified),
                long_term: income.long_term + minor_units(dividends.capital_gain_distributions),
                ..income
            }
        }
        None => income,
    });
    let self_employment = if still_to_come.is_some() {
        round2(self_employment + self_employment_later)
    } else {
        self_employment
    };
    let earnings = projected_wages
        .zip(sum(|projection| projection.projected_medicare_wages))
        .map(|(wages, medicare_wages)| Earnings {
            wages,
            medicare_wages,
            self_employment,
            // Social Security wages are Medicare wages up to the wage base, which the tax caps.
            self_employment_tax: self_employment_tax(year, self_employment, medicare_wages),
        });
    let tax_in = |year: i32, earnings: &Earnings, income: &Income| {
        let status = filing_status?;
        let tables = TABLES.years.get(&year)?;
        // Below zero, the deduction wages leave carries to investment income.
        let taxable = earnings.wages + earnings.self_employment
            - earnings.self_employment_tax / 2.0
            - tables.standard_deduction.get(status)?;
        Some(round2(
            federal_tax(tables, status, taxable, income)?
                + earnings.self_employment_tax
                + additional_medicare(status, earnings.medicare_wages, earnings.self_employment)?,
        ))
    };
    let nothing = Income {
        interest: 0,
        qualified: 0,
        ordinary_dividends: 0,
        short_term: 0,
        long_term: 0,
    };
    let projected_tax = earnings
        .as_ref()
        .zip(income.as_ref())
        .and_then(|(earnings, income)| tax_in(year, earnings, income));
    let prior_income = match prior_investment {
        PriorInvestment::Year(year) => Some(income_of(year)),
        PriorInvestment::None => Some(nothing),
        PriorInvestment::Unknown => None,
    };
    let prior_earnings = prior_earnings.map(|income| Earnings {
        wages: income.wages,
        medicare_wages: income.medicare_wages,
        self_employment: income.self_employment,
        self_employment_tax: income.self_employment_tax,
    });
    let prior = prior_earnings.as_ref().zip(prior_income.as_ref());
    let prior_tax = prior.and_then(|(earnings, income)| tax_in(year - 1, earnings, income));
    let prior_agi = prior
        .zip(filing_status)
        .and_then(|((earnings, income), status)| {
            Some(
                earnings.wages + earnings.self_employment - earnings.self_employment_tax / 2.0
                    + investment_agi(income, status)?,
            )
        });
    let prior_percent = match (prior_agi, filing_status) {
        (Some(agi), Some(status))
            if rules
                .high_income_agi
                .get(status)
                .is_some_and(|threshold| agi > *threshold) =>
        {
            rules.high_income_prior_year_percent
        }
        _ => rules.prior_year_percent,
    };
    let harbor = |percent: f64, tax: Option<f64>| {
        let required = tax.map(|tax| round2(tax * percent / 100.0));
        Harbor {
            percent,
            tax,
            required,
            met: required
                .zip(projected_withholding)
                .map(|(required, withheld)| withheld >= required),
        }
    };
    let current_year = harbor(rules.current_year_percent, projected_tax);
    let prior_year = harbor(prior_percent, prior_tax);
    let difference = projected_withholding
        .zip(projected_tax)
        .map(|(withheld, tax)| round2(withheld - tax));
    let met = if current_year.met == Some(true) {
        HarborMet::CurrentYear
    } else if prior_year.met == Some(true) {
        HarborMet::PriorYear
    } else if difference.is_some_and(|difference| -difference < rules.small_balance) {
        HarborMet::SmallBalance
    } else if current_year.met == Some(false) && prior_year.met == Some(false) {
        HarborMet::None
    } else {
        HarborMet::Unknown
    };
    Some(Withholding {
        year,
        basis: projections.unwrap_or_default(),
        uncounted_employers,
        projected_wages,
        projected_withholding,
        filing_status: filing_status.map(str::to_owned),
        projected_tax,
        difference,
        current_year,
        prior_year,
        met,
        from_last_year: still_to_come.is_some(),
    })
}

/// A year's interest and dividends in taxable accounts, in cents.
#[derive(Default)]
struct Earned {
    interest: i64,
    qualified: i64,
    non_qualified: i64,
    /// Dividends the provider didn't classify; estimates bound them both ways.
    unclassified: i64,
    capital_gain: i64,
}

impl Earned {
    fn in_year(
        transactions: &[Transaction],
        taxable: &dyn Fn(&str) -> bool,
        in_year: &dyn Fn(&Transaction, NaiveDate) -> bool,
    ) -> Self {
        let mut earned = Earned::default();
        for transaction in transactions.iter().filter(|transaction| {
            !transaction.pending
                && transaction.amount > 0.0
                && taxable(transaction.account_id.as_deref().unwrap_or_default())
        }) {
            if !calendar::transaction_date(&transaction.date)
                .is_some_and(|date| in_year(transaction, date))
            {
                continue;
            }
            let amount = minor_units(transaction.amount);
            match transaction.kind() {
                Some(Kind::Interest) => earned.interest += amount,
                Some(Kind::Dividend) => match transaction.dividend_type.as_deref() {
                    Some("qualified") => earned.qualified += amount,
                    Some("nonQualified") => earned.non_qualified += amount,
                    Some("capitalGain") => earned.capital_gain += amount,
                    _ => earned.unclassified += amount,
                },
                _ => {}
            }
        }
        earned
    }

    fn dividends(&self) -> Dividends {
        Dividends {
            total: from_minor(self.qualified + self.non_qualified + self.unclassified),
            qualified: from_minor(self.qualified),
            non_qualified: from_minor(self.non_qualified),
            unclassified: from_minor(self.unclassified),
            capital_gain_distributions: from_minor(self.capital_gain),
        }
    }

    /// The year's income with the unclassified dividends counted as qualified or as ordinary.
    fn income(&self, assume_qualified: bool, short_term: i64, long_term: i64) -> Income {
        let (qualified, ordinary) = if assume_qualified {
            (self.unclassified, 0)
        } else {
            (0, self.unclassified)
        };
        Income {
            interest: self.interest,
            qualified: self.qualified + qualified,
            ordinary_dividends: self.non_qualified + ordinary,
            short_term,
            long_term: long_term + self.capital_gain,
        }
    }
}

/// A year's sales in taxable accounts, with wash-sale losses returned to the parts that lost.
/// Also returns the short- and long-term gains in cents.
fn realized(
    trades: &[Trade],
    washes: &[WashSale],
    taxable: &dyn Fn(&str) -> bool,
    in_year: &dyn Fn(NaiveDate) -> bool,
) -> (Realized, i64, i64) {
    let mut realized = Realized::default();
    let (mut short, mut long) = (0_i64, 0_i64);
    for trade in trades.iter().filter(|trade| {
        sale(trade) && taxable(&trade.account_id) && trade_date(trade).is_some_and(in_year)
    }) {
        realized.sales += 1;
        match (
            trade.estimated_short_term_gain,
            trade.estimated_long_term_gain,
        ) {
            (Some(short_term), Some(long_term)) => {
                let (mut short_part, mut long_part) =
                    (minor_units(short_term), minor_units(long_term));
                if let Some(wash) = washes.iter().find(|wash| wash.sale_id == trade.id) {
                    // The disallowed loss comes back to the parts that lost, in proportion.
                    let disallowed = minor_units(wash.disallowed);
                    let losing = short_part.min(0) + long_part.min(0);
                    if losing < 0 {
                        let to_short = disallowed * short_part.min(0) / losing;
                        short_part += to_short;
                        long_part += disallowed - to_short;
                    }
                    realized.wash_disallowed += wash.disallowed;
                }
                short += short_part;
                long += long_part;
            }
            _ => {
                realized.unestimated_sales += 1;
                realized.unestimated_proceeds += trade.amount.abs();
            }
        }
    }
    realized.short_term = from_minor(short);
    realized.long_term = from_minor(long);
    realized.wash_disallowed = round2(realized.wash_disallowed);
    realized.unestimated_proceeds = round2(realized.unestimated_proceeds);
    (realized, short, long)
}

/// The year's tax as a range: the low end counts unclassified dividends as qualified, the high
/// end as ordinary, and the breakdown follows the high end.
fn estimate(
    profile: &TaxProfile,
    tables: &YearTables,
    earned: &Earned,
    short_term: i64,
    long_term: i64,
) -> Option<TaxEstimate> {
    let low = scenario(profile, tables, &earned.income(true, short_term, long_term))?;
    let high = scenario(
        profile,
        tables,
        &earned.income(false, short_term, long_term),
    )?;
    Some(TaxEstimate {
        low: low.total,
        high: high.total,
        ordinary: high.ordinary,
        capital_gains: high.capital_gains,
        niit: high.niit,
        state: high.state,
        marginal_rate: high.marginal_rate,
        capital_gains_rate: high.capital_gains_rate,
        loss_deduction: high.loss_deduction,
        loss_carryover: high.loss_carryover,
    })
}

/// Each harvestable position's unrealized losses by term, what selling would save this year,
/// and when a recent purchase's wash-sale window closes.
fn harvest(
    harvestable: &[(String, String)],
    positions: &[PositionLots],
    trades: &[Trade],
    saving: &dyn Fn(i64, i64) -> Option<f64>,
    today: NaiveDate,
) -> Vec<HarvestEstimate> {
    harvestable
        .iter()
        .filter_map(|(symbol, account_id)| {
            let position = positions.iter().find(|position| {
                &position.ticker == symbol && &position.account_id == account_id
            })?;
            let loss = |long_term: bool| {
                position
                    .lots
                    .iter()
                    .filter(|lot| lot.long_term == long_term)
                    .filter_map(|lot| lot.gain)
                    .filter(|gain| *gain < 0.0)
                    .map(minor_units)
                    .sum::<i64>()
            };
            let (short_term_loss, long_term_loss) = (loss(false), loss(true));
            let latest_purchase = trades
                .iter()
                .filter(|trade| purchase(trade) && ticker(trade).as_ref() == Some(symbol))
                .filter_map(trade_date)
                .filter(|date| *date <= today && (today - *date).num_days() <= WASH_DAYS)
                .max();
            Some(HarvestEstimate {
                ticker: symbol.clone(),
                account_id: account_id.clone(),
                short_term_loss: from_minor(short_term_loss),
                long_term_loss: from_minor(long_term_loss),
                tax_savings: saving(short_term_loss, long_term_loss),
                wash_until: latest_purchase.map(|date| date + Duration::days(WASH_DAYS)),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn trade(
        id: &str,
        kind: &str,
        date: &str,
        account: &str,
        units: f64,
        gain: Option<(f64, f64)>,
    ) -> Trade {
        serde_json::from_value(json!({
            "id": id, "type": kind, "date": date, "amount": units * 100.0, "account": account,
            "accountId": account, "ticker": "TEST", "description": null, "units": units, "price": 100.0,
            "estimatedRealizedGain": gain.map(|(short, long)| short + long),
            "estimatedShortTermGain": gain.map(|(short, _)| short),
            "estimatedLongTermGain": gain.map(|(_, long)| long),
        }))
        .unwrap()
    }

    fn account(id: &str, treatment: &str) -> Account {
        serde_json::from_value(json!({
            "id": id, "name": id, "institution": "Broker", "type": "brokerage", "value": 1.0,
            "taxTreatment": treatment,
        }))
        .unwrap()
    }

    fn income(
        id: &str,
        kind: &str,
        date: &str,
        account: &str,
        amount: f64,
        dividend_type: Option<&str>,
    ) -> Transaction {
        serde_json::from_value(json!({
            "id": id, "merchant": id, "category": "Income", "date": date, "amount": amount,
            "account": account, "accountId": account, "pending": false, "description": null,
            "categorySource": "provider", "dividendType": dividend_type,
            "classification": { "mark": kind, "kind": kind, "spending": false, "credit": false, "zelle": false }
        }))
        .unwrap()
    }

    fn profile(income: f64, state: Option<f64>) -> TaxProfile {
        TaxProfile {
            filing_status: "single".into(),
            taxable_income: income,
            state_rate: state,
        }
    }

    #[test]
    fn tables_reproduce_the_published_tax_at_each_bracket_edge() {
        let single = &TABLES.years[&2026].ordinary["single"];
        // Rev. Proc. 2025-32, Table 3: the tax at the top of each bracket.
        for (income, tax) in [
            (12_400.0, 1_240.0),
            (50_400.0, 5_800.0),
            (105_700.0, 17_966.0),
            (201_775.0, 41_024.0),
            (256_225.0, 58_448.0),
            (640_600.0, 192_979.25),
        ] {
            assert!(
                (ordinary_tax(single, income) - tax).abs() < 0.005,
                "{income}"
            );
        }
        let joint = &TABLES.years[&2025].ordinary["marriedJoint"];
        assert!((ordinary_tax(joint, 751_600.0) - 202_154.50).abs() < 0.005);
        assert_eq!(marginal(single, 105_699.0), 22.0);
        assert_eq!(marginal(single, 105_700.0), 24.0);
        // $10,000 of gains on $45,000 of income: $4,450 at 0%, the rest at 15%.
        assert!(
            (capital_gains_tax((49_450.0, 545_500.0), 45_000.0, 10_000.0) - 832.5).abs() < 0.005
        );
    }

    #[test]
    fn finds_wash_sales_across_accounts_and_leaves_clean_windows_open() {
        let trades = [
            trade("buy", "BUY", "2026-01-05", "taxable", 10.0, None),
            // A loss sale replaced by half as many shares in the IRA 30 days later.
            trade(
                "loss",
                "SELL",
                "2026-06-01",
                "taxable",
                10.0,
                Some((-400.0, 0.0)),
            ),
            trade("ira-buy", "BUY", "2026-07-01", "ira", 5.0, None),
            // Day 31 after the first sale, so not its replacement; but 30 days before the
            // next one, whose FIFO sale leaves three of these shares held: those replace it.
            trade("buy-2", "BUY", "2026-07-02", "taxable", 4.0, None),
            trade(
                "loss-2",
                "SELL",
                "2026-08-01",
                "taxable",
                4.0,
                Some((-120.0, 0.0)),
            ),
            trade("late", "BUY", "2026-09-01", "taxable", 1.0, None),
            trade("buy-3", "BUY", "2026-04-01", "taxable", 3.0, None),
        ];
        let taxable = |id: &str| id != "ira";
        let (washes, windows) = wash_sales(&trades, &taxable, day("2026-09-20"));
        let first = washes.iter().find(|wash| wash.sale_id == "loss").unwrap();
        assert_eq!(first.disallowed, 200.0);
        assert!(first.permanent);
        assert_eq!(first.replacement_trade_ids, ["ira-buy"]);
        // Three of four shares replaced; "late" is day 31 after this sale.
        let second = washes.iter().find(|wash| wash.sale_id == "loss-2").unwrap();
        assert_eq!(second.disallowed, 90.0);
        assert_eq!(second.replacement_trade_ids, ["buy-2"]);
        assert!(!second.permanent);
        assert!(windows.is_empty());

        // A recent loss with no replacement yet leaves its window open.
        let open = [
            trade("buy", "BUY", "2026-04-01", "taxable", 3.0, None),
            trade(
                "loss",
                "SELL",
                "2026-09-10",
                "taxable",
                2.0,
                Some((-30.0, -20.0)),
            ),
        ];
        let (washes, windows) = wash_sales(&open, &taxable, day("2026-09-20"));
        assert!(washes.is_empty());
        assert_eq!(windows.len(), 1);
        assert_eq!(
            (windows[0].ticker.as_str(), windows[0].until),
            ("TEST", day("2026-10-10"))
        );
        assert_eq!(windows[0].loss, -50.0);
    }

    /// Shares replaced by `wash`, from a sale losing $10 a share.
    fn replaced_units(wash: &WashSale) -> f64 {
        wash.disallowed / 10.0
    }

    fn loss(id: &str, date: &str, account: &str, units: f64) -> Trade {
        trade(id, "SELL", date, account, units, Some((-10.0 * units, 0.0)))
    }

    #[test]
    fn one_purchase_replaces_shares_of_only_one_loss_sale() {
        let taxable = |id: &str| id != "ira";
        let today = day("2026-12-31");
        // One 10-share buy near two 10-share loss sales elsewhere: it replaces the first.
        let trades = [
            loss("loss-1", "2026-06-01", "taxable", 10.0),
            trade("buy", "BUY", "2026-06-10", "other", 10.0, None),
            loss("loss-2", "2026-06-20", "taxable", 10.0),
        ];
        let (washes, _) = wash_sales(&trades, &taxable, today);
        assert_eq!(washes.len(), 1);
        assert_eq!(washes[0].sale_id, "loss-1");
        assert_eq!(washes[0].disallowed, 100.0);

        // Six shares bought: the first sale takes them, the second gets the rest of a
        // later buy, and neither disallows more than was bought.
        let trades = [
            loss("loss-1", "2026-06-01", "taxable", 4.0),
            trade("buy", "BUY", "2026-06-10", "other", 6.0, None),
            loss("loss-2", "2026-06-20", "taxable", 10.0),
            trade("buy-2", "BUY", "2026-06-25", "other", 1.0, None),
        ];
        let (washes, _) = wash_sales(&trades, &taxable, today);
        let by_id = |id: &str| washes.iter().find(|wash| wash.sale_id == id).unwrap();
        assert_eq!(by_id("loss-1").disallowed, 40.0);
        assert_eq!(by_id("loss-1").replacement_trade_ids, ["buy"]);
        assert_eq!(by_id("loss-2").disallowed, 30.0);
        assert_eq!(by_id("loss-2").replacement_trade_ids, ["buy", "buy-2"]);
    }

    #[test]
    fn an_ira_purchase_consumed_by_one_sale_leaves_the_next_window_open() {
        let taxable = |id: &str| id != "ira";
        let trades = [
            loss("loss-1", "2026-09-01", "taxable", 5.0),
            trade("ira-buy", "BUY", "2026-09-05", "ira", 5.0, None),
            loss("loss-2", "2026-09-10", "taxable", 5.0),
        ];
        let (washes, windows) = wash_sales(&trades, &taxable, day("2026-09-20"));
        assert_eq!(washes.len(), 1);
        assert_eq!(washes[0].sale_id, "loss-1");
        assert!(washes[0].permanent);
        // No replacement shares are left, but a purchase before the window closes could still
        // make the second sale a wash.
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].sale_id, "loss-2");
        assert_eq!(windows[0].until, day("2026-10-10"));
    }

    #[test]
    fn disallowed_shares_never_exceed_replacement_shares_bought() {
        // A small linear congruential generator keeps the cases reproducible.
        let mut seed = 0x2545_f491_u64;
        let mut next = |bound: u64| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 33) % bound
        };
        let taxable = |id: &str| id != "ira";
        let accounts = ["taxable", "other", "ira"];
        for case in 0..300 {
            let trades = (0..2 + next(8))
                .map(|index| {
                    let date = day("2026-03-01") + Duration::days(next(90) as i64);
                    let date = date.format("%Y-%m-%d").to_string();
                    let account = accounts[next(3) as usize];
                    let units = (1 + next(10)) as f64;
                    let id = format!("{case}-{index}");
                    if next(2) == 0 {
                        trade(&id, "BUY", &date, account, units, None)
                    } else {
                        loss(&id, &date, account, units)
                    }
                })
                .collect::<Vec<_>>();
            let (washes, _) = wash_sales(&trades, &taxable, day("2026-12-31"));
            let disallowed = washes.iter().map(replaced_units).sum::<f64>();
            let bought = trades
                .iter()
                .filter(|trade| purchase(trade))
                .filter_map(|trade| trade.units)
                .sum::<f64>();
            assert!(disallowed <= bought + 1e-9, "case {case}: {trades:?}");
            for wash in &washes {
                let sold = trades
                    .iter()
                    .find(|trade| trade.id == wash.sale_id)
                    .and_then(|trade| trade.units)
                    .unwrap();
                assert!(replaced_units(wash) <= sold + 1e-9, "case {case}");
            }
        }
    }

    #[test]
    fn estimates_a_range_until_dividends_are_classified_and_limits_net_losses() {
        let accounts = [account("taxable", "taxable"), account("roth", "taxFree")];
        let transactions = [
            income(
                "interest",
                "interest",
                "2026-03-31",
                "plaid:savings",
                1_000.0,
                None,
            ),
            income(
                "dividend",
                "dividend",
                "2026-06-30",
                "taxable",
                2_000.0,
                None,
            ),
            income(
                "qualified",
                "dividend",
                "2026-06-30",
                "taxable",
                500.0,
                Some("qualified"),
            ),
            // Income inside a Roth is not taxed.
            income("roth", "dividend", "2026-06-30", "roth", 9_999.0, None),
        ];
        let trades = [
            trade("buy", "BUY", "2025-01-02", "taxable", 10.0, None),
            trade(
                "sale",
                "SELL",
                "2026-05-01",
                "taxable",
                10.0,
                Some((-1_000.0, 4_000.0)),
            ),
        ];
        let result = taxes(
            TaxInputs {
                transactions: &transactions,
                trades: &trades,
                accounts: &accounts,
                positions: &mut [],
                harvestable: &[],
                profile: Some(&profile(80_000.0, Some(5.0))),
            },
            day("2026-09-20"),
        );
        let year = &result.years[0];
        assert_eq!(year.year, 2026);
        assert_eq!(year.interest, 1_000.0);
        assert_eq!(
            year.dividends,
            Dividends {
                total: 2_500.0,
                qualified: 500.0,
                non_qualified: 0.0,
                unclassified: 2_000.0,
                capital_gain_distributions: 0.0,
            }
        );
        assert_eq!(
            (year.realized.short_term, year.realized.long_term),
            (-1_000.0, 4_000.0)
        );
        let estimate = year.estimate.as_ref().unwrap();
        // Interest at 22%; $3,000 net long-term and qualified dividends at 15%; 5% state on
        // everything. Unclassified dividends cost 22% instead of 15% in the high case.
        assert_eq!(estimate.low, 220.0 + 825.0 + 325.0);
        assert_eq!(estimate.high, 220.0 + 440.0 + 525.0 + 325.0);
        assert_eq!(
            (estimate.marginal_rate, estimate.capital_gains_rate),
            (27.0, 20.0)
        );

        let losing = [
            trade("buy", "BUY", "2026-01-02", "taxable", 10.0, None),
            trade(
                "sale",
                "SELL",
                "2026-05-01",
                "taxable",
                10.0,
                Some((-5_000.0, 0.0)),
            ),
        ];
        let result = taxes(
            TaxInputs {
                transactions: &[],
                trades: &losing,
                accounts: &accounts,
                positions: &mut [],
                harvestable: &[],
                profile: Some(&profile(80_000.0, None)),
            },
            day("2026-09-20"),
        );
        let estimate = result.years[0].estimate.as_ref().unwrap();
        assert_eq!(
            (estimate.loss_deduction, estimate.loss_carryover),
            (3_000.0, 2_000.0)
        );
        assert_eq!(estimate.high, -660.0);
        // Without a profile there are amounts and no estimate.
        let unprofiled = taxes(
            TaxInputs {
                transactions: &[],
                trades: &losing,
                accounts: &accounts,
                positions: &mut [],
                harvestable: &[],
                profile: None,
            },
            day("2026-09-20"),
        );
        assert!(unprofiled.years[0].estimate.is_none() && !unprofiled.has_profile);
        assert!(unprofiled.years[0].tables_available);
    }

    #[test]
    fn capital_gain_distributions_are_their_own_line_and_net_against_losses() {
        let transactions = [income(
            "distribution",
            "dividend",
            "2026-06-30",
            "taxable",
            2_000.0,
            Some("capitalGain"),
        )];
        let trades = [trade(
            "sale",
            "SELL",
            "2026-05-01",
            "taxable",
            10.0,
            Some((-5_000.0, 0.0)),
        )];
        let result = taxes(
            TaxInputs {
                transactions: &transactions,
                trades: &trades,
                accounts: &[account("taxable", "taxable")],
                positions: &mut [],
                harvestable: &[],
                profile: Some(&profile(80_000.0, None)),
            },
            day("2026-09-20"),
        );
        let year = &result.years[0];
        // 1099-DIV box 2a, not box 1a's dividends.
        assert_eq!(year.dividends.capital_gain_distributions, 2_000.0);
        assert_eq!((year.dividends.total, year.dividends.qualified), (0.0, 0.0));
        // Long-term gain on Schedule D: the $5,000 loss nets to $3,000, all deducted.
        let estimate = year.estimate.as_ref().unwrap();
        assert_eq!(
            (estimate.loss_deduction, estimate.loss_carryover),
            (3_000.0, 0.0)
        );
        assert_eq!(estimate.high, -660.0);
    }

    /// This year's estimate for gains alone on `base` of taxable income, single, no state tax.
    fn estimate_for(base: f64, short: f64, long: f64) -> TaxEstimate {
        let trades = [trade(
            "sale",
            "SELL",
            "2026-05-01",
            "taxable",
            10.0,
            Some((short, long)),
        )];
        taxes(
            TaxInputs {
                transactions: &[],
                trades: &trades,
                accounts: &[account("taxable", "taxable")],
                positions: &mut [],
                harvestable: &[],
                profile: Some(&profile(base, None)),
            },
            day("2026-09-20"),
        )
        .years[0]
            .estimate
            .clone()
            .unwrap()
    }

    #[test]
    fn long_term_gains_stack_on_ordinary_income_across_the_2026_bands() {
        // The 0% band for a single filer ends at $49,450; the 15% band at $545,500.
        let filling = estimate_for(40_000.0, 0.0, 9_450.0);
        assert_eq!((filling.capital_gains, filling.ordinary), (0.0, 0.0));
        // The next dollar is past the band.
        assert_eq!(filling.capital_gains_rate, 15.0);
        let crossing = estimate_for(40_000.0, 0.0, 10_000.0);
        assert_eq!(crossing.capital_gains, 82.5);
        assert_eq!(crossing.low, 82.5);

        // Short-term gains are ordinary income, so they raise where the long-term stack starts.
        let both = estimate_for(40_000.0, 9_000.0, 1_000.0);
        assert_eq!(both.ordinary, 1_080.0, "$9,000 at 12%");
        assert_eq!(both.capital_gains, 82.5, "$450 at 0%, $550 at 15%");
        assert_eq!(both.marginal_rate, 12.0);
        assert_eq!(both.high, 1_162.5);

        // Across the 20% band, where the investment income tax applies too.
        let top = estimate_for(540_000.0, 0.0, 10_000.0);
        assert_eq!(top.capital_gains, 5_500.0 * 0.15 + 4_500.0 * 0.20);
        assert_eq!(top.niit, 380.0);
        assert_eq!(top.capital_gains_rate, 23.8);
        assert_eq!(top.marginal_rate, 38.8, "35% bracket and 3.8%");
        assert_eq!(top.high, 825.0 + 900.0 + 380.0);

        // A long-term loss nets against a short-term gain, which stays ordinary.
        let netted = estimate_for(80_000.0, 5_000.0, -2_000.0);
        assert_eq!((netted.ordinary, netted.capital_gains), (660.0, 0.0));
        assert_eq!((netted.loss_deduction, netted.loss_carryover), (0.0, 0.0));
        // A net loss deducts at most $3,000; the rest carries over.
        let loss = estimate_for(80_000.0, -1_000.0, -2_500.0);
        assert_eq!((loss.loss_deduction, loss.loss_carryover), (3_000.0, 500.0));
    }

    #[test]
    fn the_investment_income_tax_tests_income_before_the_standard_deduction() {
        // $190,000 taxable plus the $16,100 deduction and $10,000 of gains is over $200,000.
        let near = estimate_for(190_000.0, 10_000.0, 0.0);
        assert_eq!(near.niit, 380.0);
        assert_eq!(near.marginal_rate, 24.0 + 3.8);
        // Under the line, none is due.
        assert_eq!(estimate_for(170_000.0, 10_000.0, 0.0).niit, 0.0);
    }

    #[test]
    fn a_deductible_capital_loss_reduces_net_investment_income() {
        // $10,000 of interest less the $3,000 loss deducted is $7,000 of net investment income,
        // all of it over the line.
        let transactions = [income(
            "interest",
            "interest",
            "2026-03-31",
            "taxable",
            10_000.0,
            None,
        )];
        let trades = [trade(
            "sale",
            "SELL",
            "2026-05-01",
            "taxable",
            10.0,
            Some((-5_000.0, 0.0)),
        )];
        let estimate = taxes(
            TaxInputs {
                transactions: &transactions,
                trades: &trades,
                accounts: &[account("taxable", "taxable")],
                positions: &mut [],
                harvestable: &[],
                profile: Some(&profile(400_000.0, None)),
            },
            day("2026-09-20"),
        )
        .years[0]
            .estimate
            .clone()
            .unwrap();
        assert_eq!(estimate.niit, 266.0);
    }

    fn position(ticker: &str, account: &str, gains: &[(f64, bool)]) -> PositionLots {
        PositionLots {
            account_id: account.into(),
            ticker: ticker.into(),
            coverage: "complete",
            untracked_units: 0.0,
            lots: gains
                .iter()
                .map(|(gain, long_term)| super::super::lots::Lot {
                    acquired: day("2025-01-02"),
                    units: 1.0,
                    cost: 100.0,
                    value: Some(100.0 + gain),
                    gain: Some(*gain),
                    long_term: *long_term,
                    long_term_on: day("2026-01-03"),
                    estimated_tax: None,
                })
                .collect(),
            short_term_gain: None,
            long_term_gain: None,
            money_weighted_pct: None,
            since: day("2025-01-02"),
            tax_deferred: false,
        }
    }

    #[test]
    fn harvest_values_losses_by_term_against_this_years_tax_and_names_the_wash_window() {
        let positions = [
            // Gains never offset a lot's loss: only the losing lots count.
            position(
                "TEST",
                "taxable",
                &[(-150.25, false), (40.0, false), (-300.0, true)],
            ),
            position("OTHER", "taxable", &[(-20.0, true)]),
        ];
        let trades = [
            trade("old", "BUY", "2026-07-01", "taxable", 1.0, None),
            trade("recent", "BUY", "2026-09-01", "ira", 1.0, None),
        ];
        let harvestable = [
            ("TEST".to_owned(), "taxable".to_owned()),
            ("OTHER".to_owned(), "taxable".to_owned()),
            // A pair with no saved lots is left out, not shown as zero.
            ("GONE".to_owned(), "taxable".to_owned()),
        ];
        let run = |profile, today| {
            taxes(
                TaxInputs {
                    transactions: &[],
                    trades: &trades,
                    accounts: &[],
                    positions: &mut positions.clone(),
                    harvestable: &harvestable,
                    profile,
                },
                today,
            )
        };
        // $80,000 single, with 5% state tax.
        let with_profile = profile(80_000.0, Some(5.0));
        let result = run(Some(&with_profile), day("2026-09-20"));
        assert_eq!(result.harvest.len(), 2);
        let test = &result.harvest[0];
        assert_eq!(
            (test.short_term_loss, test.long_term_loss),
            (-150.25, -300.0)
        );
        // With no gains this year, the whole loss comes off ordinary income at 22%. State tax
        // here is on investment income only, which has none to reduce.
        assert_eq!(test.tax_savings, Some(99.06));
        // Bought 19 days ago in any account: a sale before Oct 1 would be a wash sale.
        assert_eq!(test.wash_until, Some(day("2026-10-01")));
        let other = &result.harvest[1];
        assert_eq!(other.wash_until, None, "TEST's purchases don't touch OTHER");
        // Without a profile the losses are listed and no saving is guessed.
        let unprofiled = run(None, day("2026-09-20"));
        assert!(unprofiled
            .harvest
            .iter()
            .all(|harvest| harvest.tax_savings.is_none()));
        // Thirty-one days on, the window has closed.
        let later = run(None, day("2026-10-02"));
        assert_eq!(later.harvest[0].wash_until, None);
    }

    #[test]
    fn a_harvest_saves_only_what_this_years_gains_and_loss_limit_allow() {
        let positions = [
            position("TEST", "taxable", &[(-20_000.0, false)]),
            position("OTHER", "taxable", &[(-20_000.0, true)]),
        ];
        let harvestable = [
            ("TEST".to_owned(), "taxable".to_owned()),
            ("OTHER".to_owned(), "taxable".to_owned()),
        ];
        let savings = |trades: &[Trade]| {
            taxes(
                TaxInputs {
                    transactions: &[],
                    trades,
                    accounts: &[],
                    positions: &mut positions.clone(),
                    harvestable: &harvestable,
                    profile: Some(&profile(80_000.0, None)),
                },
                day("2026-09-20"),
            )
            .harvest
            .iter()
            .map(|harvest| harvest.tax_savings)
            .collect::<Vec<_>>()
        };
        // No gains: $3,000 of either loss comes off ordinary income at 22%; the rest carries
        // to later years and saves nothing this year.
        assert_eq!(savings(&[]), [Some(660.0), Some(660.0)]);
        // A $15,000 short-term gain is offset first, then $3,000 more.
        let gain = [trade(
            "gain",
            "SELL",
            "2026-05-01",
            "taxable",
            10.0,
            Some((15_000.0, 0.0)),
        )];
        assert_eq!(savings(&gain), [Some(3_960.0), Some(3_960.0)]);
    }

    #[test]
    fn each_taxable_lot_carries_what_selling_it_alone_would_change_this_year() {
        let mut deferred = position("IRA", "ira", &[(5_000.0, true)]);
        deferred.tax_deferred = true;
        let mut positions = [
            position("TEST", "taxable", &[(-20_000.0, false), (1_000.0, true)]),
            deferred,
        ];
        let run = |positions: &mut [PositionLots], profile: Option<&TaxProfile>| {
            taxes(
                TaxInputs {
                    transactions: &[],
                    trades: &[],
                    accounts: &[],
                    positions,
                    harvestable: &[],
                    profile,
                },
                day("2026-09-20"),
            );
        };
        run(&mut positions, Some(&profile(80_000.0, None)));
        let estimated = |position: &PositionLots| {
            position
                .lots
                .iter()
                .map(|lot| lot.estimated_tax)
                .collect::<Vec<_>>()
        };
        // The loss saves only $3,000 at 22%; the gain adds 15%. A tax-deferred lot owes nothing
        // on a sale.
        assert_eq!(estimated(&positions[0]), [Some(-660.0), Some(150.0)]);
        assert_eq!(estimated(&positions[1]), [None]);
        // Without a profile no tax is guessed.
        let mut unprofiled = positions.clone();
        for position in &mut unprofiled {
            for lot in &mut position.lots {
                lot.estimated_tax = None;
            }
        }
        run(&mut unprofiled, None);
        assert_eq!(estimated(&unprofiled[0]), [None, None]);
    }

    #[test]
    fn each_year_reports_its_own_taxable_interest_dividends_and_sales() {
        let accounts = [account("taxable", "taxable"), account("ira", "taxDeferred")];
        let mut pending = income("pending", "interest", "2026-02-01", "taxable", 9.0, None);
        pending.pending = true;
        let transactions = [
            income("old", "interest", "2024-12-31", "taxable", 7.0, None),
            income("last-1", "interest", "2025-03-01", "taxable", 10.5, None),
            income(
                "last-2",
                "dividend",
                "2025-12-31",
                "taxable",
                20.0,
                Some("qualified"),
            ),
            income("this-1", "interest", "2026-01-01", "taxable", 1.25, None),
            income(
                "this-2",
                "dividend",
                "2026-06-30",
                "taxable",
                30.0,
                Some("nonQualified"),
            ),
            income("ira", "dividend", "2026-06-30", "ira", 99.0, None),
            // After today: not on this year's 1099 yet.
            income("later", "interest", "2026-12-31", "taxable", 5.0, None),
            pending,
        ];
        let trades = [
            trade(
                "sold-2025",
                "SELL",
                "2025-11-03",
                "taxable",
                1.0,
                Some((25.0, 0.0)),
            ),
            trade(
                "sold-2026",
                "SELL",
                "2026-04-03",
                "taxable",
                1.0,
                Some((0.0, -40.0)),
            ),
            trade(
                "ira-sale",
                "SELL",
                "2026-04-03",
                "ira",
                1.0,
                Some((500.0, 0.0)),
            ),
            trade("unknown", "SELL", "2026-05-03", "taxable", 2.0, None),
        ];
        let result = taxes(
            TaxInputs {
                transactions: &transactions,
                trades: &trades,
                accounts: &accounts,
                positions: &mut [],
                harvestable: &[],
                profile: None,
            },
            day("2026-09-20"),
        );
        let years = result
            .years
            .iter()
            .map(|year| year.year)
            .collect::<Vec<_>>();
        assert_eq!(years, [2026, 2025], "this year first; 2024 has been filed");
        let (this, last) = (&result.years[0], &result.years[1]);
        assert_eq!(this.interest, 1.25);
        assert_eq!(
            (this.dividends.total, this.dividends.non_qualified),
            (30.0, 30.0)
        );
        assert_eq!(
            (this.realized.short_term, this.realized.long_term),
            (0.0, -40.0)
        );
        assert_eq!(this.realized.sales, 2);
        assert_eq!(
            (
                this.realized.unestimated_sales,
                this.realized.unestimated_proceeds
            ),
            (1, 200.0)
        );
        assert_eq!(last.interest, 10.5);
        assert_eq!(
            (last.dividends.total, last.dividends.qualified),
            (20.0, 20.0)
        );
        assert_eq!(last.realized.short_term, 25.0);
        // Activity starts in 2024, so neither year is missing its start.
        assert!(!this.partial && !last.partial);
        assert!(this.tables_available && last.tables_available);
    }

    fn projection(wages: f64, withholding: f64) -> super::super::documents::PayProjection {
        super::super::documents::PayProjection {
            employer: "Example Employer".into(),
            document_id: "a".repeat(64),
            date: day("2026-09-30"),
            period: 18,
            periods: 24,
            wages_to_date: 0.0,
            withheld_to_date: 0.0,
            projected_wages: wages,
            projected_withholding: withholding,
            projected_medicare_wages: wages,
        }
    }

    /// Last year's income from W-2s alone.
    fn w2(wages: f64) -> super::super::documents::Income {
        super::super::documents::Income {
            wages,
            medicare_wages: wages,
            wages_from: "w2",
            uncounted_employers: Vec::new(),
            self_employment: 0.0,
            self_employment_later: 0.0,
            self_employment_tax: 0.0,
            taxable_income: None,
        }
    }

    fn quiet_year() -> TaxYear {
        TaxYear {
            year: 2026,
            interest: 0.0,
            dividends: Dividends::default(),
            realized: Realized::default(),
            wash_sales: Vec::new(),
            estimate: None,
            tables_available: true,
            partial: false,
            still_to_come: None,
        }
    }

    fn project(
        wages: f64,
        withholding: f64,
        prior_wages: Option<f64>,
        status: Option<&str>,
    ) -> Withholding {
        super::withholding(
            day("2026-09-30"),
            Some(vec![projection(wages, withholding)]),
            Vec::new(),
            Some(&quiet_year()),
            (0.0, 0.0),
            prior_wages.map(w2).as_ref(),
            PriorInvestment::None,
            status,
        )
        .unwrap()
    }

    #[test]
    fn last_years_investment_income_counts_toward_its_tax_and_agi() {
        // $140,000 of wages alone is under the $150,000 line; with $30,000 of dividends it isn't.
        let mut prior = quiet_year();
        prior.year = 2025;
        prior.dividends.qualified = 30_000.0;
        prior.dividends.total = 30_000.0;
        let with = |investment| {
            super::withholding(
                day("2026-09-30"),
                Some(vec![projection(150_000.0, 1_000.0)]),
                Vec::new(),
                Some(&quiet_year()),
                (0.0, 0.0),
                Some(&w2(140_000.0)),
                investment,
                Some("single"),
            )
            .unwrap()
        };
        let wages_only = with(PriorInvestment::None);
        let dividends = with(PriorInvestment::Year(&prior));
        assert_eq!(wages_only.prior_year.percent, 100.0);
        assert_eq!(dividends.prior_year.percent, 110.0);
        assert!(dividends.prior_year.tax > wages_only.prior_year.tax);
        // Saved history that doesn't reach all of last year can't settle the test.
        let unknown = with(PriorInvestment::Unknown);
        assert_eq!(
            (unknown.prior_year.tax, unknown.prior_year.met),
            (None, None)
        );
    }

    #[test]
    fn a_separate_filers_net_loss_counts_only_1500_toward_last_years_agi() {
        // $77,000 less a $1,500 loss is over the $75,000 line; less $3,000 it wouldn't be.
        let mut prior = quiet_year();
        prior.year = 2025;
        prior.realized.short_term = -3_000.0;
        let result = super::withholding(
            day("2026-09-30"),
            Some(vec![projection(80_000.0, 1_000.0)]),
            Vec::new(),
            Some(&quiet_year()),
            (0.0, 0.0),
            Some(&w2(77_000.0)),
            PriorInvestment::Year(&prior),
            Some("marriedSeparate"),
        )
        .unwrap();
        assert_eq!(result.prior_year.percent, 110.0);
    }

    #[test]
    fn withholding_meets_90_percent_of_tax_at_a_bracket_boundary() {
        // $66,500 less the $16,100 standard deduction is $50,400, the top of the 12% bracket:
        // $1,240 at 10% plus $4,560 at 12%.
        let at_top = project(66_500.0, 5_220.0, None, Some("single"));
        assert_eq!(at_top.projected_tax, Some(5_800.0));
        assert_eq!(at_top.current_year.required, Some(5_220.0));
        assert_eq!(at_top.current_year.met, Some(true));
        assert_eq!(at_top.met, HarborMet::CurrentYear);
        assert_eq!(at_top.difference, Some(-580.0));
        // One dollar more is taxed at 22%.
        let over = project(66_501.0, 5_220.0, None, Some("single"));
        assert_eq!(over.projected_tax, Some(5_800.22));
        assert_eq!(over.current_year.met, Some(false));
        // Short of 90% but owing under $1,000: the small-balance rule.
        assert_eq!(over.met, HarborMet::SmallBalance);
        // Wages under the standard deduction owe nothing.
        assert_eq!(
            project(16_000.0, 0.0, None, Some("single")).projected_tax,
            Some(0.0)
        );
    }

    #[test]
    fn the_rest_of_the_year_is_what_last_year_paid_after_this_day() {
        // A dividend paid January 3 counts once; last year's December distribution and
        // interest after January 3 are what is still to come, and the gain counts once.
        let mut so_far = quiet_year();
        so_far.dividends.non_qualified = 1_000.0;
        so_far.dividends.total = 1_000.0;
        so_far.realized.short_term = 1_000.0;
        let run = |year: &TaxYear, contractor: (f64, f64)| {
            super::withholding(
                day("2026-01-03"),
                Some(vec![projection(66_500.0, 0.0)]),
                Vec::new(),
                Some(year),
                contractor,
                None,
                PriorInvestment::None,
                Some("single"),
            )
            .unwrap()
        };
        // Without last year's history, only what has come so far counts.
        let unknown = run(&so_far, (500.0, 4_000.0));
        assert!(!unknown.from_last_year);
        let so_far_only = 5_800.0 + 2_000.0 * 0.22;
        let contractor = |pay: f64| {
            let tax = self_employment_tax(2026, pay, 66_500.0);
            tax + (pay - tax / 2.0) * 0.22
        };
        assert!((unknown.projected_tax.unwrap() - so_far_only - contractor(500.0)).abs() < 0.02);

        let mut known = so_far.clone();
        known.still_to_come = Some(StillToCome {
            interest: 3_000.0,
            dividends: Dividends {
                capital_gain_distributions: 2_000.0,
                ..Dividends::default()
            },
        });
        let result = run(&known, (500.0, 4_000.0));
        assert!(result.from_last_year);
        // $3,000 more interest at 22% and a $2,000 long-term distribution at 15%.
        let expected = so_far_only + 3_000.0 * 0.22 + 2_000.0 * 0.15 + contractor(4_500.0);
        assert!((result.projected_tax.unwrap() - expected).abs() < 0.02);
    }

    #[test]
    fn still_to_come_is_last_years_income_after_this_day_when_history_covers_it() {
        let accounts = [account("taxable", "taxable"), account("ira", "taxDeferred")];
        let mut transactions = vec![
            income("before", "interest", "2025-03-01", "taxable", 10.0, None),
            income("after", "interest", "2025-10-01", "taxable", 20.0, None),
            income(
                "distribution",
                "dividend",
                "2025-12-15",
                "taxable",
                500.0,
                Some("capitalGain"),
            ),
            income("ira", "dividend", "2025-12-15", "ira", 900.0, None),
            income("this", "interest", "2026-03-01", "taxable", 30.0, None),
        ];
        let run = |transactions: &[Transaction]| {
            taxes(
                TaxInputs {
                    transactions,
                    trades: &[],
                    accounts: &accounts,
                    positions: &mut [],
                    harvestable: &[],
                    profile: None,
                },
                day("2026-09-20"),
            )
            .years[0]
                .still_to_come
                .clone()
        };
        // History starts in March of last year, so last year can't stand in.
        assert_eq!(run(&transactions), None);
        transactions.push(income(
            "old",
            "interest",
            "2024-12-31",
            "taxable",
            1.0,
            None,
        ));
        let later = run(&transactions).unwrap();
        assert_eq!(later.interest, 20.0);
        assert_eq!(later.dividends.capital_gain_distributions, 500.0);
        assert_eq!(later.dividends.total, 0.0);
    }

    #[test]
    fn a_payment_landing_a_few_days_off_its_anniversary_counts_once() {
        let accounts = [account("taxable", "taxable")];
        let transactions = [
            income("old", "interest", "2024-12-31", "taxable", 1.0, None),
            // Paid two days after this day last year, and two days before it this year.
            income("early", "interest", "2025-09-22", "taxable", 20.0, None),
            Transaction {
                id: "early-now".into(),
                ..income("early", "interest", "2026-09-18", "taxable", 20.0, None)
            },
            // Paid two days before this day last year, and not yet this year.
            income("late", "dividend", "2025-09-18", "taxable", 50.0, None),
        ];
        let later = taxes(
            TaxInputs {
                transactions: &transactions,
                trades: &[],
                accounts: &accounts,
                positions: &mut [],
                harvestable: &[],
                profile: None,
            },
            day("2026-09-20"),
        )
        .years[0]
            .still_to_come
            .clone()
            .unwrap();
        assert_eq!(later.interest, 0.0);
        assert_eq!(later.dividends.unclassified, 50.0);
    }

    #[test]
    fn wages_over_the_line_and_contractor_pay_add_medicare_and_self_employment_tax() {
        let pay = |medicare_wages: f64| super::super::documents::PayProjection {
            projected_medicare_wages: medicare_wages,
            ..projection(200_000.0, 40_000.0)
        };
        let run = |status: &str, medicare_wages: f64, self_employment: f64| {
            super::withholding(
                day("2026-12-31"),
                Some(vec![pay(medicare_wages)]),
                Vec::new(),
                Some(&quiet_year()),
                (self_employment, 0.0),
                None,
                PriorInvestment::None,
                Some(status),
            )
            .unwrap()
        };
        let base = run("single", 200_000.0, 0.0);
        // $20,000 of deferrals over the single line: 0.9%, which the employer withholds too.
        let over = run("single", 220_000.0, 0.0);
        assert_eq!(
            over.projected_tax,
            base.projected_tax.map(|tax| tax + 180.0)
        );
        assert_eq!(over.projected_withholding, Some(40_180.0));
        // A joint return's line is $250,000, and the employer still withholds above $200,000.
        let joint = run("marriedJoint", 220_000.0, 0.0);
        let joint_base = run("marriedJoint", 200_000.0, 0.0);
        assert_eq!(joint.projected_tax, joint_base.projected_tax);
        assert_eq!(joint.projected_withholding, Some(40_180.0));
        // $10,000 of contractor pay: self-employment tax's Medicare part only, since wages are
        // past the wage base, 0.9% of its $9,235 of earnings, and income tax at 24% on it
        // less half the self-employment tax.
        let contractor = run("single", 200_000.0, 10_000.0);
        let added = 9_235.0 * 0.029 + 9_235.0 * 0.009 + (10_000.0 - 9_235.0 * 0.029 / 2.0) * 0.24;
        let difference = contractor.projected_tax.unwrap() - base.projected_tax.unwrap();
        assert!((difference - added).abs() < 0.02, "{difference} vs {added}");

        // Last year's tax counts its contractor pay the same way.
        let mut prior = w2(150_000.0);
        prior.self_employment = 10_000.0;
        prior.self_employment_tax = self_employment_tax(2025, 10_000.0, 150_000.0);
        let with = |prior: &super::super::documents::Income| {
            super::withholding(
                day("2026-09-30"),
                Some(vec![projection(150_000.0, 1_000.0)]),
                Vec::new(),
                Some(&quiet_year()),
                (0.0, 0.0),
                Some(prior),
                PriorInvestment::None,
                Some("single"),
            )
            .unwrap()
            .prior_year
            .tax
            .unwrap()
        };
        let added = prior.self_employment_tax + (10_000.0 - prior.self_employment_tax / 2.0) * 0.24;
        assert!((with(&prior) - with(&w2(150_000.0)) - added).abs() < 0.02);
    }

    #[test]
    fn deduction_wages_leave_unused_shelters_investment_income() {
        // $10,000 of wages and $8,000 of interest less $16,100 leaves $1,900 at 10%.
        let mut interest = quiet_year();
        interest.interest = 8_000.0;
        let result = super::withholding(
            day("2026-12-31"),
            Some(vec![projection(10_000.0, 0.0)]),
            Vec::new(),
            Some(&interest),
            (0.0, 0.0),
            None,
            PriorInvestment::None,
            Some("single"),
        )
        .unwrap();
        assert_eq!(result.projected_tax, Some(190.0));
        // The deduction left after ordinary income shelters qualified dividends next.
        let mut qualified = quiet_year();
        qualified.dividends.qualified = 600_000.0;
        let result = super::withholding(
            day("2026-12-31"),
            Some(vec![projection(10_000.0, 0.0)]),
            Vec::new(),
            Some(&qualified),
            (0.0, 0.0),
            None,
            PriorInvestment::None,
            Some("single"),
        )
        .unwrap();
        // $593,900 taxable: $49,450 at 0%, $496,050 at 15%, $48,400 at 20%, and 3.8% of the
        // $410,000 over the line.
        assert_eq!(
            result.projected_tax,
            Some(round2(
                496_050.0 * 0.15 + 48_400.0 * 0.20 + 410_000.0 * 0.038
            ))
        );
    }

    #[test]
    fn last_years_w2_wages_set_the_prior_year_test_and_its_percent() {
        // 2025: $64,225 less $15,750 is $48,475, the 12% bracket's top: $1,192.50 + $4,386.
        let short = project(150_000.0, 6_000.0, Some(64_225.0), Some("single"));
        assert_eq!(short.prior_year.tax, Some(5_578.5));
        assert_eq!(short.prior_year.percent, 100.0);
        assert_eq!(short.prior_year.met, Some(true));
        assert_eq!(short.met, HarborMet::PriorYear);
        // Above $150,000 of last year's income, 110%.
        let high = project(250_000.0, 1_000.0, Some(200_000.0), Some("single"));
        assert_eq!(high.prior_year.percent, 110.0);
        assert_eq!(high.met, HarborMet::None);
    }

    #[test]
    fn each_missing_figure_leaves_its_test_unavailable() {
        // Without last year's W-2s the prior-year test can't run, so a shortfall is unknown.
        let unknown = project(150_000.0, 1_000.0, None, Some("single"));
        assert_eq!(
            (unknown.prior_year.tax, unknown.prior_year.met),
            (None, None)
        );
        assert_eq!(unknown.met, HarborMet::Unknown);
        // Without a filing status there is no tax, so no difference and no current-year test.
        let no_status = project(66_500.0, 5_220.0, Some(64_225.0), None);
        assert_eq!(no_status.filing_status, None);
        assert_eq!(
            (no_status.projected_tax, no_status.difference),
            (None, None)
        );
        assert_eq!(no_status.current_year.met, None);
        assert_eq!(no_status.met, HarborMet::Unknown);
        // A statement that can't be carried forward leaves wages and withholding unknown.
        let unread = super::withholding(
            day("2026-09-30"),
            None,
            Vec::new(),
            Some(&quiet_year()),
            (0.0, 0.0),
            None,
            PriorInvestment::None,
            Some("single"),
        )
        .unwrap();
        assert_eq!(
            (
                unread.projected_wages,
                unread.projected_withholding,
                unread.projected_tax
            ),
            (None, None, None)
        );
        assert!(unread.basis.is_empty());
        // No pay statement this year: no projection, and the typed income stays the basis.
        assert_eq!(
            super::withholding(
                day("2026-09-30"),
                Some(Vec::new()),
                Vec::new(),
                Some(&quiet_year()),
                (0.0, 0.0),
                None,
                PriorInvestment::None,
                Some("single")
            ),
            None
        );
        // A year without tables has no tax.
        let untabled = super::withholding(
            day("2030-09-30"),
            Some(vec![projection(66_500.0, 5_220.0)]),
            Vec::new(),
            Some(&quiet_year()),
            (0.0, 0.0),
            None,
            PriorInvestment::None,
            Some("single"),
        )
        .unwrap();
        assert_eq!(untabled.projected_tax, None);
    }
}

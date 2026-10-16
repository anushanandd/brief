//! Taxes: what the year's 1099s will roughly say, and what it may cost. Brief sees interest,
//! dividends, and sales in taxable accounts; it never sees wages, so a tax estimate stacks
//! these on the taxable income the user enters and says it is an estimate. Without that
//! income, amounts are reported and no tax is guessed.
//!
//! Gains come from the first-in-first-out estimate on saved trades. A loss sale is a wash
//! sale when the same ticker is bought in any account within 30 days either side; a
//! replacement in a tax-advantaged account makes the loss permanent. Unclassified dividends
//! make the estimate a range, from all qualified to all ordinary. Rates come from
//! `tax-tables.json`; a year without tables has no estimate. Amounts sum in whole cents.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

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
    years: BTreeMap<i32, YearTables>,
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

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Dividends {
    pub total: f64,
    pub qualified: f64,
    pub non_qualified: f64,
    /// Dividends whose provider didn't say which; the estimate is a range because of them.
    pub unclassified: f64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaxYear {
    pub year: i32,
    pub interest: f64,
    pub dividends: Dividends,
    pub realized: Realized,
    pub wash_sales: Vec<WashSale>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate: Option<TaxEstimate>,
    pub tables_available: bool,
    /// Saved activity starts after January 1, so the year's amounts are incomplete.
    pub partial: bool,
}

/// A taxable position below cost, with what selling it might save.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HarvestEstimate {
    pub ticker: String,
    pub account_id: String,
    /// Unrealized losses on lots held a year or less, and longer, negative.
    pub short_term_loss: f64,
    pub long_term_loss: f64,
    /// At this year's marginal rates, when there is a tax profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_savings: Option<f64>,
    /// A purchase in the last 30 days would make a sale before this day a wash sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wash_until: Option<NaiveDate>,
}

/// A recent loss sale whose window is still open: buying the ticker on or before `until`
/// would disallow the loss.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WashWindow {
    pub sale_id: String,
    pub ticker: String,
    pub account_id: String,
    pub sold_on: NaiveDate,
    pub until: NaiveDate,
    pub loss: f64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Taxes {
    /// This year first.
    pub years: Vec<TaxYear>,
    pub harvest: Vec<HarvestEstimate>,
    pub wash_windows: Vec<WashWindow>,
    pub has_profile: bool,
    pub tables_version: String,
    pub sources: Vec<String>,
}

/// Marginal rates for one year, for estimating the tax on a sale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    /// Percent, including the investment income tax and state tax where they apply.
    pub short_term: f64,
    pub long_term: f64,
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
    let ordinary_income = (base + ordinary_extra).max(0.0);
    let preferential = from_minor(income.qualified) + preferential_gain;
    let ordinary = ordinary_tax(brackets, ordinary_income) - ordinary_tax(brackets, base);
    let capital_gains = capital_gains_tax(bands, ordinary_income, preferential);
    let investment_income = interest + dividends + ordinary_gain + preferential_gain;
    let niit = TABLES.niit_rate / 100.0
        * investment_income.min((ordinary_income + preferential - threshold).max(0.0));
    let state_rate = profile.state_rate.unwrap_or(0.0);
    let state = state_rate / 100.0
        * (interest + dividends + ordinary_gain + preferential_gain - deduction).max(0.0);
    let over_threshold = ordinary_income + preferential > threshold;
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
        let (mut same_before, mut elsewhere) = (0.0, 0.0);
        let mut replacements = Vec::new();
        let mut permanent = false;
        for (other_date, other, candidate) in &ordered {
            if other != symbol
                || candidate.id == trade.id
                || !purchase(candidate)
                || (*other_date - *date).num_days().abs() > WASH_DAYS
            {
                continue;
            }
            let units = candidate.units.map(f64::abs).unwrap_or(0.0);
            if units <= 0.0 {
                continue;
            }
            if candidate.account_id == trade.account_id && other_date <= date {
                same_before += units;
            } else {
                elsewhere += units;
            }
            permanent |= !taxable(&candidate.account_id);
            replacements.push(candidate.id.clone());
        }
        let replaced = (elsewhere + same_before.min(remaining)).min(sold);
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
    pub positions: &'a [PositionLots],
    /// Taxable losses large enough to harvest, by ticker and account.
    pub harvestable: &'a [(String, String)],
    pub profile: Option<&'a TaxProfile>,
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
    for year in years {
        let in_year = |date: NaiveDate| date.year() == year && date <= today;
        let earned = Earned::in_year(transactions, &taxable, &in_year);
        let (realized, short, long) = realized(trades, &washes, &taxable, &in_year);
        let tables = TABLES.years.get(&year);
        let estimate = profile
            .zip(tables)
            .and_then(|(profile, tables)| estimate(profile, tables, &earned, short, long));
        results.push(TaxYear {
            year,
            interest: from_minor(earned.interest),
            dividends: Dividends {
                total: from_minor(earned.qualified + earned.non_qualified + earned.unclassified),
                qualified: from_minor(earned.qualified),
                non_qualified: from_minor(earned.non_qualified),
                unclassified: from_minor(earned.unclassified),
            },
            realized,
            wash_sales: washes
                .iter()
                .filter(|wash| wash.date.year() == year)
                .cloned()
                .collect(),
            estimate,
            tables_available: tables.is_some(),
            partial: first_activity
                .is_none_or(|first| first > NaiveDate::from_ymd_opt(year, 1, 1).unwrap_or(first)),
        });
    }

    let rates = this_year_rates(&results);
    let harvest = harvest(harvestable, positions, trades, rates, today);

    Taxes {
        years: results,
        harvest,
        wash_windows,
        has_profile: profile.is_some(),
        tables_version: TABLES.version.clone(),
        sources: TABLES.sources.clone(),
    }
}

/// A year's interest and dividends in taxable accounts, in cents.
#[derive(Default)]
struct Earned {
    interest: i64,
    qualified: i64,
    non_qualified: i64,
    /// Dividends the provider didn't classify; estimates bound them both ways.
    unclassified: i64,
}

impl Earned {
    fn in_year(
        transactions: &[Transaction],
        taxable: &dyn Fn(&str) -> bool,
        in_year: &dyn Fn(NaiveDate) -> bool,
    ) -> Self {
        let mut earned = Earned::default();
        for transaction in transactions.iter().filter(|transaction| {
            !transaction.pending
                && transaction.amount > 0.0
                && taxable(transaction.account_id.as_deref().unwrap_or_default())
        }) {
            if !calendar::transaction_date(&transaction.date).is_some_and(in_year) {
                continue;
            }
            let amount = minor_units(transaction.amount);
            match transaction.kind() {
                Some(Kind::Interest) => earned.interest += amount,
                Some(Kind::Dividend) => match transaction.dividend_type.as_deref() {
                    Some("qualified") => earned.qualified += amount,
                    Some("nonQualified") => earned.non_qualified += amount,
                    _ => earned.unclassified += amount,
                },
                _ => {}
            }
        }
        earned
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
            long_term,
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

/// Each harvestable position's unrealized losses by term, what selling would save at this
/// year's rates, and when a recent purchase's wash-sale window closes.
fn harvest(
    harvestable: &[(String, String)],
    positions: &[PositionLots],
    trades: &[Trade],
    rates: Option<Rates>,
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
                tax_savings: rates.map(|rates| {
                    round2(
                        -(from_minor(short_term_loss) * rates.short_term
                            + from_minor(long_term_loss) * rates.long_term)
                            / 100.0,
                    )
                }),
                wash_until: latest_purchase.map(|date| date + Duration::days(WASH_DAYS)),
            })
        })
        .collect()
}

fn this_year_rates(years: &[TaxYear]) -> Option<Rates> {
    years
        .first()
        .and_then(|year| year.estimate.as_ref())
        .map(|estimate| Rates {
            short_term: estimate.marginal_rate,
            long_term: estimate.capital_gains_rate,
        })
}

/// This year's marginal rates, when there is an estimate.
pub fn rates(taxes: &Taxes) -> Option<Rates> {
    this_year_rates(&taxes.years)
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
                positions: &[],
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
                unclassified: 2_000.0
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
                positions: &[],
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
                positions: &[],
                harvestable: &[],
                profile: None,
            },
            day("2026-09-20"),
        );
        assert!(unprofiled.years[0].estimate.is_none() && !unprofiled.has_profile);
        assert!(unprofiled.years[0].tables_available);
    }
}

//! Money's and Investments' reports, read on demand: income, interest, fees, card credits,
//! dividends, realized gains, and cash flow, each over a window and the accounts the person
//! picks. A window is a chart period ending today, or a custom pair of days; each report sums its
//! entries beside the window before, by group, by day, month, or year, and by account. The
//! renderer only draws and lists what this returns.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::LazyLock;

use chrono::{Datelike, Duration, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::calendar::{self, Zone};
use super::View;
use crate::finance_contract::Transaction;
use crate::numeric::{from_minor, minor_units, round2};
use crate::transaction_policy::{Kind, Mark};

/// A report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Chart {
    CashFlow,
    Income,
    Interest,
    Fees,
    AmexCredits,
    Dividends,
    Realized,
}

impl Chart {
    pub const ALL: [Chart; 7] = [
        Chart::CashFlow,
        Chart::Income,
        Chart::Interest,
        Chart::Fees,
        Chart::AmexCredits,
        Chart::Dividends,
        Chart::Realized,
    ];
}

/// A chart period ending today.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Range {
    Day,
    Week,
    Month,
    Quarter,
    Year,
    All,
}

#[cfg(test)]
impl Range {
    pub const ALL: [Range; 6] = [
        Range::Day,
        Range::Week,
        Range::Month,
        Range::Quarter,
        Range::Year,
        Range::All,
    ];
}

/// The reports to read: over which window and accounts, and the one to read in full.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRequest {
    #[serde(default)]
    pub chart: Option<Chart>,
    /// The month when neither it nor `from` is given.
    #[serde(default)]
    pub range: Option<Range>,
    #[serde(default)]
    pub from: Option<NaiveDate>,
    #[serde(default)]
    pub to: Option<NaiveDate>,
    /// Every account when absent; none when empty.
    #[serde(default)]
    pub accounts: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReportWindow {
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// The snapshot's day: later days have no figures yet.
    pub saved_end: NaiveDate,
    pub previous_start: NaiveDate,
    pub previous_end: NaiveDate,
    pub range: Range,
    pub custom: bool,
    /// The window ends before it starts.
    pub valid: bool,
}

/// One report's total over the window, beside the window before.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReportSummary {
    pub chart: Chart,
    /// None when any entry has no value, such as a sale without a known cost.
    pub total: Option<f64>,
    /// None when the history doesn't reach the window before.
    pub previous: Option<f64>,
    pub change: Option<f64>,
    pub percent: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// `spending:` and a transaction ID, or `trade:` and a trade ID.
    pub id: String,
    pub date: NaiveDate,
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account_id: Option<String>,
    pub group: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct GroupTotal {
    pub label: String,
    pub value: Option<f64>,
    pub count: usize,
}

/// A day, month, or year of a report.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Bucket {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// None after the snapshot's day, or when an entry has no value.
    pub value: Option<f64>,
    pub count: usize,
    /// Newest first.
    pub entry_ids: Vec<String>,
    /// The bucket's gains and losses summed apart, for the chart's scale.
    pub positive: f64,
    pub negative: f64,
    /// Each group's gains and losses apart, by name, with their entries.
    pub groups: Vec<BucketGroup>,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct BucketGroup {
    pub name: String,
    pub negative: bool,
    pub total: f64,
    pub entry_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, TS, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Day,
    Month,
    Year,
}

/// One report in full over the window and accounts.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    #[serde(flatten)]
    pub summary: ReportSummary,
    /// In the window, as the feed lists them.
    pub entries: Vec<Entry>,
    pub average: Option<f64>,
    /// The entry farthest from zero.
    pub largest: Option<f64>,
    pub active_days: usize,
    /// Largest first.
    pub breakdown: Vec<GroupTotal>,
    pub buckets: Vec<Bucket>,
    pub unit: Unit,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountTotal {
    pub account_id: String,
    pub total: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct FlowItem {
    pub id: String,
    pub label: String,
    pub value: f64,
    /// Newest first.
    pub children: Vec<FlowTransaction>,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct FlowTransaction {
    pub id: String,
    pub label: String,
    pub value: f64,
    pub date: NaiveDate,
}

/// Where the window's money came from and went: sources by payer, purchases by category, fees,
/// taxes, and what was left over or had to be found.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct CashFlow {
    pub sources: Vec<FlowItem>,
    pub purchases: Vec<FlowItem>,
    pub fees: f64,
    pub taxes: f64,
    pub inflow: f64,
    pub outflow: f64,
    /// More went out than came in.
    pub gap: f64,
    /// More came in than went out.
    pub retained: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Reports {
    pub window: ReportWindow,
    /// Every report over the window and accounts.
    pub summaries: Vec<ReportSummary>,
    /// The requested report in full.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub report: Option<Report>,
    /// The requested report's total in each account, over every account.
    pub accounts: Vec<AccountTotal>,
    /// The requested report's groups across every account and day, so each keeps its color.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    /// The recurring series behind the requested report's entries on any day, in its accounts,
    /// so what they are expected to bring next can be listed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recurring_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub flow: Option<CashFlow>,
}

static BROKERAGE_TRANSFER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\btransfer\s+money\s+from\s+brokerage\b").unwrap());
static PAYROLL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bpayroll\b|\bpaycheck\b|\bsalary\b").unwrap());

/// The groups income is listed under: payment apps, brokerage transfers, dividends, interest,
/// payroll, or the payer as written.
fn income_group(transaction: &Transaction, mark: &Mark, zelle: bool) -> String {
    if zelle {
        return "Zelle payments".into();
    }
    if BROKERAGE_TRANSFER.is_match(&transaction.merchant) {
        return "Brokerage transfers".into();
    }
    match mark {
        Mark::Dividend => return "Dividends".into(),
        Mark::Interest => return "Interest".into(),
        _ => {}
    }
    if PAYROLL.is_match(&transaction.merchant) {
        return "Payroll".into();
    }
    transaction
        .merchant
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Values summed in whole cents; none when any is missing.
fn total<'a>(values: impl IntoIterator<Item = &'a Option<f64>>) -> Option<f64> {
    values
        .into_iter()
        .map(|value| value.map(minor_units))
        .sum::<Option<i64>>()
        .map(from_minor)
}

/// The change from `previous` to `total`, and as a percent of the size of `previous`.
fn summary(chart: Chart, total: Option<f64>, previous: Option<f64>) -> ReportSummary {
    let both = total.zip(previous);
    ReportSummary {
        chart,
        total,
        previous,
        change: both
            .map(|(total, previous)| from_minor(minor_units(total) - minor_units(previous))),
        percent: both
            .filter(|(_, previous)| *previous != 0.0)
            .map(|(total, previous)| round2((total - previous) / previous.abs() * 100.0)),
    }
}

struct Reader<'a> {
    view: &'a View,
    /// The snapshot's saved day as written, past which no entry is counted.
    written: NaiveDate,
    window: ReportWindow,
}

impl Reader<'_> {
    fn eligible(&self, accounts: Option<&[String]>, id: Option<&str>) -> bool {
        let usd = self
            .view
            .snapshot
            .accounts
            .iter()
            .find(|account| Some(account.id.as_str()) == id)
            .is_none_or(|account| account.currency.as_deref().is_none_or(|code| code == "USD"));
        usd && accounts.is_none_or(|accounts| id.is_some_and(|id| accounts.iter().any(|a| a == id)))
    }

    /// The name the user gave an account, or else the feed's.
    fn account_name(&self, transaction: &Transaction) -> String {
        transaction
            .account_id
            .as_deref()
            .and_then(|id| self.view.account_names.get(id))
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or(&transaction.account)
            .to_owned()
    }

    /// Every entry of `chart` in the accounts, on any day through the snapshot's.
    fn entries(&self, chart: Chart, accounts: Option<&[String]>) -> Vec<Entry> {
        let snapshot = &self.view.snapshot;
        match chart {
            Chart::Realized => snapshot
                .trades
                .iter()
                .filter(|trade| {
                    trade.r#type.to_uppercase().contains("SELL")
                        && self.eligible(accounts, Some(&trade.account_id))
                })
                .filter_map(|trade| {
                    let date = calendar::transaction_date(&trade.date)?;
                    (date <= self.written).then(|| Entry {
                        id: format!("trade:{}", trade.id),
                        date,
                        value: trade.estimated_realized_gain,
                        account_id: Some(trade.account_id.clone()),
                        group: trade.ticker.clone().unwrap_or_else(|| "Other sales".into()),
                    })
                })
                .collect(),
            Chart::AmexCredits => {
                // Native benefit rules classify the saved card's activity; this only scopes it.
                let insights = self.view.insights.as_ref();
                let benefits = insights
                    .and_then(|insights| insights.benefits.as_ref())
                    .filter(|benefits| {
                        self.view.spending_account_id.as_deref()
                            == Some(benefits.account_id.as_str())
                    });
                benefits
                    .map(|benefits| benefits.activity.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .filter(|item| {
                        !item.pending
                            && self.eligible(accounts, item.account_id.as_deref())
                            && matches!(
                                item.kind,
                                super::benefits::ActivityKind::Credit
                                    | super::benefits::ActivityKind::Reversal
                            )
                    })
                    .map(|item| Entry {
                        id: format!("spending:{}", item.id),
                        date: item.date,
                        value: Some(item.amount),
                        account_id: item.account_id.clone(),
                        group: item.benefit_name.clone(),
                    })
                    .collect()
            }
            _ => snapshot
                .transactions
                .iter()
                .filter(|transaction| {
                    !transaction.pending
                        && self.eligible(accounts, transaction.account_id.as_deref())
                })
                .filter_map(|transaction| {
                    let date = calendar::transaction_date(transaction.posted_date())?;
                    let classification = transaction.classification.as_ref()?;
                    if date > self.written {
                        return None;
                    }
                    let (value, group) = self.classify(chart, transaction, classification)?;
                    Some(Entry {
                        id: format!("spending:{}", transaction.id),
                        date,
                        value: Some(value),
                        account_id: transaction.account_id.clone(),
                        group,
                    })
                })
                .collect(),
        }
    }

    /// Whether a transaction belongs on `chart`, with the value it counts and its group.
    fn classify(
        &self,
        chart: Chart,
        transaction: &Transaction,
        classification: &crate::transaction_policy::Classification,
    ) -> Option<(f64, String)> {
        let amount = transaction.amount;
        let mark = &classification.mark;
        let kind = &classification.kind;
        match chart {
            Chart::Income => (amount > 0.0
                && matches!(mark, Mark::Income | Mark::Dividend | Mark::Interest))
            .then(|| {
                (
                    amount,
                    income_group(transaction, mark, classification.zelle),
                )
            }),
            Chart::Dividends => (amount > 0.0 && *mark == Mark::Dividend)
                .then(|| (amount, self.account_name(transaction))),
            Chart::Interest => (amount > 0.0 && *mark == Mark::Interest)
                .then(|| (amount, self.account_name(transaction))),
            Chart::Fees => (*kind == Kind::Fee).then(|| (-amount, self.account_name(transaction))),
            _ => {
                let label = match kind {
                    Kind::Income => "Income",
                    Kind::Dividend => "Dividends",
                    Kind::Interest => "Interest",
                    Kind::Reimbursement => "Reimbursements",
                    Kind::Expense => "Purchases",
                    Kind::Fee => "Fees",
                    Kind::Tax => "Taxes",
                    _ => return None,
                };
                let money_in = matches!(
                    kind,
                    Kind::Income | Kind::Dividend | Kind::Interest | Kind::Reimbursement
                );
                (!money_in || amount > 0.0).then(|| (amount, label.to_owned()))
            }
        }
    }

    /// The report over the window: the window before counts only when the history reaches it.
    fn report(&self, chart: Chart, entries: &[Entry]) -> Report {
        let window = &self.window;
        let selected = entries
            .iter()
            .filter(|entry| entry.date >= window.start && entry.date <= window.end)
            .cloned()
            .collect::<Vec<_>>();
        let values = |items: &[&Entry]| total(items.iter().map(|entry| &entry.value));
        let all = selected.iter().collect::<Vec<_>>();
        let sum = values(&all);
        let comparable = window.range != Range::All
            && window.saved_end >= window.start
            && entries
                .iter()
                .any(|entry| entry.date <= window.previous_start);
        // Past a stale snapshot, compare only the days it covers against as many before.
        let previous_end = if window.end <= window.saved_end {
            window.previous_end
        } else {
            window.previous_start + (window.saved_end - window.start)
        };
        let previous = comparable
            .then(|| {
                values(
                    &entries
                        .iter()
                        .filter(|entry| {
                            entry.date >= window.previous_start && entry.date <= previous_end
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .flatten();
        let mut groups: Vec<(String, Vec<&Entry>)> = Vec::new();
        for entry in &selected {
            match groups.iter_mut().find(|(label, _)| *label == entry.group) {
                Some((_, items)) => items.push(entry),
                None => groups.push((entry.group.clone(), vec![entry])),
            }
        }
        let mut breakdown = groups
            .iter()
            .map(|(label, items)| GroupTotal {
                label: label.clone(),
                value: values(items),
                count: items.len(),
            })
            .collect::<Vec<_>>();
        breakdown.sort_by(|left, right| {
            right
                .value
                .unwrap_or(0.0)
                .abs()
                .total_cmp(&left.value.unwrap_or(0.0).abs())
        });
        let days = (window.end - window.start).num_days() + 1;
        let unit = if days <= 45 {
            Unit::Day
        } else if days > 3 * 366 {
            Unit::Year
        } else {
            Unit::Month
        };
        let mut buckets = Vec::new();
        let mut cursor = window.start;
        while window.valid && cursor <= window.end {
            let next = match unit {
                Unit::Day => cursor + Duration::days(1),
                Unit::Month => calendar::add_months_first(cursor, 1),
                Unit::Year => calendar::first_of_month(cursor.year() + 1, 1),
            };
            let to = (next - Duration::days(1)).min(window.end);
            let mut items = selected
                .iter()
                .filter(|entry| entry.date >= cursor && entry.date <= to)
                .collect::<Vec<_>>();
            items.sort_by_key(|entry| std::cmp::Reverse(entry.date));
            buckets.push(bucket(cursor, to, &items, cursor > window.saved_end));
            cursor = next;
        }
        let average = sum
            .filter(|_| !selected.is_empty())
            .map(|sum| round2(sum / selected.len() as f64));
        // The first entry farthest from zero.
        let largest = sum.and_then(|_| {
            selected.iter().filter_map(|entry| entry.value).fold(
                None,
                |current: Option<f64>, value| match current {
                    Some(current) if current.abs() >= value.abs() => Some(current),
                    _ => Some(value),
                },
            )
        });
        Report {
            summary: summary(chart, sum, previous),
            active_days: selected
                .iter()
                .map(|entry| entry.date)
                .collect::<BTreeSet<_>>()
                .len(),
            entries: selected,
            average,
            largest,
            breakdown,
            buckets,
            unit,
        }
    }
}

/// One bucket: its total, its gains and losses apart, and each group's.
fn bucket(from: NaiveDate, to: NaiveDate, items: &[&Entry], after_snapshot: bool) -> Bucket {
    let known = items.iter().all(|entry| entry.value.is_some());
    let cents = |entry: &&Entry| entry.value.map_or(0, minor_units);
    let mut groups = BTreeMap::<(String, bool), (i64, Vec<String>)>::new();
    // Each group stacks its entries by account, then day.
    let mut stacked = items.to_vec();
    stacked.sort_by(|left, right| {
        left.account_id
            .cmp(&right.account_id)
            .then(left.date.cmp(&right.date))
            .then(left.id.cmp(&right.id))
    });
    for entry in &stacked {
        let value = cents(entry);
        if value == 0 || !known {
            continue;
        }
        let group = groups.entry((entry.group.clone(), value < 0)).or_default();
        group.0 += value;
        group.1.push(entry.id.clone());
    }
    let side = |negative: bool| {
        from_minor(
            items
                .iter()
                .map(cents)
                .filter(|value| (*value < 0) == negative)
                .sum::<i64>(),
        )
    };
    Bucket {
        from,
        to,
        value: if after_snapshot {
            None
        } else {
            total(items.iter().map(|entry| &entry.value))
        },
        count: items.len(),
        entry_ids: items.iter().map(|entry| entry.id.clone()).collect(),
        positive: if known { side(false) } else { 0.0 },
        negative: if known { side(true) } else { 0.0 },
        groups: groups
            .into_iter()
            .map(|((name, negative), (cents, entry_ids))| BucketGroup {
                name,
                negative,
                total: from_minor(cents),
                entry_ids,
            })
            .collect(),
    }
}

/// The window: a chart period ending today or at `to`, or the custom days given. `saved` is the
/// snapshot's day.
fn window(
    view: &View,
    request: &ReportRequest,
    today: NaiveDate,
    saved: NaiveDate,
) -> ReportWindow {
    let snapshot = &view.snapshot;
    let end = request.to.filter(|to| *to < today).unwrap_or(today);
    let range = request.range.unwrap_or(Range::Month);
    let start = request.from.unwrap_or_else(|| match range {
        Range::All => snapshot
            .transactions
            .iter()
            .map(|transaction| transaction.posted_date())
            .chain(snapshot.trades.iter().map(|trade| trade.date.as_str()))
            .filter_map(calendar::transaction_date)
            .filter(|date| *date <= end)
            .min()
            .unwrap_or(end),
        Range::Year => calendar::first_of_month(end.year(), 1),
        Range::Day => end,
        Range::Week => end - Duration::days(6),
        Range::Quarter => end - Duration::days(89),
        Range::Month => end - Duration::days(29),
    });
    let days = (end - start).num_days() + 1;
    ReportWindow {
        start,
        end,
        saved_end: saved,
        previous_start: start - Duration::days(days.max(1)),
        previous_end: start - Duration::days(1),
        range,
        custom: request.from.is_some() || request.to.is_some(),
        valid: days > 0,
    }
}

/// Where the money in `entries` came from and went.
fn cash_flow(view: &View, entries: &[Entry]) -> CashFlow {
    let ids = entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut sources = Vec::<(String, FlowTransaction)>::new();
    let mut purchases = Vec::<(String, FlowTransaction)>::new();
    let (mut fees, mut taxes) = (0, 0);
    for transaction in &view.snapshot.transactions {
        let Some(classification) = transaction.classification.as_ref() else {
            continue;
        };
        if !ids.contains(format!("spending:{}", transaction.id).as_str()) {
            continue;
        }
        let date = calendar::transaction_date(transaction.posted_date()).unwrap_or_default();
        let amount = minor_units(transaction.amount.abs());
        let kind = &classification.kind;
        if transaction.amount > 0.0
            && matches!(
                kind,
                Kind::Income
                    | Kind::Dividend
                    | Kind::Interest
                    | Kind::Reimbursement
                    | Kind::Fee
                    | Kind::Tax
            )
        {
            let source = Some(transaction.merchant.trim())
                .filter(|merchant| !merchant.is_empty())
                .unwrap_or("Unknown source")
                .to_owned();
            let label = transaction
                .description
                .as_deref()
                .map(str::trim)
                .filter(|description| !description.is_empty())
                .map_or_else(|| source.clone(), str::to_owned);
            sources.push((
                source,
                FlowTransaction {
                    id: format!("source-transaction:{}", transaction.id),
                    label,
                    value: from_minor(amount),
                    date,
                },
            ));
        } else if *kind == Kind::Expense {
            for (index, (category, part)) in transaction.parts().into_iter().enumerate() {
                let label = Some(transaction.merchant.trim())
                    .filter(|merchant| !merchant.is_empty())
                    .or_else(|| transaction.description.as_deref().map(str::trim))
                    .filter(|label| !label.is_empty())
                    .unwrap_or("Purchase")
                    .to_owned();
                purchases.push((
                    Some(category.trim())
                        .filter(|category| !category.is_empty())
                        .unwrap_or("Other")
                        .to_owned(),
                    FlowTransaction {
                        id: if index == 0 {
                            format!("purchase-transaction:{}", transaction.id)
                        } else {
                            format!("purchase-transaction:{}:{index}", transaction.id)
                        },
                        label,
                        value: from_minor(minor_units(part.abs())),
                        date,
                    },
                ));
            }
        } else if *kind == Kind::Fee {
            fees += amount;
        } else if *kind == Kind::Tax {
            taxes += amount;
        }
    }
    let sources = flow_items(sources, "source");
    let purchases = flow_items(purchases, "purchase");
    let positive = |items: &[FlowItem]| -> i64 {
        items
            .iter()
            .map(|item| minor_units(item.value))
            .filter(|value| *value > 0)
            .sum()
    };
    let inflow = positive(&sources);
    let outflow = positive(&purchases) + fees + taxes;
    CashFlow {
        sources,
        purchases,
        fees: from_minor(fees),
        taxes: from_minor(taxes),
        inflow: from_minor(inflow),
        outflow: from_minor(outflow),
        gap: from_minor((outflow - inflow).max(0)),
        retained: from_minor((inflow - outflow).max(0)),
    }
}

/// Transactions by group, largest first, each newest first.
fn flow_items(entries: Vec<(String, FlowTransaction)>, prefix: &str) -> Vec<FlowItem> {
    let mut groups: Vec<(String, Vec<FlowTransaction>)> = Vec::new();
    for (group, transaction) in entries {
        match groups.iter_mut().find(|(label, _)| *label == group) {
            Some((_, children)) => children.push(transaction),
            None => groups.push((group, vec![transaction])),
        }
    }
    let mut items = groups
        .into_iter()
        .map(|(label, mut children)| {
            children.sort_by(|left, right| {
                right
                    .date
                    .cmp(&left.date)
                    .then_with(|| right.value.total_cmp(&left.value))
                    .then_with(|| left.label.cmp(&right.label))
            });
            FlowItem {
                id: format!("{prefix}:{label}"),
                value: from_minor(children.iter().map(|child| minor_units(child.value)).sum()),
                label,
                children,
            }
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .value
            .total_cmp(&left.value)
            .then_with(|| left.label.cmp(&right.label))
    });
    items
}

/// Every report over the request's window and accounts, and the requested one in full. Windows
/// run through `today`, which may be later than the snapshot's day.
pub fn reports(
    view: &View,
    request: &ReportRequest,
    today: NaiveDate,
    zone: &dyn Zone,
) -> Option<Reports> {
    let snapshot = &view.snapshot;
    let saved = calendar::calendar_date(&snapshot.updated_at, zone)?;
    let written = calendar::transaction_date(&snapshot.updated_at)?;
    let reader = Reader {
        view,
        written,
        window: window(view, request, today.max(saved), saved),
    };
    let accounts = request.accounts.as_deref();
    let summaries = Chart::ALL
        .iter()
        .map(|chart| {
            reader
                .report(*chart, &reader.entries(*chart, accounts))
                .summary
        })
        .collect();
    let Some(chart) = request.chart else {
        return Some(Reports {
            window: reader.window,
            summaries,
            report: None,
            accounts: Vec::new(),
            groups: Vec::new(),
            recurring_ids: Vec::new(),
            flow: None,
        });
    };
    let scoped = reader.entries(chart, accounts);
    let entry_ids = scoped
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<BTreeSet<_>>();
    let recurring_ids = snapshot
        .transactions
        .iter()
        .filter(|transaction| entry_ids.contains(format!("spending:{}", transaction.id).as_str()))
        .filter_map(|transaction| transaction.recurring_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let report = reader.report(chart, &scoped);
    let everywhere = reader.entries(chart, None);
    let in_window = reader.report(chart, &everywhere).entries;
    let mut by_account = HashMap::<&str, Vec<&Option<f64>>>::new();
    for entry in &in_window {
        if let Some(id) = entry.account_id.as_deref() {
            by_account.entry(id).or_default().push(&entry.value);
        }
    }
    let account_totals = snapshot
        .accounts
        .iter()
        .filter(|account| {
            account.id != "all" && account.currency.as_deref().is_none_or(|code| code == "USD")
        })
        .filter_map(|account| {
            let values = by_account.get(account.id.as_str())?;
            Some(AccountTotal {
                account_id: account.id.clone(),
                total: total(values.iter().copied()),
            })
        })
        .collect();
    Some(Reports {
        window: reader.window.clone(),
        summaries,
        accounts: account_totals,
        groups: everywhere
            .iter()
            .map(|entry| entry.group.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        recurring_ids,
        flow: (chart == Chart::CashFlow).then(|| cash_flow(view, &report.entries)),
        report: Some(report),
    })
}

#[cfg(test)]
mod tests;

//! One spending period in full, read on demand: the transactions it lists, their merchants, and
//! the running total beside the period before, or for all time its bars by month. Each category
//! carries the same narrowed to its own spending, so focusing on one needs no other request;
//! its bars are its share of each month.
use super::*;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PeriodDetail {
    #[serde(flatten)]
    pub period: SpendingPeriod,
    /// The saved history reaches a period before this one.
    pub earlier: bool,
    #[serde(flatten)]
    pub view: PeriodView,
    /// All time's bars by month; other periods draw their pace.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bars: Vec<Bar>,
    /// The view narrowed to each category with spending, by name.
    pub focus: BTreeMap<String, PeriodView>,
}

/// What a period lists and paces, for all of it or one category.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PeriodView {
    /// The transactions listed, newest first: someone else's only when counting theirs, and
    /// for a category only those with spending in it.
    pub transaction_ids: Vec<String>,
    /// The listed transactions by merchant, refunds netting out, largest spending first.
    pub merchants: Vec<Merchant>,
    pub pace: Pace,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Merchant {
    pub key: String,
    pub name: String,
    /// Signed as transactions are.
    pub total: f64,
    /// Newest first.
    pub transaction_ids: Vec<String>,
}

/// Spending accumulated day by day, beside the whole period before.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pace {
    pub start: NaiveDate,
    /// The period's last day, past today for the current period.
    pub end: NaiveDate,
    /// The running total on each day from `start` through the period's end so far.
    pub current: Vec<f64>,
    /// The same over the whole period before; empty for all time.
    pub previous: Vec<f64>,
    /// Where the current period is heading at its pace so far.
    pub projected: Option<f64>,
    /// Each day with spending.
    pub days: Vec<PaceDay>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaceDay {
    /// Days from the period's start.
    pub index: i64,
    pub date: NaiveDate,
    pub total: f64,
    /// Largest first.
    pub purchases: Vec<Purchase>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Purchase {
    pub id: String,
    /// Positive.
    pub value: f64,
}

/// A day, week, or month of spending by category; credits count against it as `Credits`.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Bar {
    pub from: NaiveDate,
    pub to: NaiveDate,
    #[serde(skip_serializing_if = "is_zero")]
    pub value: f64,
    /// By name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<BarCategory>,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BarCategory {
    pub name: String,
    /// Negative for credits.
    pub value: f64,
    pub purchases: Vec<Purchase>,
}

/// What the chart calls credits that offset spending.
const CREDITS: &str = "Credits";

fn is_credit(transaction: &Transaction) -> bool {
    transaction
        .classification
        .as_ref()
        .is_some_and(|classification| classification.credit)
}

/// Names in the order a person reads them, ignoring case.
fn by_name(left: &str, right: &str) -> std::cmp::Ordering {
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| left.cmp(right))
}

impl Scope<'_> {
    /// Whether a transaction belongs on this owner's list: someone else's spending lists only
    /// their purchases, and yours leaves theirs out.
    fn lists(&self, transaction: &Transaction) -> bool {
        match self.owner {
            Owner::Me => !transaction.other,
            Owner::Other => transaction.other,
            Owner::All => true,
        }
    }

    /// Spending from `from` through `through` in whole cents, by transaction and date: whole
    /// purchases, or with a focus only their parts in it.
    fn spends(
        &self,
        from: NaiveDate,
        through: NaiveDate,
        focus: Option<&str>,
    ) -> Vec<(NaiveDate, &Transaction, i64)> {
        self.activity
            .dated
            .iter()
            .filter(|(date, transaction)| {
                *date >= from && *date <= through && spending_for(transaction, self.owner)
            })
            .filter_map(|(date, transaction)| {
                let cents = match focus {
                    Some(focus) => transaction
                        .parts()
                        .into_iter()
                        .filter(|(category, _)| *category == focus)
                        .map(|(_, amount)| minor_units(amount.abs()))
                        .sum(),
                    None => minor_units(transaction.amount.abs()),
                };
                (focus.is_none() || cents != 0).then_some((*date, *transaction, cents))
            })
            .collect()
    }

    /// The running total on each day from `from` through `through`.
    fn running(&self, from: NaiveDate, through: NaiveDate, focus: Option<&str>) -> Vec<f64> {
        let mut by_day = BTreeMap::<NaiveDate, i64>::new();
        for (date, _, cents) in self.spends(from, through, focus) {
            *by_day.entry(date).or_default() += cents;
        }
        let mut total = 0;
        from.iter_days()
            .take_while(|day| *day <= through)
            .map(|day| {
                total += by_day.get(&day).copied().unwrap_or(0);
                from_minor(total)
            })
            .collect()
    }

    /// The period `offset` periods before the current one of `kind`, in full; see `period`.
    pub fn detail(&self, kind: PeriodKind, offset: usize) -> Option<PeriodDetail> {
        let (period, earlier) = self.period(kind, offset)?;
        let focus = period
            .categories
            .iter()
            .map(|category| {
                let view = self.view(&period, Some(&category.name));
                (category.name.clone(), view)
            })
            .collect();
        Some(PeriodDetail {
            earlier,
            view: self.view(&period, None),
            bars: if period.kind == PeriodKind::All {
                self.bars(period.start, period.end)
            } else {
                Vec::new()
            },
            focus,
            period,
        })
    }

    /// What `period` lists and paces, for all of it or the category in `focus`.
    fn view(&self, period: &SpendingPeriod, focus: Option<&str>) -> PeriodView {
        let (start, end) = (period.start, period.end);
        let calendar = self.calendar(period.kind);
        let mut listed = self
            .activity
            .dated
            .iter()
            .filter(|(date, transaction)| {
                *date >= start
                    && *date <= end
                    && self.lists(transaction)
                    && focus.is_none_or(|focus| {
                        spending_for(transaction, self.owner)
                            && transaction
                                .parts()
                                .iter()
                                .any(|(category, _)| *category == focus)
                    })
            })
            .map(|(_, transaction)| *transaction)
            .collect::<Vec<_>>();
        // Newest by the transaction's own date, then largest.
        listed.sort_by(|left, right| {
            calendar::transaction_date(&right.date)
                .cmp(&calendar::transaction_date(&left.date))
                .then_with(|| right.amount.abs().total_cmp(&left.amount.abs()))
        });
        let pace = Pace {
            start,
            end: if period.kind == PeriodKind::All {
                end
            } else {
                calendar.last_day(start, end)
            },
            current: self.running(start, end, focus),
            previous: if period.kind == PeriodKind::All {
                Vec::new()
            } else {
                self.running(
                    calendar.previous_start(start),
                    start - Duration::days(1),
                    focus,
                )
            },
            projected: period.projected.as_ref().and_then(|projected| match focus {
                Some(focus) => projected.categories.get(focus).copied(),
                None => Some(projected.total),
            }),
            days: self.pace_days(start, end, focus),
        };
        PeriodView {
            transaction_ids: listed
                .iter()
                .map(|transaction| transaction.id.clone())
                .collect(),
            merchants: merchants(&listed),
            pace,
        }
    }

    /// Each day with spending, its purchases largest first.
    fn pace_days(&self, start: NaiveDate, end: NaiveDate, focus: Option<&str>) -> Vec<PaceDay> {
        let mut days = BTreeMap::<NaiveDate, Vec<(String, i64)>>::new();
        for (date, transaction, cents) in self.spends(start, end, focus) {
            days.entry(date)
                .or_default()
                .push((transaction.id.clone(), cents));
        }
        days.into_iter()
            .map(|(date, mut purchases)| {
                purchases.sort_by_key(|purchase| std::cmp::Reverse(purchase.1));
                PaceDay {
                    index: (date - start).num_days(),
                    date,
                    total: from_minor(purchases.iter().map(|(_, cents)| cents).sum()),
                    purchases: purchases
                        .into_iter()
                        .map(|(id, cents)| Purchase {
                            id,
                            value: from_minor(cents),
                        })
                        .collect(),
                }
            })
            .collect()
    }

    /// All time's spending and credits by category, a bar a month from its first day; other
    /// periods draw their pace instead.
    fn bars(&self, start: NaiveDate, end: NaiveDate) -> Vec<Bar> {
        // Each month's categories in cents, with the purchases behind them.
        let mut by_month = BTreeMap::<NaiveDate, BTreeMap<String, (i64, Vec<Purchase>)>>::new();
        for (date, transaction) in &self.activity.dated {
            let credit = is_credit(transaction);
            if *date < start
                || *date > end
                || !self.lists(transaction)
                || !(credit || spending_for(transaction, self.owner))
            {
                continue;
            }
            for (category, amount) in transaction.parts() {
                let cents = minor_units(amount.abs());
                let (name, cents) = if credit {
                    (CREDITS, -cents)
                } else {
                    (category, cents)
                };
                let entry = by_month
                    .entry(calendar::add_months_first(*date, 0))
                    .or_default()
                    .entry(name.to_owned())
                    .or_default();
                entry.0 += cents;
                entry.1.push(Purchase {
                    id: transaction.id.clone(),
                    value: from_minor(cents.abs()),
                });
            }
        }
        let mut month = calendar::add_months_first(start, 0);
        let mut bars = Vec::new();
        while month <= end {
            let next = calendar::add_months_first(month, 1);
            let mut categories = by_month
                .remove(&month)
                .unwrap_or_default()
                .into_iter()
                .map(|(name, (cents, purchases))| (name, cents, purchases))
                .collect::<Vec<_>>();
            categories.sort_by(|left, right| by_name(&left.0, &right.0));
            bars.push((
                month.max(start),
                (next - Duration::days(1)).min(end),
                categories,
            ));
            month = next;
        }
        bars.into_iter()
            .map(|(from, to, categories)| Bar {
                from,
                to,
                value: from_minor(categories.iter().map(|(_, cents, _)| cents).sum()),
                categories: categories
                    .into_iter()
                    .map(|(name, cents, purchases)| BarCategory {
                        name,
                        value: from_minor(cents),
                        purchases,
                    })
                    .collect(),
            })
            .collect()
    }
}

/// Transactions grouped by merchant, refunds netting out, largest spending first; each keeps
/// its transactions in the order given.
fn merchants(listed: &[&Transaction]) -> Vec<Merchant> {
    let mut groups: Vec<(String, String, i64, Vec<String>)> = Vec::new();
    for transaction in listed {
        let key = transaction.merchant.trim().to_lowercase();
        let index = match groups.iter().position(|group| group.0 == key) {
            Some(index) => index,
            None => {
                groups.push((key, transaction.merchant.clone(), 0, Vec::new()));
                groups.len() - 1
            }
        };
        groups[index].2 += minor_units(transaction.amount);
        groups[index].3.push(transaction.id.clone());
    }
    groups.sort_by(|left, right| {
        left.2
            .cmp(&right.2)
            .then_with(|| by_name(&left.1, &right.1))
    });
    groups
        .into_iter()
        .map(|(key, name, cents, transaction_ids)| Merchant {
            key,
            name,
            total: from_minor(cents),
            transaction_ids,
        })
        .collect()
}

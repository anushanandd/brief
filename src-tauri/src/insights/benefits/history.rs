//! Each benefit's periods, from the year the saved card history starts through the end of
//! this year, and the matches that need a person's decision.
use std::collections::{HashMap, HashSet};

use super::attribution::EARLY_CREDIT_DAYS;
use super::windows::benefit_window;
use super::*;

/// What became of one period, or what it is now.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PeriodState {
    /// Closed and settled with nothing usable left.
    Full,
    /// Closed and settled with some allowance unused.
    Partial,
    /// Closed and settled with nothing credited.
    Missed,
    /// Contains today.
    Open,
    /// Closed, but credits for it may still post.
    Settling,
    Future,
    /// Use doesn't reach the card (Uber Cash spent before a charge).
    NotVisible,
    /// A card charge suggests the period's Uber Cash ran out first.
    LikelyUsed,
    /// The saved card history starts after the period began.
    BeforeHistory,
    /// The rules don't estimate this benefit's unused allowance.
    NotTracked,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenefitPeriod {
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// "Jan", "Q3", "Jul – Dec", "2026", or the dates of a period cut short.
    pub label: String,
    pub cap: f64,
    /// Credits net of reversals placed here, including ones that may belong to the period
    /// before.
    pub credited: f64,
    pub state: PeriodState,
    /// A settled period's usable allowance left unused, a range when credits can't be placed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unused: Option<AmountRange>,
    /// What an open period still has, from zero when the history starts inside it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<AmountRange>,
    /// The credits, reversals, and purchases behind it.
    pub activity_ids: Vec<String>,
    /// Credits still to come in an open or future period: posted purchases waiting for theirs,
    /// then the charges the forecast expects, each capped by what the period has left.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub planned: Vec<PlannedCredit>,
}

/// A charge the forecast expects on the card, already tied to the benefit its series earns.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedCharge {
    pub benefit_id: String,
    pub date: NaiveDate,
    /// Negative, as charged.
    pub amount: f64,
    pub title: String,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PlannedSource {
    /// A posted qualifying purchase whose credit hasn't posted yet.
    Purchase,
    /// A charge a recurring series is expected to make.
    Recurring,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlannedCredit {
    pub title: String,
    pub date: NaiveDate,
    /// The credit it should earn.
    pub amount: f64,
    pub source: PlannedSource,
    /// The posted purchase, for a credit it is waiting for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenefitHistory {
    pub id: String,
    pub name: String,
    pub recurring: bool,
    /// Its use doesn't show on the card, so periods are estimated from charges.
    pub external: bool,
    /// The benefit ended before today; its periods remain as history.
    pub retired: bool,
    pub periods: Vec<BenefitPeriod>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewKind {
    /// A positive amount from a benefit's merchant without the issuer's credit wording: a
    /// credit or a return.
    UnverifiedCredit,
    /// The issuer's credit wording that no rule recognizes.
    UnassignedCredit,
    /// A credit without a qualifying purchase, and a charge that likely earned it.
    NoPurchase,
    /// A matched purchase whose credit never came within the terms' limit.
    NotEligible,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenefitReview {
    pub kind: ReviewKind,
    pub transaction_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benefit_id: Option<String>,
    /// The first day of the period it belongs to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_start: Option<NaiveDate>,
    /// For a credit without a purchase: the card charge that most likely earned it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion_id: Option<String>,
}

/// What an open or future period should still be credited, in the order it should arrive.
/// Uber Cash posts no credits, and a closed period expects nothing new.
fn planned(
    rule: &Rule,
    period: &Period,
    matched: &[BenefitActivity],
    today: NaiveDate,
    charges: &[PlannedCharge],
) -> Vec<PlannedCredit> {
    let definition = &rule.data;
    if period.end < today || !definition.issuer_credits {
        return Vec::new();
    }
    let credits = RuleSet::window_credits(rule, period, matched, today);
    // An allowance plus tax has no dollar cap, so each charge sets its own credit.
    let mut room = if definition.track_remaining {
        (minor_units(period.cap) - credits.certain - credits.provisional).max(0)
    } else {
        i64::MAX
    };
    let mut take = |amount: i64| {
        let credit = amount.min(room);
        room -= credit;
        credit
    };
    let mut planned = Vec::new();
    for item in credits.activity.iter().filter(|item| item.awaits_credit()) {
        let Some(PurchaseCredit::Expected { amount, .. } | PurchaseCredit::Late { amount, .. }) =
            item.credit
        else {
            continue;
        };
        let credit = take(minor_units(amount));
        if credit > 0 {
            planned.push(PlannedCredit {
                title: item.merchant.clone(),
                date: item.date,
                amount: from_minor(credit),
                source: PlannedSource::Purchase,
                transaction_id: Some(item.id.clone()),
            });
        }
    }
    let mut upcoming = charges
        .iter()
        .filter(|charge| {
            charge.benefit_id == definition.id
                && charge.date >= today
                && charge.date >= period.start
                && charge.date <= period.end
        })
        .collect::<Vec<_>>();
    upcoming.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then(left.title.cmp(&right.title))
    });
    for charge in upcoming {
        let credit = take(-minor_units(charge.amount));
        if credit > 0 {
            planned.push(PlannedCredit {
                title: charge.title.clone(),
                date: charge.date,
                amount: from_minor(credit),
                source: PlannedSource::Recurring,
                transaction_id: None,
            });
        }
    }
    planned
}

fn label(period: &Period) -> String {
    let whole = benefit_window(period.start, period.cadence) == (period.start, period.end);
    if !whole {
        return format!(
            "{} – {}",
            calendar::short_label(period.start),
            calendar::short_label(period.end)
        );
    }
    match period.cadence {
        Cadence::Monthly => period.start.format("%b").to_string(),
        Cadence::Quarterly => format!("Q{}", period.start.month0() / 3 + 1),
        Cadence::Semiannual => format!(
            "{} – {}",
            period.start.format("%b"),
            period.end.format("%b")
        ),
        _ => period.start.year().to_string(),
    }
}

impl RuleSet {
    /// Every calendar benefit's periods from January of the history's first year through
    /// December of this year, judged as of `today`. Renewal and per-purchase benefits have no
    /// periods.
    pub fn history(
        &self,
        matched: &[BenefitActivity],
        history_start: Option<NaiveDate>,
        today: NaiveDate,
        charges: &[PlannedCharge],
    ) -> Vec<BenefitHistory> {
        let year = |date: NaiveDate| date.year();
        let first_year = history_start.map_or(year(today), year).min(year(today));
        let (Some(from), Some(to)) = (
            NaiveDate::from_ymd_opt(first_year, 1, 1),
            NaiveDate::from_ymd_opt(year(today), 12, 31),
        ) else {
            return Vec::new();
        };
        self.rules
            .iter()
            .filter(|rule| rule.data.cadence.is_calendar())
            .map(|rule| {
                let definition = &rule.data;
                let lag = CreditLag::observe(rule, matched);
                let until = definition.effective_until.map_or(to, |until| until.min(to));
                let mut cursor = definition
                    .effective_from
                    .map_or(from, |start| start.max(from));
                let mut periods = Vec::new();
                while cursor <= until {
                    let mut period = rule.period(cursor);
                    // A change of cadence starts a new allowance, so the old one ends the day
                    // before it.
                    if let Some(change) = definition.allowance_changes.iter().find(|change| {
                        change.from > period.start
                            && change.from <= period.end
                            && change.cadence != period.cadence
                    }) {
                        period.end = change.from - Duration::days(1);
                    }
                    period.end = period.end.min(until);
                    // The allowance in effect today, for the period that holds today.
                    period.cap = rule.period(today.clamp(period.start, period.end)).cap;
                    let mut record =
                        self.period_record(rule, &period, matched, history_start, today, lag.days);
                    record.planned = planned(rule, &period, matched, today, charges);
                    periods.push(record);
                    cursor = period.end + Duration::days(1);
                }
                BenefitHistory {
                    id: definition.id.clone(),
                    name: definition.name.clone(),
                    recurring: definition.recurring,
                    external: !definition.issuer_credits,
                    retired: definition
                        .effective_until
                        .is_some_and(|until| until < today),
                    periods,
                }
            })
            .collect()
    }

    fn period_record(
        &self,
        rule: &Rule,
        period: &Period,
        matched: &[BenefitActivity],
        history_start: Option<NaiveDate>,
        today: NaiveDate,
        lag_days: i64,
    ) -> BenefitPeriod {
        use PeriodState::*;
        let definition = &rule.data;
        let credits = Self::window_credits(rule, period, matched, today);
        let cap = minor_units(period.cap);
        let charged = credits
            .activity
            .iter()
            .any(|item| item.kind == ActivityKind::Purchase && !item.pending);
        // The history covers the whole period, or only its end. A period it reaches partway is
        // judged by the credits it shows, as the whole of what the period credited.
        let covered = history_start.is_some_and(|start| start <= period.start);
        let reached = history_start.is_some_and(|start| start <= period.end);
        let tracked = definition.track_remaining && definition.issuer_credits;
        let credited = from_minor(credits.certain + credits.provisional);
        let seen = credited != 0.0 || charged;
        let (low, high) = credits.unused(cap);
        let usable = |low: i64, high: i64| AmountRange {
            low: from_minor(rule.usable(low)),
            high: from_minor(rule.usable(high)),
        };
        let (state, unused, left) = if period.start > today {
            (Future, None, None)
        } else if period.end >= today {
            match (definition.issuer_credits, tracked) {
                (false, _) => (if charged { LikelyUsed } else { Open }, None, None),
                (true, false) => (Open, None, None),
                // Credits from before the saved history are missing, so as little as nothing
                // may be left.
                (true, true) => (Open, None, Some(usable(low, high))),
            }
        } else if !reached || (!covered && !seen) {
            (BeforeHistory, None, None)
        } else if !definition.issuer_credits {
            // Uber Cash is spent before the card is charged, so only a charge is evidence.
            (if charged { LikelyUsed } else { NotVisible }, None, None)
        } else if !tracked {
            (if credited > 0.0 { Full } else { NotTracked }, None, None)
        } else if today <= period.end + Duration::days(lag_days)
            || credits.activity.iter().any(|item| item.awaits_credit())
        {
            (Settling, None, None)
        } else {
            let range = usable(low, high);
            // Cents left over still count as missed, but the credit was used in full.
            let state = if range.high < from_minor(NEGLIGIBLE_CENTS) {
                Full
            } else if range.low == from_minor(cap) {
                Missed
            } else {
                Partial
            };
            (state, Some(range), None)
        };
        BenefitPeriod {
            start: period.start,
            end: period.end,
            label: label(period),
            cap: period.cap,
            credited,
            state,
            unused,
            left,
            activity_ids: credits
                .activity
                .iter()
                .map(|item| item.id.clone())
                .collect(),
            planned: Vec::new(),
        }
    }

    /// Matches that need a decision, newest first. A decision saved on the transaction (a
    /// confirmation, a rejection, or an assignment) settles each one.
    pub fn review(&self, own: &[&Transaction], matched: &[BenefitActivity]) -> Vec<BenefitReview> {
        let transactions = own
            .iter()
            .map(|transaction| (transaction.id.as_str(), *transaction))
            .collect::<HashMap<_, _>>();
        let undecided = |id: &str| {
            transactions.get(id).is_some_and(|transaction| {
                transaction.benefit_confirmed.is_none() && transaction.benefit_id.is_none()
            })
        };
        let rule = |id: &str| self.rules.iter().find(|rule| rule.data.id == id);
        let mut items = Vec::new();
        for item in matched
            .iter()
            .filter(|item| !item.pending && undecided(&item.id))
        {
            let Some(rule) = rule(&item.benefit_id) else {
                continue;
            };
            let (kind, suggestion) = match (item.kind, &item.credit) {
                (ActivityKind::Unverified, _) => (ReviewKind::UnverifiedCredit, None),
                (ActivityKind::Credit, _) if item.paired_id.is_none() => {
                    match self.likely_purchase(item, rule, own, matched) {
                        Some(purchase) => (ReviewKind::NoPurchase, Some(purchase)),
                        None => continue,
                    }
                }
                (ActivityKind::Purchase, Some(PurchaseCredit::NotCredited)) => {
                    (ReviewKind::NotEligible, None)
                }
                _ => continue,
            };
            items.push((
                item.date,
                BenefitReview {
                    kind,
                    transaction_id: item.id.clone(),
                    benefit_id: Some(item.benefit_id.clone()),
                    period_start: item
                        .credit_window
                        .or_else(|| Some(rule.period(item.date).start)),
                    suggestion_id: suggestion,
                },
            ));
        }
        let classified = matched
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();
        for transaction in own.iter().filter(|transaction| {
            transaction.amount > 0.0
                && !transaction.pending
                && !classified.contains(transaction.id.as_str())
                && undecided(&transaction.id)
                && self.is_issuer_credit(&transaction.descriptors())
        }) {
            let Some(date) = calendar::transaction_date(transaction.occurred_date()) else {
                continue;
            };
            items.push((
                date,
                BenefitReview {
                    kind: ReviewKind::UnassignedCredit,
                    transaction_id: transaction.id.clone(),
                    benefit_id: None,
                    period_start: None,
                    suggestion_id: None,
                },
            ));
        }
        items.sort_by(|(left_date, left), (right_date, right)| {
            right_date
                .cmp(left_date)
                .then_with(|| left.transaction_id.cmp(&right.transaction_id))
        });
        items.into_iter().map(|(_, item)| item).collect()
    }

    /// The card charge most likely to have earned an unpaired credit: one at least as large,
    /// posted within the benefit's usual delay before it, preferring the benefit's own
    /// merchant (which may also post just after it), then an exact amount, then the closest
    /// day. Charges already tied to a benefit, or rejected by hand, are skipped.
    fn likely_purchase(
        &self,
        credit: &BenefitActivity,
        rule: &Rule,
        own: &[&Transaction],
        matched: &[BenefitActivity],
    ) -> Option<String> {
        let tied = matched
            .iter()
            .filter(|item| item.kind != ActivityKind::Purchase || item.paired_id.is_some())
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>();
        let amount = minor_units(credit.amount);
        own.iter()
            .filter(|transaction| {
                transaction.amount < 0.0
                    && !transaction.pending
                    && transaction.benefit_confirmed != Some(false)
                    && !tied.contains(transaction.id.as_str())
                    && -minor_units(transaction.amount) >= amount
            })
            .filter_map(|transaction| {
                // Both by transaction date, which the issuer gives a credit from its purchase.
                let date = calendar::transaction_date(transaction.occurred_date())?;
                let days = (credit.date - date).num_days();
                let text = transaction.descriptors();
                let merchant = rule.matches.is_match(&text)
                    && !rule.excludes.as_ref().is_some_and(|re| re.is_match(&text));
                let earliest = if merchant { -EARLY_CREDIT_DAYS } else { 0 };
                (earliest..=rule.data.typical_delay_days)
                    .contains(&days)
                    .then_some((transaction, merchant, days))
            })
            .min_by_key(|(transaction, merchant, days)| {
                (
                    !merchant,
                    -minor_units(transaction.amount) != amount,
                    days.abs(),
                    transaction.id.clone(),
                )
            })
            .map(|(transaction, _, _)| transaction.id.clone())
    }
}

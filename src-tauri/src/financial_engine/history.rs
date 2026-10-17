//! Balance, holding, and net-worth history reconstruction.
use super::*;

pub(super) fn parse_points(
    values: &[Value],
    label: &str,
    warnings: &mut Vec<String>,
) -> Vec<(String, Decimal)> {
    let mut points = parse_records::<HistoryPoint>(values, label, warnings)
        .into_iter()
        .filter_map(|point| {
            Some((
                iso_date(Some(&point.date))?,
                point.total_value.or(point.value)?,
            ))
        })
        .collect::<Vec<_>>();
    points.sort_by(|left, right| left.0.cmp(&right.0));
    points.dedup_by(|left, right| {
        if left.0 == right.0 {
            left.1 = right.1;
            true
        } else {
            false
        }
    });
    points
}

pub(super) fn holding_history(
    mut acquisitions: Vec<Acquisition>,
    prices: &[(String, Decimal)],
) -> Vec<(String, Decimal)> {
    acquisitions.sort_by(|left, right| left.date.cmp(&right.date));
    let mut cursor = 0;
    let mut shares = Decimal::ZERO;
    prices
        .iter()
        .map(|(date, price)| {
            while cursor < acquisitions.len() && acquisitions[cursor].date <= *date {
                shares += acquisitions[cursor].quantity;
                cursor += 1;
            }
            (date.clone(), rounded(shares * *price))
        })
        .collect()
}

/// Sums value histories from the earliest date any of them starts. Each history counts from its
/// first point, carrying its last value across dates it skips, so one late-starting account or
/// holding doesn't cut the total's history short. A history of today's value alone says nothing
/// about the past, and counting it from today would read as a jump, so then there is no total.
pub(super) fn combine_value_histories(
    histories: &[Vec<(String, Decimal)>],
) -> Vec<(String, Decimal)> {
    if histories.is_empty() || histories.iter().any(|history| history.len() < 2) {
        return Vec::new();
    }
    let dates = histories
        .iter()
        .flat_map(|history| history.iter().map(|point| point.0.clone()))
        .collect::<BTreeSet<_>>();
    dates
        .into_iter()
        .map(|date| {
            let value = histories
                .iter()
                .filter_map(|history| {
                    history
                        .iter()
                        .rev()
                        .find(|point| point.0 <= date)
                        .map(|point| point.1)
                })
                .sum();
            (date, rounded(value))
        })
        .collect()
}

/// Adds a point on each external-flow date that a sparse balance history skips, so the
/// carried-forward balance moves with money that left or entered the account that day.
/// Market movement inside the gap stays unknown until the provider's next point.
pub(super) fn with_flow_points(
    points: Vec<(String, Decimal)>,
    flows: &BTreeMap<String, Decimal>,
) -> Vec<(String, Decimal)> {
    let observed = points.into_iter().collect::<BTreeMap<_, _>>();
    let mut filled = observed.clone();
    for date in flows.keys() {
        if observed.contains_key(date) || observed.range(date.clone()..).next().is_none() {
            continue;
        }
        let Some((base_date, base)) = observed.range(..date.clone()).next_back() else {
            continue;
        };
        let moved = flows
            .range(base_date.clone()..=date.clone())
            .filter(|(flow_date, _)| *flow_date != base_date)
            .map(|(_, amount)| *amount)
            .sum::<Decimal>();
        filled.insert(date.clone(), rounded(*base + moved));
    }
    filled.into_iter().collect()
}

pub(super) fn reconstructed_plaid_histories(
    accounts: &[PlaidAccount],
    transactions: &[PlaidTransaction],
    history_starts: &BTreeMap<String, String>,
    imported_starts: &BTreeMap<String, NaiveDate>,
    today: NaiveDate,
) -> BTreeMap<String, Vec<(String, Decimal)>> {
    let earliest_allowed = today - Duration::days(729);
    accounts
        .iter()
        .filter_map(|account| {
            let current = usd_value(
                account.balances.current,
                account.balances.iso_currency_code.as_deref(),
            )?;
            let start = history_starts
                .get(&account.account_id)
                .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
                .or_else(|| {
                    transactions
                        .iter()
                        .filter(|transaction| transaction.account_id == account.account_id)
                        .filter_map(plaid_posted_date)
                        .filter_map(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok())
                        .min()
                })
                .unwrap_or(today)
                .clamp(earliest_allowed, today);
            // Imported statements extend history past Plaid's window.
            let start = imported_starts
                .get(&account.account_id)
                .map_or(start, |imported| start.min(*imported));
            let mut effects = BTreeMap::new();
            for transaction in transactions.iter().filter(|transaction| {
                transaction.account_id == account.account_id && !transaction.pending
            }) {
                let Some(date) = plaid_posted_date(transaction)
                    .and_then(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok())
                else {
                    continue;
                };
                *effects.entry(date).or_default() += rounded(-transaction.amount);
            }
            let mut value = rounded(if account.kind == "credit" && !current.is_zero() {
                -current
            } else {
                current
            });
            let mut points = Vec::new();
            for offset in 0..=(today - start).num_days() {
                let date = today - Duration::days(offset);
                points.push((date.to_string(), value));
                if let Some(effect) = effects.get(&date) {
                    value = rounded(value - effect);
                }
            }
            points.reverse();
            Some((account.account_id.clone(), points))
        })
        .collect()
}

pub(super) fn reconstructed_net_worth_history(
    mut histories: Vec<Vec<(String, Decimal)>>,
    investment_accounts: &[Performance],
    flows: &BTreeMap<String, BTreeMap<String, Decimal>>,
) -> Vec<(String, Decimal)> {
    for account in investment_accounts {
        if account.history_source.as_deref() == Some("unavailable") {
            return Vec::new();
        }
        let Some(points) = account
            .points
            .iter()
            .map(|point| Some((iso_date(Some(&point.date))?, decimal_f64(point.value)?)))
            .collect::<Option<Vec<_>>>()
        else {
            return Vec::new();
        };
        if points.len() < 2 {
            return Vec::new();
        }
        histories.push(match flows.get(&account.account_id) {
            Some(flows) => with_flow_points(points, flows),
            None => points,
        });
    }
    combine_value_histories(&histories)
}

/// Money moving between two tracked accounts often posts on different days, so for a while it
/// appears in both accounts or in neither. Returns, for each date before a transfer's second
/// leg posts, the amount its first leg counted early, from the legs the policy paired.
pub(super) fn in_transit_transfers(
    transactions: &[Transaction],
    accounts: &BTreeSet<String>,
) -> BTreeMap<String, Decimal> {
    let by_id = transactions
        .iter()
        .map(|transaction| (transaction.id.as_str(), transaction))
        .collect::<BTreeMap<_, _>>();
    let leg = |transaction: &Transaction| {
        let account = transaction.account_id.as_deref()?;
        let date = NaiveDate::parse_from_str(transaction.posted_date(), "%Y-%m-%d").ok()?;
        let amount = decimal_f64(transaction.amount)?;
        accounts.contains(account).then_some((date, amount))
    };
    let mut early = BTreeMap::new();
    // Each pair is read once, from its outgoing leg.
    for sent in transactions
        .iter()
        .filter(|transaction| transaction.amount < 0.0)
    {
        let Some(arrived) = sent
            .transfer_pair_id
            .as_deref()
            .and_then(|id| by_id.get(id))
        else {
            continue;
        };
        let (Some(sent), Some(arrived)) = (leg(sent), leg(arrived)) else {
            continue;
        };
        let (first, last) = if arrived.0 < sent.0 {
            (arrived, sent)
        } else {
            (sent, arrived)
        };
        let mut date = first.0;
        while date < last.0 {
            *early.entry(date.to_string()).or_default() += first.1;
            date += Duration::days(1);
        }
    }
    early
}

/// How far the reconstructed history strays from the net worth Brief saw at each refresh, on
/// the days both exist. Brokerage history is estimated, so small gaps are normal; a gap counts
/// only beyond both an amount and a share of the observed value.
pub(super) fn history_check(
    estimated: &[(String, Decimal)],
    observed: &[Point],
) -> Option<HistoryCheck> {
    let estimates = estimated.iter().cloned().collect::<BTreeMap<_, _>>();
    let mut compared = 0;
    let mut divergent = 0;
    let mut largest: Option<(String, Decimal)> = None;
    for (date, seen) in point_values(observed) {
        let Some(estimate) = estimates.get(&date) else {
            continue;
        };
        compared += 1;
        let gap = rounded(*estimate - seen);
        if diverges(*estimate, seen) {
            divergent += 1;
            if largest
                .as_ref()
                .is_none_or(|(_, value)| gap.abs() > value.abs())
            {
                largest = Some((date, gap));
            }
        }
    }
    (compared > 0).then(|| HistoryCheck {
        compared_days: compared,
        divergent_days: divergent,
        largest_gap: largest.as_ref().map(|(_, gap)| money(*gap)),
        largest_gap_date: largest.map(|(date, _)| date),
    })
}

/// Whether an estimate is further from what Brief saw than both $100 and 2% of it.
fn diverges(estimate: Decimal, seen: Decimal) -> bool {
    let gap = (estimate - seen).abs();
    gap > Decimal::ONE_HUNDRED && gap * Decimal::from(50) > seen.abs()
}

/// Each point with a valid date, as a decimal.
fn point_values(points: &[Point]) -> impl Iterator<Item = (String, Decimal)> + '_ {
    points
        .iter()
        .filter_map(|point| Some((iso_date(Some(&point.date))?, decimal_f64(point.value)?)))
}

/// The latest 730 of these dated values.
fn latest(values: BTreeMap<String, Decimal>) -> Vec<(String, Decimal)> {
    let skip = values.len().saturating_sub(730);
    values.into_iter().skip(skip).collect()
}

fn points(values: Vec<(String, Decimal)>) -> Vec<Point> {
    values
        .into_iter()
        .map(|(date, value)| Point {
            date,
            value: money(value),
        })
        .collect()
}

/// The estimated history with what Brief saw at each refresh in place of the estimate. An
/// observation that diverges from that day's estimate yields to it: a refresh can catch one
/// side of a transfer before the other, while the estimate dates both legs. `history_check`
/// still reports the gap.
pub(super) fn merged_net_worth_history(
    estimated: Vec<(String, Decimal)>,
    observed: &[Point],
    in_transit: &BTreeMap<String, Decimal>,
) -> (Vec<Point>, bool) {
    let mut history = estimated.into_iter().collect::<BTreeMap<_, _>>();
    let mut observed_dates = BTreeSet::new();
    for (date, seen) in point_values(observed) {
        if history
            .get(&date)
            .is_none_or(|estimate| !diverges(*estimate, seen))
        {
            observed_dates.insert(date.clone());
            history.insert(date, seen);
        }
    }
    let history = latest(history);
    let has_estimates = history
        .iter()
        .any(|(date, _)| !observed_dates.contains(date));
    let settled = history
        .into_iter()
        .map(|(date, value)| {
            let moving = in_transit.get(&date).copied().unwrap_or_default();
            (date, value - moving)
        })
        .collect();
    (points(settled), has_estimates)
}

pub(super) fn observed_history(
    previous: &Snapshot,
    current: Decimal,
    calendar_date: NaiveDate,
    fresh: bool,
) -> Vec<Point> {
    let saved = if !previous.observed_net_worth_history.is_empty() {
        &previous.observed_net_worth_history
    } else if !previous.net_worth_history_estimated {
        &previous.net_worth_history
    } else {
        &Vec::new()
    };
    let mut observations = point_values(saved).collect::<BTreeMap<_, _>>();
    if fresh {
        observations.insert(calendar_date.to_string(), rounded(current));
    }
    points(latest(observations))
}

pub(super) fn snapshot_change(
    previous: &Snapshot,
    current_accounts: &[Account],
    current_transactions: &[Transaction],
    refreshed: &BTreeSet<String>,
    observed_at: &str,
    current_incomplete: bool,
) -> Option<Change> {
    let previous_accounts = previous
        .accounts
        .iter()
        .filter(|account| account.id != "all")
        .map(|account| (account.id.as_str(), account))
        .collect::<BTreeMap<_, _>>();
    if previous_accounts.is_empty() || previous.updated_at == observed_at {
        return None;
    }
    // Transactions are compared only on a refresh that read providers, in the Plaid cash and
    // card accounts that refresh read; a re-projection of saved data changes nothing real.
    let checked = current_accounts
        .iter()
        .filter(|account| {
            account.id.starts_with("plaid:")
                && matches!(account.r#type.as_str(), "cash" | "credit")
                && refreshed.contains(&account.id)
                && previous_accounts.contains_key(account.id.as_str())
        })
        .map(|account| account.id.clone())
        .collect::<BTreeSet<_>>();
    let diff = (!refreshed.is_empty())
        .then(|| transaction_diff(previous, current_transactions, &checked, observed_at));
    let explained = |account: &Account| {
        diff.as_ref()
            .and_then(|diff| diff.explained.get(&account.id))
            .copied()
    };
    let mut changes = current_accounts
        .iter()
        .filter(|account| account.id != "all")
        .filter_map(|account| {
            let prior = previous_accounts
                .get(account.id.as_str())?
                .value
                .and_then(decimal_f64)?;
            let current = account.value.and_then(decimal_f64)?;
            if !refreshed.is_empty() && !refreshed.contains(&account.id) {
                return None;
            }
            let change = rounded(current - prior);
            let explained = explained(account).map(rounded);
            (change.abs() >= Decimal::new(1, 2)
                || explained.is_some_and(|explained| explained.abs() >= Decimal::new(1, 2)))
            .then_some((account, change, explained))
        })
        .collect::<Vec<_>>();
    changes.sort_by_key(|(_, change, _)| std::cmp::Reverse(change.abs()));
    let previous_transaction_ids = previous
        .transactions
        .iter()
        .map(|transaction| transaction.id.as_str())
        .collect::<BTreeSet<_>>();
    let current_ids = current_accounts
        .iter()
        .map(|account| account.id.as_str())
        .filter(|id| *id != "all")
        .collect::<BTreeSet<_>>();
    let comparison_complete = !previous.net_worth_incomplete
        && !current_incomplete
        && current_ids.len() == previous_accounts.len()
        && current_ids
            .iter()
            .all(|id| previous_accounts.contains_key(*id))
        && (refreshed.is_empty() || current_ids.iter().all(|id| refreshed.contains(*id)));
    let (transaction_changes, backfilled) = diff
        .map(|diff| (diff.changes, diff.backfilled))
        .unwrap_or_default();
    Some(Change {
        observed_at: observed_at.into(),
        previous_updated_at: previous.updated_at.clone(),
        previous_net_worth: previous.net_worth,
        net_worth_change: money(changes.iter().map(|(_, change, _)| *change).sum()),
        comparison_complete: Some(comparison_complete),
        account_changes: changes
            .into_iter()
            .map(|(account, change, explained)| AccountChange {
                account_id: account.id.clone(),
                name: account.name.clone(),
                change: money(change),
                explained: explained.map(money),
            })
            .collect(),
        new_transaction_ids: current_transactions
            .iter()
            .filter(|transaction| !previous_transaction_ids.contains(transaction.id.as_str()))
            .map(|transaction| transaction.id.clone())
            .collect(),
        transaction_changes,
        backfilled,
    })
}

/// The saved account movements with this refresh's changes, the latest 2,000.
pub(super) fn merge_movements(
    previous: &Snapshot,
    change: Option<&Change>,
) -> Vec<AccountMovement> {
    let mut movements = previous
        .account_movements
        .iter()
        .map(|movement| (movement.id.clone(), movement.clone()))
        .collect::<BTreeMap<_, _>>();
    if let Some(change) = change {
        for account in &change.account_changes {
            let id = format!("{}:{}", change.observed_at, account.account_id);
            movements.insert(
                id.clone(),
                AccountMovement {
                    id,
                    observed_at: change.observed_at.clone(),
                    account_id: account.account_id.clone(),
                    name: account.name.clone(),
                    change: account.change,
                    explained: account.explained,
                },
            );
        }
    }
    let mut movements = movements.into_values().collect::<Vec<_>>();
    movements.sort_by(|left, right| left.observed_at.cmp(&right.observed_at));
    let skip = movements.len().saturating_sub(2_000);
    movements.split_off(skip)
}

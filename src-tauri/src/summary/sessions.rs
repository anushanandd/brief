//! Each session's move in layers, kept. A session's record holds every holding's split into
//! market, group, and own (in percent and, when its shares are known, dollars), the fit behind
//! it, the groups that moved, and counts for calibration. A period's layers are the sum of its
//! sessions' records, so today, the week, and a holding's page all use one model.
//!
//! Records are computed from daily closes with each session's fit measured on the days before
//! it, so past sessions can be filled from history. Closed sessions are saved in the finance
//! database (`storage`), where they stay as computed; today's stays provisional until it closes.
use super::layers::{self, Fit, FitMeta, Returns};
use super::*;
use crate::insights::reconcile::{close_on, Shares};
use crate::numeric::round2;

/// Fits are refreshed this often across a run of sessions; a week changes a year's fit little.
const REFIT_SESSIONS: usize = 5;
/// The method that made a record; one made another way is computed again. Method 2 added the
/// quiet state and the session's events.
pub const METHOD: u32 = 2;

/// One session's layers, as saved.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub session: NaiveDate,
    pub method: u32,
    pub computed_at: String,
    /// The S&P 500's change, and that in its typical days.
    pub market_pct: f64,
    pub market_z: f64,
    pub holdings: Vec<HoldingSession>,
    pub groups: Vec<GroupSession>,
    pub calibration: Calibration,
    /// Whether the session was quiet, once its events are known.
    #[serde(default)]
    pub quiet: Option<super::eventlog::Quiet>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HoldingSession {
    pub ticker: String,
    pub change_pct: f64,
    pub market_pct: f64,
    pub group_pct: f64,
    pub own_pct: f64,
    /// The own part in the holding's typical days; `None` without a fit, so a guess is never
    /// called unusual.
    pub own_z: Option<f64>,
    pub sigma: Option<f64>,
    /// The holding's dollars that session and each layer's; `None` when its shares aren't known.
    pub change: Option<f64>,
    pub market: Option<f64>,
    pub group: Option<f64>,
    pub own: Option<f64>,
    pub fit: Option<FitMeta>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GroupSession {
    /// The fund's symbol, or the members joined by `+` for a group without one.
    pub key: String,
    pub fund: Option<String>,
    pub name: Option<String>,
    pub members: Vec<String>,
    pub beyond_pct: f64,
    pub z: f64,
    pub change: Option<f64>,
}

/// What the session flagged, to measure how often the thresholds fire.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SessionCalibration")]
pub struct Calibration {
    pub modeled: usize,
    pub fund_groups: usize,
    pub loose_groups: usize,
    /// Holdings whose own move was at least `layers::OWN_Z` of their typical days.
    pub notable: usize,
}

/// What a session's layers are computed from.
pub(super) struct Inputs<'a> {
    pub returns: &'a BTreeMap<String, Returns>,
    pub closes: &'a DailyCloses,
    /// Each holding's industry fund, the prior for its group.
    pub priors: &'a BTreeMap<String, &'static str>,
    /// Each holding's shares, rebuilt back from the saved trades; absent when unknown.
    pub shares: &'a BTreeMap<String, Shares>,
}

/// Every holding's fit as of `day`.
fn fits(tickers: &[String], inputs: &Inputs, day: NaiveDate) -> Option<BTreeMap<String, Fit>> {
    let market = inputs.returns.get(MARKET_FUND.0)?;
    Some(
        tickers
            .iter()
            .filter_map(|ticker| {
                let stock = inputs.returns.get(ticker)?;
                let prior = inputs.priors.get(ticker).copied();
                let fit = layers::fit(ticker, stock, market, inputs.returns, prior, day)?;
                Some((ticker.clone(), fit))
            })
            .collect(),
    )
}

/// The session's record from `rows` (ticker, change %, dollars when known) with `fits`.
pub(super) fn record(
    day: NaiveDate,
    rows: &[(String, f64, Option<f64>)],
    fits: &BTreeMap<String, Fit>,
    today: &BTreeMap<String, f64>,
    market_history: &Returns,
) -> Option<SessionRecord> {
    let market_pct = *today.get(MARKET_FUND.0)?;
    let history: Vec<f64> = market_history
        .range(..day)
        .map(|(_, value)| *value)
        .collect();
    let sigma = closes::robust_sigma(&history[history.len().saturating_sub(250)..]);
    let priced: Vec<(String, f64, f64)> = rows
        .iter()
        .map(|(ticker, pct, dollars)| (ticker.clone(), *pct, dollars.unwrap_or(0.0)))
        .collect();
    let splits = layers::split(&priced, fits, today);
    let groups = layers::groups(&splits, fits, today);
    let known: BTreeMap<&String, bool> = rows
        .iter()
        .map(|(ticker, _, dollars)| (ticker, dollars.is_some()))
        .collect();
    let holdings: Vec<HoldingSession> = splits
        .iter()
        .map(|split| {
            let fit = fits.get(&split.ticker);
            let dollars = known.get(&split.ticker).copied().unwrap_or(false);
            let money = |value: f64| dollars.then_some(value);
            HoldingSession {
                ticker: split.ticker.clone(),
                change_pct: split.change_pct,
                market_pct: split.market_pct,
                group_pct: split.group_pct,
                own_pct: split.own_pct,
                own_z: fit.map(|_| split.own_z),
                sigma: fit.map(|fit| round2(fit.sigma)),
                change: money(split.change),
                market: money(split.market),
                group: money(split.group),
                own: money(split.own),
                fit: fit.map(Fit::meta),
            }
        })
        .collect();
    let calibration = Calibration {
        modeled: holdings.iter().filter(|item| item.fit.is_some()).count(),
        fund_groups: groups.iter().filter(|group| group.fund.is_some()).count(),
        loose_groups: groups.iter().filter(|group| group.fund.is_none()).count(),
        notable: holdings
            .iter()
            .filter(|item| item.own_z.is_some_and(|z| z.abs() >= layers::OWN_Z))
            .count(),
    };
    Some(SessionRecord {
        session: day,
        method: METHOD,
        computed_at: Utc::now().to_rfc3339(),
        market_pct: round2(market_pct),
        market_z: (market_pct / sigma * 10.0).round() / 10.0,
        groups: groups
            .iter()
            .map(|group| GroupSession {
                key: match group.fund {
                    Some((symbol, _)) => symbol.to_owned(),
                    None => group.members.join("+"),
                },
                fund: group.fund.map(|(symbol, _)| symbol.to_owned()),
                name: group.fund.map(|(_, name)| name.to_owned()),
                members: group.members.clone(),
                beyond_pct: group.beyond_pct,
                z: group.z,
                change: group
                    .members
                    .iter()
                    .all(|member| known.get(member).copied().unwrap_or(false))
                    .then_some(group.change),
            })
            .collect(),
        holdings,
        calibration,
        quiet: None,
    })
}

/// Records for each session in `days` that `saved` lacks, from closes alone: each holding's
/// change that session, its dollars from the shares held at the session before's close, and
/// fits refreshed every `REFIT_SESSIONS` sessions.
pub(super) fn backfill(
    days: &[NaiveDate],
    tickers: &[String],
    inputs: &Inputs,
    saved: &BTreeMap<NaiveDate, SessionRecord>,
) -> Vec<SessionRecord> {
    let Some(market) = inputs.returns.get(MARKET_FUND.0) else {
        return Vec::new();
    };
    let mut cached: Option<BTreeMap<String, Fit>> = None;
    let mut records = Vec::new();
    for (index, day) in days.iter().enumerate() {
        if saved.get(day).is_some_and(|record| record.method == METHOD) {
            continue;
        }
        if cached.is_none() || index % REFIT_SESSIONS == 0 {
            cached = fits(tickers, inputs, *day);
        }
        let Some(fits) = cached.as_ref() else {
            continue;
        };
        let today: BTreeMap<String, f64> = inputs
            .returns
            .iter()
            .filter_map(|(symbol, series)| Some((symbol.clone(), *series.get(day)?)))
            .collect();
        let rows: Vec<(String, f64, Option<f64>)> = tickers
            .iter()
            .filter_map(|ticker| {
                let pct = *today.get(ticker)?;
                let before = day.pred_opt()?;
                let dollars = inputs.shares.get(ticker).and_then(|shares| {
                    let close = close_on(inputs.closes.get(ticker)?, before)?;
                    Some(shares.at(before) * close * pct / 100.0)
                });
                Some((ticker.clone(), pct, dollars))
            })
            .collect();
        if let Some(record) = self::record(*day, &rows, fits, &today, market) {
            records.push(record);
        }
    }
    records
}

/// A holding's session named by its largest layer: its own part when that was unusual for it,
/// quiet when the whole move was within its typical day, otherwise the larger of its group's
/// and the market's parts. `None` without a fit.
pub fn session_kind(row: &HoldingSession) -> Option<&'static str> {
    let (z, sigma) = (row.own_z?, row.sigma?);
    Some(
        if z.abs() >= layers::OWN_Z && row.own_pct.abs() >= layers::OWN_PCT {
            "ownMove"
        } else if row.change_pct.abs() < sigma {
            "quiet"
        } else if row.group_pct.abs() > row.market_pct.abs() {
            "withSector"
        } else {
            "withMarket"
        },
    )
}

/// One holding's layers summed over a period's sessions.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HoldingPeriod {
    /// Each layer's daily parts added up, in percent.
    pub market_pct: f64,
    pub group_pct: f64,
    pub own_pct: f64,
    /// The own part in typical periods of this length for this holding.
    pub own_z: Option<f64>,
    /// One typical period's own move for this holding, in percent.
    pub typical_pct: Option<f64>,
    /// The fit behind the latest session: its fund, betas, R², and window.
    pub fit: Option<FitMeta>,
    pub sessions: usize,
}

impl HoldingPeriod {
    /// `quiet`, `ownMove`, `withSector`, or `withMarket`: the own part when it was at least one
    /// typical period, quiet when the whole move was under half of one, and otherwise whichever
    /// of the group and the market moved it more.
    pub fn kind(&self) -> Option<&'static str> {
        let z = self.own_z?;
        let typical = self.typical_pct?;
        let whole = self.market_pct + self.group_pct + self.own_pct;
        Some(if z.abs() >= 1.0 {
            "ownMove"
        } else if whole.abs() < 0.5 * typical {
            "quiet"
        } else if self.group_pct.abs() > self.market_pct.abs() {
            "withSector"
        } else {
            "withMarket"
        })
    }
}

/// A holding's sessions in `records`, summed.
pub(super) fn holding_period(records: &[&SessionRecord], ticker: &str) -> Option<HoldingPeriod> {
    let rows: Vec<&HoldingSession> = records
        .iter()
        .filter_map(|record| record.holdings.iter().find(|item| item.ticker == ticker))
        .collect();
    if rows.is_empty() {
        return None;
    }
    let sum = |value: fn(&HoldingSession) -> f64| rows.iter().map(|row| value(row)).sum::<f64>();
    let own_pct = sum(|row| row.own_pct);
    // Typical size grows with the square root of the sessions measured.
    let variance: Option<f64> = rows
        .iter()
        .map(|row| row.sigma.map(|sigma| sigma * sigma))
        .sum();
    let typical = variance.filter(|variance| *variance > 0.0).map(f64::sqrt);
    Some(HoldingPeriod {
        market_pct: round2(sum(|row| row.market_pct)),
        group_pct: round2(sum(|row| row.group_pct)),
        own_pct: round2(own_pct),
        own_z: typical.map(|typical| (own_pct / typical * 10.0).round() / 10.0),
        typical_pct: typical.map(round2),
        fit: rows.last().and_then(|row| row.fit.clone()),
        sessions: rows.len(),
    })
}

/// The portfolio's layers summed over a period's sessions, in dollars.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PeriodLayers {
    /// Sessions summed, and sessions in the period without a record.
    pub sessions: usize,
    pub missing: usize,
    pub market: f64,
    /// Each fund's group layer over the period, largest first.
    pub groups: Vec<PeriodGroup>,
    /// Each holding's own layer over the period, largest dollars first.
    pub own: Vec<PeriodOwn>,
    /// Dollars of holdings without a fit, or whose shares weren't known.
    pub unmodeled: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PeriodGroup {
    pub fund: String,
    pub name: Option<String>,
    pub change: f64,
    pub members: Vec<String>,
    /// Sessions the group moved together beyond the market.
    pub flagged: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PeriodOwn {
    pub ticker: String,
    pub change: f64,
    pub own_pct: f64,
    pub z: Option<f64>,
    /// The latest session's fit, so a split from a weak or short fit reads as rough.
    pub fit: Option<FitMeta>,
}

/// The period's layers from its sessions' records.
pub(super) fn period_layers(records: &[&SessionRecord], expected: usize) -> PeriodLayers {
    let mut market = 0.0;
    let mut unmodeled = 0.0;
    let mut groups: BTreeMap<String, PeriodGroup> = BTreeMap::new();
    let mut own: BTreeMap<String, f64> = BTreeMap::new();
    let mut fits: BTreeMap<String, FitMeta> = BTreeMap::new();
    for record in records {
        for row in &record.holdings {
            let (Some(change), Some(fit)) = (row.change, &row.fit) else {
                unmodeled += row.change.unwrap_or(0.0);
                continue;
            };
            let _ = change;
            market += row.market.unwrap_or(0.0);
            *own.entry(row.ticker.clone()).or_default() += row.own.unwrap_or(0.0);
            fits.insert(row.ticker.clone(), fit.clone());
            if let Some(fund) = &fit.fund {
                let group = groups.entry(fund.clone()).or_insert_with(|| PeriodGroup {
                    fund: fund.clone(),
                    name: None,
                    change: 0.0,
                    members: Vec::new(),
                    flagged: 0,
                });
                group.change += row.group.unwrap_or(0.0);
                if !group.members.contains(&row.ticker) {
                    group.members.push(row.ticker.clone());
                }
            }
        }
        for flagged in &record.groups {
            if let Some(group) = flagged.fund.as_ref().and_then(|fund| groups.get_mut(fund)) {
                group.flagged += 1;
                group.name = group.name.clone().or_else(|| flagged.name.clone());
            }
        }
    }
    let mut groups: Vec<PeriodGroup> = groups
        .into_values()
        .map(|group| PeriodGroup {
            change: round2(group.change),
            ..group
        })
        .collect();
    groups.sort_by(|a, b| b.change.abs().total_cmp(&a.change.abs()));
    let mut own: Vec<PeriodOwn> = own
        .into_iter()
        .map(|(ticker, change)| {
            let period = holding_period(records, &ticker);
            PeriodOwn {
                change: round2(change),
                own_pct: period.as_ref().map_or(0.0, |period| period.own_pct),
                z: period.and_then(|period| period.own_z),
                fit: fits.get(&ticker).cloned(),
                ticker,
            }
        })
        .collect();
    own.sort_by(|a, b| b.change.abs().total_cmp(&a.change.abs()));
    PeriodLayers {
        sessions: records.len(),
        missing: expected.saturating_sub(records.len()),
        market: round2(market),
        groups,
        own,
        unmodeled: round2(unmodeled),
    }
}

/// A holding's own part by session and its typical size then, from its records.
pub(super) fn own_series(
    records: &[&SessionRecord],
    ticker: &str,
) -> BTreeMap<NaiveDate, (f64, f64)> {
    records
        .iter()
        .filter_map(|record| {
            let row = record.holdings.iter().find(|item| item.ticker == ticker)?;
            Some((record.session, (row.own_pct, row.sigma?)))
        })
        .collect()
}

/// How often the thresholds fired across saved sessions, to measure them before tuning them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "Calibration")]
pub struct CalibrationSummary {
    pub sessions: usize,
    pub first: Option<NaiveDate>,
    pub last: Option<NaiveDate>,
    /// Per session, on average.
    pub fund_groups: f64,
    pub loose_groups: f64,
    pub notable: f64,
    /// Each fund's flags per 21 sessions it had members: the research asked for about one.
    pub per_fund: Vec<FundRate>,
    /// Sessions that were quiet for the portfolio.
    pub quiet: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FundRate {
    pub fund: String,
    pub flags: usize,
    pub sessions: usize,
    pub per_month: f64,
}

pub fn calibration(records: &[SessionRecord]) -> CalibrationSummary {
    let quiet = records
        .iter()
        .filter(|record| record.quiet.as_ref().is_some_and(|quiet| quiet.portfolio))
        .count();
    let sessions = records.len();
    if sessions == 0 {
        return CalibrationSummary::default();
    }
    let average = |value: fn(&Calibration) -> usize| {
        let total: usize = records
            .iter()
            .map(|record| value(&record.calibration))
            .sum();
        round2(total as f64 / sessions as f64)
    };
    let mut funds: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for record in records {
        let present: BTreeSet<&String> = record
            .holdings
            .iter()
            .filter_map(|row| row.fit.as_ref()?.fund.as_ref())
            .collect();
        for fund in present {
            funds.entry(fund.clone()).or_default().1 += 1;
        }
        for group in &record.groups {
            if let Some(fund) = &group.fund {
                funds.entry(fund.clone()).or_default().0 += 1;
            }
        }
    }
    CalibrationSummary {
        sessions,
        quiet,
        first: records.iter().map(|record| record.session).min(),
        last: records.iter().map(|record| record.session).max(),
        fund_groups: average(|c| c.fund_groups),
        loose_groups: average(|c| c.loose_groups),
        notable: average(|c| c.notable),
        per_fund: funds
            .into_iter()
            .filter(|(_, (_, present))| *present > 0)
            .map(|(fund, (flags, present))| FundRate {
                per_month: round2(flags as f64 / present as f64 * 21.0),
                fund,
                flags,
                sessions: present,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(n: i64) -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap() + Duration::days(n)
    }

    /// A repeatable stream of pseudo-random daily moves, about 1% each.
    fn noise(seed: u64, n: usize) -> Vec<f64> {
        let mut state = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (0..n)
            .map(|_| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((state >> 33) as f64 / (1u64 << 31) as f64 - 0.5) * 3.4
            })
            .collect()
    }

    const N: usize = 330;

    fn series(values: &[f64]) -> Returns {
        values
            .iter()
            .enumerate()
            .map(|(i, value)| (day(i as i64), *value))
            .collect()
    }

    /// Closes that compound `returns` from 100.
    fn closes_of(returns: &Returns) -> Vec<(NaiveDate, f64)> {
        let mut price = 100.0;
        let mut out = vec![(day(-1), price)];
        for (date, r) in returns {
            price *= 1.0 + r / 100.0;
            out.push((*date, price));
        }
        out
    }

    /// A market, a chip fund, two chip stocks, and a steady stock.
    fn world() -> (BTreeMap<String, Returns>, DailyCloses) {
        let market = noise(1, N);
        let chips = noise(2, N);
        let mut returns = BTreeMap::new();
        returns.insert("SPY".to_string(), series(&market));
        returns.insert(
            "SMH".to_string(),
            series(
                &market
                    .iter()
                    .zip(&chips)
                    .map(|(m, c)| 1.2 * m + c)
                    .collect::<Vec<_>>(),
            ),
        );
        for (ticker, seed) in [("CHIPA", 7), ("CHIPB", 8)] {
            let own = noise(seed, N);
            returns.insert(
                ticker.to_string(),
                series(
                    &(0..N)
                        .map(|i| 1.4 * market[i] + chips[i] + 0.5 * own[i])
                        .collect::<Vec<_>>(),
                ),
            );
        }
        let own = noise(9, N);
        returns.insert(
            "STDY".to_string(),
            series(
                &(0..N)
                    .map(|i| 0.8 * market[i] + 0.4 * own[i])
                    .collect::<Vec<_>>(),
            ),
        );
        let closes = returns
            .iter()
            .map(|(symbol, series)| (symbol.clone(), closes_of(series)))
            .collect();
        (returns, closes)
    }

    fn shares(now: f64) -> Shares {
        Shares::rebuild("X", now, &[], day(0)).unwrap()
    }

    #[test]
    fn past_sessions_fill_from_closes_and_their_layers_add_up_in_dollars() {
        let (returns, closes) = world();
        let tickers: Vec<String> = ["CHIPA", "CHIPB", "STDY"].map(String::from).to_vec();
        let priors = BTreeMap::from([("CHIPA".to_string(), "XLK")]);
        let held = BTreeMap::from([
            ("CHIPA".to_string(), shares(10.0)),
            ("STDY".to_string(), shares(20.0)),
        ]);
        let inputs = Inputs {
            returns: &returns,
            closes: &closes,
            priors: &priors,
            shares: &held,
        };
        let days: Vec<NaiveDate> = (300..310).map(day).collect();
        let records = backfill(&days, &tickers, &inputs, &BTreeMap::new());
        assert_eq!(records.len(), 10);
        let first = &records[0];
        let chip = first
            .holdings
            .iter()
            .find(|row| row.ticker == "CHIPA")
            .unwrap();
        assert_eq!(chip.fit.as_ref().unwrap().fund.as_deref(), Some("SMH"));
        // The layers add up to the holding's dollars that session.
        let total = chip.market.unwrap() + chip.group.unwrap() + chip.own.unwrap();
        assert!(
            (total - chip.change.unwrap()).abs() < 0.011,
            "{total} {:?}",
            chip.change
        );
        // Without known shares, the percents are kept and the dollars aren't invented.
        let other = first
            .holdings
            .iter()
            .find(|row| row.ticker == "CHIPB")
            .unwrap();
        assert_eq!(other.change, None);
        assert!(other.own_z.is_some());
        // A saved record of this method isn't computed again.
        let saved = BTreeMap::from([(records[0].session, records[0].clone())]);
        assert_eq!(backfill(&days, &tickers, &inputs, &saved).len(), 9);
        let mut old = records[0].clone();
        old.method = METHOD + 1;
        let stale = BTreeMap::from([(old.session, old)]);
        assert_eq!(backfill(&days, &tickers, &inputs, &stale).len(), 10);

        // A period's layers are its sessions' summed.
        let refs: Vec<&SessionRecord> = records.iter().collect();
        let layers = period_layers(&refs, 12);
        assert_eq!((layers.sessions, layers.missing), (10, 2));
        let market: f64 = records
            .iter()
            .flat_map(|record| &record.holdings)
            .filter(|row| row.fit.is_some())
            .filter_map(|row| row.market)
            .sum();
        assert!((layers.market - market).abs() < 0.011);
        assert!(layers.groups.iter().any(|group| group.fund == "SMH"));
        let chip = holding_period(&refs, "CHIPA").unwrap();
        assert_eq!(chip.sessions, 10);
        let own: f64 = records
            .iter()
            .flat_map(|record| &record.holdings)
            .filter(|row| row.ticker == "CHIPA")
            .map(|row| row.own_pct)
            .sum();
        assert!((chip.own_pct - own).abs() < 0.011);
        assert!(chip.kind().is_some());
    }

    #[test]
    fn a_period_kind_follows_the_layers_not_the_benchmark_chosen() {
        let period = |market_pct: f64, group_pct: f64, own_pct: f64| HoldingPeriod {
            market_pct,
            group_pct,
            own_pct,
            own_z: Some(own_pct / 4.0),
            typical_pct: Some(4.0),
            fit: None,
            sessions: 20,
        };
        assert_eq!(period(0.5, 0.3, 0.2).kind(), Some("quiet"));
        assert_eq!(period(-6.0, 1.0, 1.0).kind(), Some("withMarket"));
        // A sector fund chosen as the group doesn't make a move "with its sector": the group
        // layer has to be the larger.
        assert_eq!(period(-3.0, -5.0, 1.0).kind(), Some("withSector"));
        assert_eq!(period(-1.0, -1.0, -9.0).kind(), Some("ownMove"));
        let unfit = HoldingPeriod {
            own_z: None,
            ..period(1.0, 0.0, 0.0)
        };
        assert_eq!(unfit.kind(), None);
    }

    #[test]
    fn calibration_counts_flags_per_session_and_per_fund_month() {
        let (returns, closes) = world();
        let tickers: Vec<String> = ["CHIPA", "CHIPB"].map(String::from).to_vec();
        let priors = BTreeMap::new();
        let held = BTreeMap::new();
        let inputs = Inputs {
            returns: &returns,
            closes: &closes,
            priors: &priors,
            shares: &held,
        };
        let days: Vec<NaiveDate> = (270..330).map(day).collect();
        let records = backfill(&days, &tickers, &inputs, &BTreeMap::new());
        let summary = calibration(&records);
        assert_eq!(summary.sessions, 60);
        assert_eq!(summary.first, Some(day(270)));
        let smh = summary
            .per_fund
            .iter()
            .find(|rate| rate.fund == "SMH")
            .unwrap();
        assert_eq!(smh.sessions, 60);
        // A fund at one of its typical days flags on roughly a third of sessions when most
        // members follow, which is what the log exists to measure.
        let flags: usize = records
            .iter()
            .map(|record| record.calibration.fund_groups)
            .sum();
        assert_eq!(smh.flags, flags);
        assert!((summary.fund_groups - flags as f64 / 60.0).abs() < 0.01);
        assert_eq!(calibration(&[]), CalibrationSummary::default());
    }
}

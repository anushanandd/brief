//! Why the portfolio moved today, in layers. Each holding's day is split into what the market
//! explains (its beta times the S&P 500), what its group explains (its beta to the fund that best
//! fits its moves beyond the market, times that fund's move beyond the market), and the rest, its
//! own. The layers are summed in dollars, so the summary can lead with whichever moved the
//! portfolio most instead of the largest single loser.
//!
//! The model is a time-series factor model on observable funds, as risk models use for small
//! universes: the fund's return is first stripped of the market, so the market and the group
//! never claim the same move. Groups are found from prices, not industry codes: a holding joins
//! the fund among the sector and theme funds whose moves beyond the market track its own best in
//! both halves of the year, and the SEC industry code's fund wins unless another fits clearly
//! better. Holdings that moved far outside their own normal together, with no fund that fits,
//! form a group of their own.
use super::*;
use crate::numeric::round2;
use crate::providers::closes::robust_sigma;

/// Funds a holding's group may be measured against: the eleven sector funds and themes the
/// sector funds split badly (chips trade apart from software inside Technology).
pub(super) const GROUP_FUNDS: &[(&str, &str)] = &[
    ("XLK", "Technology"),
    ("XLV", "Health care"),
    ("XLF", "Financials"),
    ("XLE", "Energy"),
    ("XLI", "Industrials"),
    ("XLY", "Consumer discretionary"),
    ("XLP", "Consumer staples"),
    ("XLU", "Utilities"),
    ("XLB", "Materials"),
    ("XLRE", "Real estate"),
    ("XLC", "Communication services"),
    ("SMH", "Semiconductor"),
    ("IGV", "Software"),
];
/// Days of history behind each fit; today is never part of it.
const FIT_DAYS: usize = 250;
/// Fewer shared days than this cannot fit a holding's layers.
const MIN_FIT_DAYS: usize = 60;
/// Below this many days a beta is shrunk toward its prior, in proportion to the days missing:
/// a short history's estimate is mostly noise.
const FULL_WEIGHT_DAYS: usize = 120;
/// A fund explains a holding's moves beyond the market only when it accounts for at least this
/// share of their variance in both halves of the year.
const MIN_GROUP_FIT: f64 = 0.10;
/// Another fund replaces the industry code's only when it fits this much better.
const GROUP_FIT_MARGIN: f64 = 0.05;
/// A fund's day beyond the market this many of its typical days is a group move. Not a test of
/// the unusual: an ordinary down day for chips still costs a chip-heavy portfolio, and the
/// summary should say where the money went.
pub(super) const GROUP_Z: f64 = 1.0;
/// At least this share of a group must have moved beyond the market in the fund's direction.
const GROUP_BREADTH: f64 = 0.7;
/// A holding's own move this many of its typical days is unusual for it.
pub(super) const OWN_Z: f64 = 2.0;
/// A holding's own move this many of its typical days is named, when its dollars matter.
/// An own move smaller than this is never named, however steady the stock.
pub(super) const OWN_PCT: f64 = 1.0;
/// Holdings that moved together without a fund: at least this many, each at least
/// `LOOSE_Z` of its typical days the same way, whose own moves have tended to go together, and
/// whose average own move is at least `GROUP_Z` of that average's typical day.
const LOOSE_GROUP: usize = 2;
const LOOSE_Z: f64 = 0.75;
const LOOSE_CORRELATION: f64 = 0.15;

pub(super) type Returns = BTreeMap<NaiveDate, f64>;

/// The fund a holding's group is measured against, and how the holding follows it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct GroupFit {
    pub symbol: &'static str,
    pub name: &'static str,
    /// The holding's move for each 1% the fund moves beyond the market.
    pub beta: f64,
    /// The fund's own beta to the market, which its move beyond the market removes.
    pub fund_beta: f64,
    /// The fund's typical day beyond the market.
    pub fund_sigma: f64,
    /// The smaller of the two halves' shares of variance explained.
    pub fit: f64,
}

/// One holding's layers, fitted on the days before today.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Fit {
    pub market_beta: f64,
    pub group: Option<GroupFit>,
    /// The typical day left after the market and the group.
    pub sigma: f64,
    /// What neither explained, by day: for telling which holdings tend to move together.
    pub residuals: Returns,
    /// The share of the holding's daily variance the market and its group explain.
    pub r_squared: f64,
    /// Days the fit was measured over.
    pub window: usize,
    /// The betas were shrunk toward their priors for want of history.
    pub shrunk: bool,
}

/// What a layer figure was measured with, so the surface can say how much to trust it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitMeta {
    pub market_beta: f64,
    pub fund: Option<String>,
    pub group_beta: Option<f64>,
    pub r_squared: f64,
    pub window: usize,
    pub shrunk: bool,
}

impl Fit {
    pub fn meta(&self) -> FitMeta {
        FitMeta {
            market_beta: self.market_beta,
            fund: self.group.as_ref().map(|group| group.symbol.to_owned()),
            group_beta: self.group.as_ref().map(|group| group.beta),
            r_squared: self.r_squared,
            window: self.window,
            shrunk: self.shrunk,
        }
    }
}

/// An estimate from `days` of history pulled toward `prior` when the history is short.
fn shrink(estimate: f64, prior: f64, days: usize) -> f64 {
    let weight = (days as f64 / FULL_WEIGHT_DAYS as f64).min(1.0);
    weight * estimate + (1.0 - weight) * prior
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len().max(1) as f64
}

/// The slope of `y` on `x`, or `None` when `x` never moved.
fn slope(x: &[f64], y: &[f64]) -> Option<f64> {
    let (mx, my) = (mean(x), mean(y));
    let variance: f64 = x.iter().map(|x| (x - mx).powi(2)).sum();
    (variance > 0.0).then(|| {
        x.iter()
            .zip(y)
            .map(|(x, y)| (x - mx) * (y - my))
            .sum::<f64>()
            / variance
    })
}

pub(super) fn correlation(x: &[f64], y: &[f64]) -> Option<f64> {
    let (mx, my) = (mean(x), mean(y));
    let sx: f64 = x.iter().map(|x| (x - mx).powi(2)).sum();
    let sy: f64 = y.iter().map(|y| (y - my).powi(2)).sum();
    (sx > 0.0 && sy > 0.0).then(|| {
        x.iter()
            .zip(y)
            .map(|(x, y)| (x - mx) * (y - my))
            .sum::<f64>()
            / (sx * sy).sqrt()
    })
}

/// The last `FIT_DAYS` days before `today` on which every series has a return, oldest first.
fn shared_days(series: &[&Returns], today: NaiveDate) -> Vec<NaiveDate> {
    let Some((first, rest)) = series.split_first() else {
        return Vec::new();
    };
    let mut days: Vec<NaiveDate> = first
        .range(..today)
        .map(|(day, _)| *day)
        .filter(|day| rest.iter().all(|series| series.contains_key(day)))
        .collect();
    let skip = days.len().saturating_sub(FIT_DAYS);
    days.drain(..skip);
    days
}

fn values(series: &Returns, days: &[NaiveDate]) -> Vec<f64> {
    days.iter().map(|day| series[day]).collect()
}

/// A fund's move beyond the market on each day, with the fund's beta removed.
fn beyond_market(fund: &[f64], market: &[f64]) -> Option<(f64, Vec<f64>)> {
    let beta = slope(market, fund)?;
    Some((
        beta,
        fund.iter().zip(market).map(|(f, m)| f - beta * m).collect(),
    ))
}

/// How much of the holding's moves beyond the market the fund's explain, in the weaker half of
/// the year, so a fund that fit once and then stopped isn't chosen.
fn group_fit(stock_beyond: &[f64], fund_beyond: &[f64]) -> f64 {
    let half = stock_beyond.len() / 2;
    let explained =
        |a: &[f64], b: &[f64]| correlation(a, b).map_or(0.0, |r| if r > 0.0 { r * r } else { 0.0 });
    explained(&stock_beyond[..half], &fund_beyond[..half])
        .min(explained(&stock_beyond[half..], &fund_beyond[half..]))
}

/// Fits a holding's layers from the days before `today`. The group is the candidate fund that
/// best explains the holding's moves beyond the market; `prior` (the industry code's fund) wins
/// unless another fits better by `GROUP_FIT_MARGIN`. A fund is never its own group.
pub(super) fn fit(
    ticker: &str,
    stock: &Returns,
    market: &Returns,
    funds: &BTreeMap<String, Returns>,
    prior: Option<&str>,
    today: NaiveDate,
) -> Option<Fit> {
    let days = shared_days(&[stock, market], today);
    if days.len() < MIN_FIT_DAYS {
        return None;
    }
    let (s, m) = (values(stock, &days), values(market, &days));
    // A short history's market beta leans toward the industry fund's own beta to the market,
    // or one without a fund.
    let prior_beta = prior
        .and_then(|symbol| funds.get(symbol))
        .filter(|fund| days.iter().all(|day| fund.contains_key(day)))
        .and_then(|fund| slope(&m, &values(fund, &days)))
        .unwrap_or(1.0);
    let market_beta = shrink(slope(&m, &s)?, prior_beta, days.len()).clamp(0.2, 2.5);
    let stock_beyond: Vec<f64> = s.iter().zip(&m).map(|(s, m)| s - market_beta * m).collect();
    let mut candidates: Vec<(GroupFit, Vec<f64>)> = Vec::new();
    for (symbol, name) in GROUP_FUNDS {
        let Some(fund) = funds.get(*symbol).filter(|_| *symbol != ticker) else {
            continue;
        };
        if days.iter().any(|day| !fund.contains_key(day)) {
            continue;
        }
        let Some((fund_beta, beyond)) = beyond_market(&values(fund, &days), &m) else {
            continue;
        };
        let Some(beta) = slope(&beyond, &stock_beyond) else {
            continue;
        };
        // A group other than the industry code's needs history to earn its beta.
        let prior_group_beta = if prior == Some(*symbol) { 1.0 } else { 0.0 };
        candidates.push((
            GroupFit {
                symbol,
                name,
                beta: shrink(beta, prior_group_beta, days.len()).clamp(0.0, 3.0),
                fund_beta,
                fund_sigma: robust_sigma(&beyond),
                fit: group_fit(&stock_beyond, &beyond),
            },
            beyond,
        ));
    }
    let best = candidates
        .iter()
        .max_by(|a, b| a.0.fit.total_cmp(&b.0.fit))
        .map(|candidate| candidate.0.symbol);
    let prior_fit = candidates
        .iter()
        .find(|candidate| Some(candidate.0.symbol) == prior)
        .map(|candidate| candidate.0.fit);
    let chosen = match (best, prior_fit) {
        (Some(best), Some(prior_fit)) => {
            let best_fit = candidates
                .iter()
                .find(|candidate| candidate.0.symbol == best)
                .map_or(0.0, |candidate| candidate.0.fit);
            if best_fit >= prior_fit + GROUP_FIT_MARGIN {
                Some(best)
            } else {
                prior
            }
        }
        (best, _) => best,
    };
    let group = candidates
        .into_iter()
        .find(|candidate| Some(candidate.0.symbol) == chosen)
        .filter(|candidate| candidate.0.fit >= MIN_GROUP_FIT);
    let residual: Vec<f64> = match &group {
        Some((group, beyond)) => stock_beyond
            .iter()
            .zip(beyond)
            .map(|(s, f)| s - group.beta * f)
            .collect(),
        None => stock_beyond,
    };
    let mean_s = mean(&s);
    let total: f64 = s.iter().map(|value| (value - mean_s).powi(2)).sum();
    let mean_r = mean(&residual);
    let left: f64 = residual.iter().map(|value| (value - mean_r).powi(2)).sum();
    let window = days.len();
    Some(Fit {
        market_beta: round2(market_beta),
        sigma: robust_sigma(&residual),
        r_squared: if total > 0.0 {
            round2((1.0 - left / total).max(0.0))
        } else {
            0.0
        },
        window,
        shrunk: window < FULL_WEIGHT_DAYS,
        residuals: days.into_iter().zip(residual).collect(),
        group: group.map(|(group, _)| GroupFit {
            beta: round2(group.beta),
            fund_beta: round2(group.fund_beta),
            fit: round2(group.fit),
            ..group
        }),
    })
}

/// One holding's day, split into layers, in percent and dollars.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Split {
    pub ticker: String,
    pub change_pct: f64,
    pub change: f64,
    pub market_pct: f64,
    pub group_pct: f64,
    pub own_pct: f64,
    pub market: f64,
    pub group: f64,
    pub own: f64,
    /// The own move in the holding's typical days.
    pub own_z: f64,
    pub fund: Option<&'static str>,
}

/// Splits each priced holding's day. `positions` are today's (ticker, change %, dollar change);
/// `today` maps each symbol to its return today. A holding without a fit stays whole in `own`,
/// with no z, so it is never called unusual on a guess.
pub(super) fn split(
    positions: &[(String, f64, f64)],
    fits: &BTreeMap<String, Fit>,
    today: &BTreeMap<String, f64>,
) -> Vec<Split> {
    let market = today.get(MARKET_FUND.0).copied();
    positions
        .iter()
        .map(|(ticker, change_pct, change)| {
            let fit = fits.get(ticker).zip(market);
            let (market_pct, group_pct, own_z, fund) = match fit {
                Some((fit, market)) => {
                    let market_pct = fit.market_beta * market;
                    let group = fit.group.as_ref().and_then(|group| {
                        let fund = today.get(group.symbol)?;
                        Some((group.beta * (fund - group.fund_beta * market), group.symbol))
                    });
                    let group_pct = group.map_or(0.0, |(pct, _)| pct);
                    let own = change_pct - market_pct - group_pct;
                    (
                        market_pct,
                        group_pct,
                        own / fit.sigma,
                        group.map(|(_, symbol)| symbol),
                    )
                }
                None => (0.0, 0.0, 0.0, None),
            };
            // Dollars follow the percentages, and the own part takes the rounding, so the
            // layers always add up to the holding's change.
            let dollars = |pct: f64| {
                if *change_pct == 0.0 {
                    0.0
                } else {
                    round2(change * pct / change_pct)
                }
            };
            let (market, group) = (dollars(market_pct), dollars(group_pct));
            Split {
                ticker: ticker.clone(),
                change_pct: *change_pct,
                change: *change,
                market_pct: round2(market_pct),
                group_pct: round2(group_pct),
                own_pct: round2(change_pct - market_pct - group_pct),
                market,
                group,
                own: round2(change - market - group),
                own_z: round1(own_z),
                fund,
            }
        })
        .collect()
}

/// Holdings that moved together today, beyond the market.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Group {
    /// The fund that names the group; `None` for holdings that moved together without one.
    pub fund: Option<(&'static str, &'static str)>,
    pub members: Vec<String>,
    /// The members' dollars beyond the market: the group's part, and for a group without a
    /// fund, their own moves.
    pub change: f64,
    /// The fund's move beyond the market today, and that in its typical days; for a group
    /// without a fund, the members' average own move and its average size in typical days.
    pub beyond_pct: f64,
    pub z: f64,
}

/// The groups that moved today: a fund's day beyond the market at least `GROUP_Z` of its typical
/// days, followed by most of its members, and holdings that moved the same way beyond the
/// market together, when their own moves have tended to go together.
pub(super) fn groups(
    splits: &[Split],
    fits: &BTreeMap<String, Fit>,
    today: &BTreeMap<String, f64>,
) -> Vec<Group> {
    let Some(market) = today.get(MARKET_FUND.0).copied() else {
        return Vec::new();
    };
    let mut by_fund: BTreeMap<&'static str, Vec<&Split>> = BTreeMap::new();
    for split in splits {
        if let Some(fund) = split.fund {
            by_fund.entry(fund).or_default().push(split);
        }
    }
    let mut found = Vec::new();
    for (symbol, members) in by_fund {
        let Some(group) = members
            .iter()
            .find_map(|member| fits.get(&member.ticker)?.group.clone())
        else {
            continue;
        };
        let Some(fund) = today.get(symbol) else {
            continue;
        };
        let beyond = fund - group.fund_beta * market;
        let z = beyond / group.fund_sigma;
        let along = members
            .iter()
            .filter(|member| (member.change_pct - member.market_pct).signum() == beyond.signum())
            .count();
        if z.abs() < GROUP_Z || (along as f64) < GROUP_BREADTH * members.len() as f64 {
            continue;
        }
        found.push(Group {
            fund: Some((group.symbol, group.name)),
            members: members.iter().map(|member| member.ticker.clone()).collect(),
            change: round2(members.iter().map(|member| member.group).sum()),
            beyond_pct: round2(beyond),
            z: round1(z),
        });
    }
    let grouped: BTreeSet<String> = found
        .iter()
        .flat_map(|group| group.members.iter().cloned())
        .collect();
    for sign in [-1.0, 1.0] {
        let moved: Vec<&Split> = splits
            .iter()
            .filter(|split| {
                !grouped.contains(&split.ticker)
                    && split.own_z.signum() == sign
                    && split.own_z.abs() >= LOOSE_Z
                    && split.own_pct.abs() >= OWN_PCT
            })
            .collect();
        let together: Vec<&Split> = moved
            .iter()
            .filter(|split| {
                let others: Vec<f64> = moved
                    .iter()
                    .filter(|other| other.ticker != split.ticker)
                    .filter_map(|other| {
                        let (a, b) = (fits.get(&split.ticker)?, fits.get(&other.ticker)?);
                        let days: Vec<&NaiveDate> = a
                            .residuals
                            .keys()
                            .filter(|day| b.residuals.contains_key(day))
                            .collect();
                        let x: Vec<f64> = days.iter().map(|day| a.residuals[*day]).collect();
                        let y: Vec<f64> = days.iter().map(|day| b.residuals[*day]).collect();
                        correlation(&x, &y)
                    })
                    .collect();
                !others.is_empty() && mean(&others) >= LOOSE_CORRELATION
            })
            .copied()
            .collect();
        if together.len() < LOOSE_GROUP {
            continue;
        }
        // The members' average own move, against how far that average usually wanders: shared
        // swings make it wander more than independent ones would.
        let days: Vec<NaiveDate> = fits[&together[0].ticker]
            .residuals
            .keys()
            .filter(|day| {
                together
                    .iter()
                    .all(|split| fits[&split.ticker].residuals.contains_key(day))
            })
            .copied()
            .collect();
        if days.len() < MIN_FIT_DAYS {
            continue;
        }
        let basket: Vec<f64> = days
            .iter()
            .map(|day| {
                mean(
                    &together
                        .iter()
                        .map(|split| fits[&split.ticker].residuals[day])
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let n = together.len() as f64;
        let average = together.iter().map(|split| split.own_pct).sum::<f64>() / n;
        let z = average / robust_sigma(&basket);
        if z.abs() < GROUP_Z {
            continue;
        }
        found.push(Group {
            fund: None,
            members: together.iter().map(|split| split.ticker.clone()).collect(),
            change: round2(together.iter().map(|split| split.own).sum()),
            beyond_pct: round2(average),
            z: round1(z),
        });
    }
    found.sort_by(|a, b| b.change.abs().total_cmp(&a.change.abs()));
    found
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
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

    fn series(values: &[f64]) -> Returns {
        values
            .iter()
            .enumerate()
            .map(|(i, value)| (day(i as i64), *value))
            .collect()
    }

    const N: usize = 260;

    /// A market, a chip fund that adds its own swings to the market's, a software fund, and a
    /// stock that follows the chip fund's swings and has its own.
    fn world() -> (Returns, BTreeMap<String, Returns>, Vec<f64>) {
        let market = noise(1, N);
        let chips = noise(2, N);
        let software = noise(3, N);
        let funds = BTreeMap::from([
            (
                "SMH".to_string(),
                series(
                    &market
                        .iter()
                        .zip(&chips)
                        .map(|(m, c)| 1.2 * m + c)
                        .collect::<Vec<_>>(),
                ),
            ),
            (
                "XLK".to_string(),
                series(
                    &market
                        .iter()
                        .zip(&chips)
                        .zip(&software)
                        .map(|((m, c), s)| m + 0.3 * c + 0.3 * s)
                        .collect::<Vec<_>>(),
                ),
            ),
            (
                "IGV".to_string(),
                series(
                    &market
                        .iter()
                        .zip(&software)
                        .map(|(m, s)| m + s)
                        .collect::<Vec<_>>(),
                ),
            ),
        ]);
        (series(&market), funds, chips)
    }

    fn chip_stock(market: &Returns, chips: &[f64], seed: u64) -> Returns {
        let own = noise(seed, N);
        series(
            &(0..N)
                .map(|i| 1.5 * market[&day(i as i64)] + chips[i] + 0.5 * own[i])
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn a_chip_stock_is_grouped_with_chips_though_its_industry_code_says_technology() {
        let (market, funds, chips) = world();
        let stock = chip_stock(&market, &chips, 9);
        let chip = fit("CHIP", &stock, &market, &funds, Some("XLK"), day(N as i64)).unwrap();
        assert!(
            (chip.market_beta - 1.5).abs() < 0.15,
            "{}",
            chip.market_beta
        );
        let group = chip.group.clone().unwrap();
        assert_eq!(group.symbol, "SMH");
        assert!((group.beta - 1.0).abs() < 0.15, "{}", group.beta);
        // A fund never measures itself, and a stock that only follows the market has no group.
        let steady = series(&market.values().map(|m| 0.9 * m).collect::<Vec<_>>());
        let fund = fit("SMH", &funds["SMH"], &market, &funds, None, day(N as i64)).unwrap();
        assert_ne!(fund.group.map(|group| group.symbol), Some("SMH"));
        let steady = fit("STDY", &steady, &market, &funds, None, day(N as i64)).unwrap();
        assert_eq!(steady.group, None);
        // A short history fits nothing.
        let short: Returns = stock.range(..day(30)).map(|(d, v)| (*d, *v)).collect();
        assert_eq!(
            fit("NEW", &short, &market, &funds, None, day(N as i64)),
            None
        );
        // The fit says how much it explains and over how long.
        assert!(
            chip.r_squared > 0.5 && chip.r_squared <= 1.0,
            "{}",
            chip.r_squared
        );
        assert_eq!((chip.window, chip.shrunk), (FIT_DAYS, false));
        assert_eq!(chip.meta().fund.as_deref(), Some("SMH"));
    }

    #[test]
    fn a_short_historys_betas_lean_toward_their_priors() {
        let (market, funds, chips) = world();
        let stock = chip_stock(&market, &chips, 9);
        // Ninety days in: three quarters of the estimate, a quarter of the prior.
        let recent: Returns = stock.range(day(170)..).map(|(d, v)| (*d, *v)).collect();
        let short = fit("CHIP", &recent, &market, &funds, Some("XLK"), day(N as i64)).unwrap();
        let full = fit("CHIP", &stock, &market, &funds, Some("XLK"), day(N as i64)).unwrap();
        assert_eq!(short.window, 90);
        assert!(short.shrunk);
        // XLK's beta to the market is about one, so the chip stock's 1.5 is pulled toward it.
        assert!(
            short.market_beta < full.market_beta,
            "{} {}",
            short.market_beta,
            full.market_beta
        );
        assert!(short.market_beta > 1.0, "{}", short.market_beta);
        assert_eq!(shrink(2.0, 1.0, 60), 1.5);
        assert_eq!(shrink(2.0, 1.0, 240), 2.0);
    }

    #[test]
    fn the_industry_codes_fund_keeps_its_place_unless_another_fits_clearly_better() {
        let (market, funds, chips) = world();
        let software = noise(3, N);
        // A stock split evenly between chips and software fits XLK nearly as well as either.
        let own = noise(11, N);
        let mixed = series(
            &(0..N)
                .map(|i| market[&day(i as i64)] + 0.5 * chips[i] + 0.5 * software[i] + 0.3 * own[i])
                .collect::<Vec<_>>(),
        );
        let fit = fit("MIX", &mixed, &market, &funds, Some("XLK"), day(N as i64)).unwrap();
        assert_eq!(fit.group.unwrap().symbol, "XLK");
    }

    #[test]
    fn todays_move_splits_into_layers_that_add_up_in_dollars() {
        let (market, funds, chips) = world();
        let today_day = day(N as i64);
        let stock = chip_stock(&market, &chips, 9);
        let fits = BTreeMap::from([(
            "CHIP".to_string(),
            fit("CHIP", &stock, &market, &funds, Some("XLK"), today_day).unwrap(),
        )]);
        // The market fell 1%, chips fell 4% beyond it, and the stock fell 6.5%.
        let smh_beta = fits["CHIP"].group.as_ref().unwrap().fund_beta;
        let today = BTreeMap::from([
            ("SPY".to_string(), -1.0),
            ("SMH".to_string(), -4.0 - smh_beta),
            ("CHIP".to_string(), -6.5),
        ]);
        let splits = split(&[("CHIP".into(), -6.5, -650.0)], &fits, &today);
        let s = &splits[0];
        assert_eq!(s.fund, Some("SMH"));
        assert!((s.market_pct + 1.5).abs() < 0.2, "{}", s.market_pct);
        assert!((s.group_pct + 4.0).abs() < 0.6, "{}", s.group_pct);
        assert!((s.market + s.group + s.own - s.change).abs() < 0.011);
        assert!(s.own_z.abs() < OWN_Z);
        // Without a fit the whole move stays the holding's own, never called unusual.
        let whole = split(&[("NEW".into(), -9.0, -90.0)], &fits, &today);
        assert_eq!((whole[0].own, whole[0].own_z), (-90.0, 0.0));
    }

    #[test]
    fn a_group_moved_when_its_fund_was_unusual_and_most_members_went_along() {
        let (market, funds, chips) = world();
        let today_day = day(N as i64);
        let fits: BTreeMap<String, Fit> = [9, 10, 12]
            .iter()
            .map(|seed| {
                let ticker = format!("C{seed}");
                let stock = chip_stock(&market, &chips, *seed);
                let fit = fit(&ticker, &stock, &market, &funds, Some("XLK"), today_day).unwrap();
                (ticker, fit)
            })
            .collect();
        let smh_beta = fits["C9"].group.as_ref().unwrap().fund_beta;
        let sigma = fits["C9"].group.as_ref().unwrap().fund_sigma;
        let day_with = |beyond: f64, moves: [f64; 3]| {
            let mut today = BTreeMap::from([
                ("SPY".to_string(), -0.5),
                ("SMH".to_string(), beyond - 0.5 * smh_beta),
            ]);
            let positions: Vec<(String, f64, f64)> = ["C9", "C10", "C12"]
                .iter()
                .zip(moves)
                .map(|(ticker, change)| {
                    today.insert((*ticker).into(), change);
                    ((*ticker).into(), change, change * 100.0)
                })
                .collect();
            let splits = split(&positions, &fits, &today);
            groups(&splits, &fits, &today)
        };
        let moved = day_with(-3.0 * sigma, [-5.0, -4.0, -6.0]);
        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].fund, Some(("SMH", "Semiconductor")));
        assert_eq!(moved[0].members, ["C9", "C10", "C12"]);
        assert!(moved[0].change < 0.0 && moved[0].z <= -GROUP_Z);
        // An ordinary day for the fund is not a group move.
        assert!(day_with(-0.5 * sigma, [-1.0, -1.0, -1.0]).is_empty());
        // Nor is a fund move most members didn't follow.
        assert!(day_with(-3.0 * sigma, [2.0, 3.0, -6.0]).is_empty());
    }

    #[test]
    fn holdings_that_moved_far_together_without_a_fund_form_a_group() {
        let market = noise(1, N);
        let shared = noise(40, N);
        let fits: BTreeMap<String, Fit> = [41, 42, 43, 44]
            .iter()
            .map(|seed| {
                let own = noise(*seed, N);
                // The fourth stock's own moves are its own; the rest share theirs.
                let weight = if *seed == 44 { 0.0 } else { 1.0 };
                let stock = series(
                    &(0..N)
                        .map(|i| market[i] + weight * shared[i] + 0.4 * own[i])
                        .collect::<Vec<_>>(),
                );
                let ticker = format!("S{seed}");
                let fit = fit(
                    &ticker,
                    &stock,
                    &series(&market),
                    &BTreeMap::new(),
                    None,
                    day(N as i64),
                )
                .unwrap();
                (ticker, fit)
            })
            .collect();
        let today = BTreeMap::from([("SPY".to_string(), 0.0)]);
        let positions: Vec<(String, f64, f64)> = fits
            .iter()
            .map(|(ticker, fit)| (ticker.clone(), -4.0 * fit.sigma, -400.0 * fit.sigma))
            .collect();
        let splits = split(&positions, &fits, &today);
        let found = groups(&splits, &fits, &today);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].fund, None);
        assert_eq!(found[0].members, ["S41", "S42", "S43"]);
    }

    #[test]
    fn volatile_holdings_that_fell_together_on_an_ordinary_day_for_each_are_a_group() {
        // Three volatile stocks whose own swings partly go together each fell about one typical
        // day: unremarkable alone, but together the portfolio's loss.
        let market = noise(1, N);
        let shared = noise(50, N);
        let fits: BTreeMap<String, Fit> = [51, 52, 53]
            .iter()
            .map(|seed| {
                let own = noise(*seed, N);
                let stock = series(
                    &(0..N)
                        .map(|i| 2.0 * market[i] + 2.0 * shared[i] + 3.0 * own[i])
                        .collect::<Vec<_>>(),
                );
                let ticker = format!("V{seed}");
                let fit = fit(
                    &ticker,
                    &stock,
                    &series(&market),
                    &BTreeMap::new(),
                    None,
                    day(N as i64),
                )
                .unwrap();
                (ticker, fit)
            })
            .collect();
        let today = BTreeMap::from([("SPY".to_string(), -0.5)]);
        let positions: Vec<(String, f64, f64)> = fits
            .iter()
            .map(|(ticker, fit)| {
                let change = fit.market_beta * -0.5 - 1.2 * fit.sigma;
                (ticker.clone(), change, change * 50.0)
            })
            .collect();
        let splits = split(&positions, &fits, &today);
        assert!(splits.iter().all(|split| split.own_z.abs() < OWN_Z));
        let found = groups(&splits, &fits, &today);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].members, ["V51", "V52", "V53"]);
        assert!(found[0].change < 0.0 && found[0].z <= -GROUP_Z);
        // The same holdings moving opposite ways are not a group.
        let mut mixed = positions.clone();
        mixed[0].1 = -mixed[0].1;
        mixed[0].2 = -mixed[0].2;
        let splits = split(&mixed, &fits, &today);
        assert!(groups(&splits, &fits, &today)
            .iter()
            .all(|group| group.members.len() < 3));
    }
}

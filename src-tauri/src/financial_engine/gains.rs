//! Realized gains: trades restated for splits, then matched first-in-first-out, with each sale's
//! gain split into short- and long-term parts.
use super::*;

/// An estimated sale: the trade's index, its basis and gain, and the part of the gain from
/// shares held more than a year.
pub(super) struct FifoSale {
    index: usize,
    basis: Decimal,
    gain: Decimal,
    long_term_gain: Decimal,
}

#[derive(Default)]
pub(super) struct FifoState {
    /// Open lots, oldest first: units, unit cost, and acquisition date.
    pub(super) lots: VecDeque<(Decimal, Decimal, NaiveDate)>,
    pub(super) sales: Vec<FifoSale>,
    pub(super) invalid: bool,
}

impl FifoState {
    /// Sells `units` for `amount` from the oldest lots. Selling more than the lots hold leaves
    /// the history unexplained, so no sale in it is estimated.
    fn sell(&mut self, index: usize, units: Decimal, amount: Decimal, date: NaiveDate) {
        let mut remaining = units;
        let mut basis = Decimal::ZERO;
        let mut long_term_gain = Decimal::ZERO;
        while remaining > Decimal::ZERO {
            let Some((available, unit_cost, acquired)) = self.lots.pop_front() else {
                self.invalid = true;
                self.sales.clear();
                return;
            };
            let used = available.min(remaining);
            basis += used * unit_cost;
            if held_long_term(acquired, date) {
                long_term_gain += amount * used / units - used * unit_cost;
            }
            remaining -= used;
            if available > used {
                self.lots
                    .push_front((available - used, unit_cost, acquired));
            }
        }
        self.sales.push(FifoSale {
            index,
            basis,
            gain: amount - basis,
            long_term_gain,
        });
    }

    /// Whether the open lots match today's position in units and cost basis, within rounding.
    fn reconciles(&self, holdings: &[Holding], account_id: &str, ticker: &str) -> bool {
        let imported_units = self
            .lots
            .iter()
            .map(|(units, _, _)| *units)
            .sum::<Decimal>();
        let imported_basis = self
            .lots
            .iter()
            .map(|(units, unit_cost, _)| *units * *unit_cost)
            .sum::<Decimal>();
        let positions = positions(holdings, account_id, ticker).collect::<Vec<_>>();
        let current_units = positions
            .iter()
            .filter_map(|holding| holding.shares.and_then(decimal_f64))
            .sum::<Decimal>();
        let costs = positions
            .iter()
            .map(|holding| holding.cost_basis.and_then(decimal_f64))
            .collect::<Vec<_>>();
        let current_basis = costs.iter().flatten().sum::<Decimal>();
        let basis_complete = costs.iter().all(Option::is_some);
        let unit_difference = (imported_units - current_units).abs();
        let average_unit_cost = if imported_units.is_zero() {
            Decimal::ZERO
        } else {
            imported_basis.abs() / imported_units.abs()
        };
        // SnapTrade activity units can be rounded to 3 decimals while positions retain 4.
        let basis_tolerance = Decimal::new(11, 3) + unit_difference * average_unit_cost;
        !self.invalid
            && basis_complete
            && unit_difference <= Decimal::new(5, 4)
            && (imported_basis - current_basis).abs() <= basis_tolerance
    }
}

/// Shares sold on `sold` were held long-term when sold after the first anniversary of
/// `acquired`, matching the lots insight.
fn held_long_term(acquired: NaiveDate, sold: NaiveDate) -> bool {
    acquired
        .checked_add_months(chrono::Months::new(12))
        .and_then(|date| date.succ_opt())
        .is_some_and(|long_term_on| sold >= long_term_on)
}

/// A stock split: on `ex_date` each share became `ratio` shares.
pub(super) struct Split {
    pub(super) symbol: String,
    pub(super) ex_date: NaiveDate,
    pub(super) ratio: Decimal,
}

pub(super) fn splits(actions: &[CorporateAction]) -> Vec<Split> {
    actions
        .iter()
        .filter_map(|action| {
            let CorporateAction::Split {
                symbol,
                ex_date,
                old_rate,
                new_rate,
            } = action
            else {
                return None;
            };
            let positive = |rate: f64| decimal_f64(rate).filter(|rate| *rate > Decimal::ZERO);
            Some(Split {
                symbol: symbol.to_ascii_uppercase(),
                ex_date: *ex_date,
                ratio: positive(*new_rate)? / positive(*old_rate)?,
            })
        })
        .collect()
}

/// Restates trades recorded before a split in today's shares. Some brokers restate their
/// history and some don't, so a position is restated only when the restated trades explain
/// the shares held and the recorded ones don't; history a broker already adjusted is never
/// adjusted twice, and one neither version explains is left as recorded.
pub(super) fn adjust_for_splits(trades: &mut [Trade], holdings: &[Holding], splits: &[Split]) {
    let mut groups = BTreeMap::<(String, String), Vec<usize>>::new();
    for (index, trade) in trades.iter().enumerate() {
        let Some(ticker) = &trade.ticker else {
            continue;
        };
        groups
            .entry((trade.account_id.clone(), ticker.trim().to_ascii_uppercase()))
            .or_default()
            .push(index);
    }
    let tolerance = Decimal::new(5, 4);
    for ((account, ticker), indices) in groups {
        let factor = |trade: &Trade| {
            let date = NaiveDate::parse_from_str(&trade.date, "%Y-%m-%d").ok();
            splits
                .iter()
                .filter(|split| {
                    split.symbol == ticker && date.is_some_and(|date| date < split.ex_date)
                })
                .map(|split| split.ratio)
                .product::<Decimal>()
        };
        let signed = |trade: &Trade| {
            let kind = trade.r#type.to_ascii_uppercase();
            let units = trade.units.and_then(decimal_f64).unwrap_or_default().abs();
            if kind.contains("BUY") || kind.contains("REINVEST") {
                units
            } else if kind.contains("SELL") {
                -units
            } else {
                Decimal::ZERO
            }
        };
        if indices
            .iter()
            .all(|index| factor(&trades[*index]) == Decimal::ONE)
        {
            continue;
        }
        let recorded = indices
            .iter()
            .map(|index| signed(&trades[*index]))
            .sum::<Decimal>();
        let restated = indices
            .iter()
            .map(|index| signed(&trades[*index]) * factor(&trades[*index]))
            .sum::<Decimal>();
        let held = positions(holdings, &account, &ticker)
            .filter_map(|holding| holding.shares.and_then(decimal_f64))
            .sum::<Decimal>();
        if (restated - held).abs() > tolerance || (recorded - held).abs() <= tolerance {
            continue;
        }
        for index in indices {
            let ratio = factor(&trades[index]);
            if ratio == Decimal::ONE {
                continue;
            }
            let trade = &mut trades[index];
            trade.units = trade
                .units
                .and_then(decimal_f64)
                .map(|units| scaled(units * ratio, 8));
            trade.price = trade
                .price
                .and_then(decimal_f64)
                .map(|price| scaled(price / ratio, 8));
            trade.split_ratio = Some(scaled(ratio, 8));
        }
    }
}

/// An account's positions in one ticker.
fn positions<'a>(
    holdings: &'a [Holding],
    account_id: &'a str,
    ticker: &'a str,
) -> impl Iterator<Item = &'a Holding> {
    holdings.iter().filter(move |holding| {
        holding.account_id == account_id && holding.ticker.eq_ignore_ascii_case(ticker)
    })
}

/// `complete` says whether an account's saved trades are its whole history; gains are
/// estimated only there, and only for positions the trades reconcile with.
pub(super) fn add_estimated_realized_gains(
    trades: &mut [Trade],
    holdings: &[Holding],
    complete: &dyn Fn(&str) -> bool,
) {
    let mut states = BTreeMap::<(String, String), FifoState>::new();
    let mut order = (0..trades.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        (&trades[*left].date, &trades[*left].id).cmp(&(&trades[*right].date, &trades[*right].id))
    });

    for index in order {
        let trade = &trades[index];
        let Some(ticker) = trade
            .ticker
            .as_deref()
            .map(|ticker| ticker.trim().to_ascii_uppercase())
            .filter(|ticker| !ticker.is_empty())
        else {
            continue;
        };
        let state = states
            .entry((trade.account_id.clone(), ticker))
            .or_default();
        let kind = trade.r#type.to_ascii_uppercase();
        let Some(units) = trade
            .units
            .and_then(decimal_f64)
            .map(|units| units.abs())
            .filter(|units| !units.is_zero())
        else {
            state.invalid = true;
            continue;
        };
        let amount = decimal_f64(trade.amount)
            .map(|amount| amount.abs())
            .filter(|amount| !amount.is_zero())
            .or_else(|| {
                trade
                    .price
                    .and_then(decimal_f64)
                    .map(|price| price.abs() * units)
            });
        let Some(amount) = amount else {
            state.invalid = true;
            continue;
        };
        let Ok(date) = NaiveDate::parse_from_str(&trade.date, "%Y-%m-%d") else {
            state.invalid = true;
            continue;
        };

        if kind.contains("BUY") || kind.contains("REINVEST") {
            state.lots.push_back((units, amount / units, date));
            continue;
        }
        if !kind.contains("SELL") || state.invalid {
            state.invalid = true;
            continue;
        }

        state.sell(index, units, amount, date);
    }

    for ((account_id, ticker), state) in states {
        if !complete(&account_id) {
            continue;
        }
        if !state.reconciles(holdings, &account_id, &ticker) {
            continue;
        }
        for FifoSale {
            index,
            basis,
            gain,
            long_term_gain,
        } in state.sales
        {
            let trade = &mut trades[index];
            trade.realized_cost_basis = Some(money(basis));
            trade.estimated_realized_gain = Some(money(gain));
            // The short-term part is the remainder, so the two parts sum to the gain exactly.
            let long_term = rounded(long_term_gain);
            trade.estimated_long_term_gain = Some(money(long_term));
            trade.estimated_short_term_gain = Some(money(rounded(gain) - long_term));
            trade.estimated_realized_gain_pct =
                (!basis.is_zero()).then(|| scaled(gain / basis.abs() * Decimal::ONE_HUNDRED, 4));
            trade.realized_gain_method = Some("estimated-fifo".into());
        }
    }
}

pub(super) fn reconciled_acquisitions(
    holding: &PlaidHolding,
    ticker: &str,
    activities: &BTreeMap<String, Vec<SnapActivity>>,
) -> Vec<Acquisition> {
    let reconciles = |events: &[Acquisition]| {
        let total: Decimal = events.iter().map(|event| event.quantity).sum();
        let tolerance = (holding.quantity.abs() * Decimal::new(1, 4)).max(Decimal::new(1, 3));
        (total - holding.quantity).abs() <= tolerance
    };
    let lots = holding
        .tax_lots
        .iter()
        .filter_map(|lot| {
            let quantity = lot.quantity?;
            (quantity > Decimal::ZERO).then_some(Acquisition {
                date: iso_date(lot.original_purchase_datetime.as_deref())?,
                quantity,
            })
        })
        .collect::<Vec<_>>();
    if !lots.is_empty() && reconciles(&lots) {
        return lots;
    }
    let transfers = activities
        .values()
        .flatten()
        .filter_map(|activity| {
            let (allocated, quantity) = share_allocation(activity)?;
            allocated
                .eq_ignore_ascii_case(ticker)
                .then_some(Acquisition {
                    date: activity.date()?,
                    quantity,
                })
        })
        .collect::<Vec<_>>();
    if reconciles(&transfers) {
        transfers
    } else {
        Vec::new()
    }
}

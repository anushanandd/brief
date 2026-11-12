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
        // A buy or sell without its units can't explain the shares held either way.
        let signed = |trade: &Trade| {
            let kind = trade.r#type.to_ascii_uppercase();
            let units = || trade.units.and_then(decimal_f64).map(|units| units.abs());
            if kind.contains("BUY") || kind.contains("REINVEST") {
                units()
            } else if kind.contains("SELL") {
                units().map(|units| -units)
            } else {
                Some(Decimal::ZERO)
            }
        };
        if indices
            .iter()
            .all(|index| factor(&trades[*index]) == Decimal::ONE)
        {
            continue;
        }
        let Some(recorded) = indices
            .iter()
            .map(|index| signed(&trades[*index]))
            .sum::<Option<Decimal>>()
        else {
            continue;
        };
        let Some(restated) = indices
            .iter()
            .map(|index| Some(signed(&trades[*index])? * factor(&trades[*index])))
            .sum::<Option<Decimal>>()
        else {
            continue;
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn trade(id: usize, kind: &str, date: NaiveDate, units: f64, amount: f64) -> Trade {
        serde_json::from_value(serde_json::json!({
            "id": format!("t{id:03}"), "type": kind, "date": date.to_string(), "amount": amount,
            "account": "Brokerage", "accountId": "broker", "ticker": "EXMP",
            "description": null, "units": units, "price": amount / units,
        }))
        .unwrap()
    }

    fn holding(shares: f64, cost_basis: f64) -> Holding {
        serde_json::from_value(serde_json::json!({
            "ticker": "EXMP", "name": "Example Fund", "accountId": "broker", "shares": shares,
            "price": 10.0, "value": shares * 10.0, "costBasis": cost_basis,
            "dailyChangePct": null, "totalChangePct": null, "color": "#000000",
        }))
        .unwrap()
    }

    /// A generated history in today's shares: buys of whole multiples of six shares, and sales
    /// that never sell more than is held, two weeks apart from 2024-01-02. Returns the trades
    /// and what stays held, in shares and basis.
    fn history(seed: u64) -> (Vec<Trade>, f64, Decimal) {
        let mut state = seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1;
        let mut next = move |bound: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % bound
        };
        let mut lots: VecDeque<(Decimal, Decimal)> = VecDeque::new();
        let mut trades = Vec::new();
        for index in 0..24 {
            let date = day("2024-01-02") + chrono::Duration::days(14 * index as i64);
            let held: Decimal = lots.iter().map(|(units, _)| *units).sum();
            let units = Decimal::from(6 * (1 + next(5)));
            let amount = units * Decimal::from(5 + next(20));
            if held >= units && next(3) == 0 {
                trades.push(trade(
                    index,
                    "SELL",
                    date,
                    units.try_into().unwrap(),
                    amount.try_into().unwrap(),
                ));
                let mut remaining = units;
                while remaining > Decimal::ZERO {
                    let (available, cost) = lots.pop_front().unwrap();
                    let used = available.min(remaining);
                    if available > used {
                        lots.push_front((available - used, cost));
                    }
                    remaining -= used;
                }
            } else {
                trades.push(trade(
                    index,
                    "BUY",
                    date,
                    units.try_into().unwrap(),
                    amount.try_into().unwrap(),
                ));
                lots.push_back((units, amount / units));
            }
        }
        let shares = lots.iter().map(|(units, _)| *units).sum::<Decimal>();
        let basis = lots
            .iter()
            .map(|(units, cost)| *units * *cost)
            .sum::<Decimal>();
        (trades, shares.try_into().unwrap(), basis)
    }

    /// Each sale's estimated gain, by trade id.
    fn gains(trades: &[Trade]) -> Vec<(String, Option<f64>, Option<f64>)> {
        trades
            .iter()
            .filter(|trade| trade.r#type == "SELL")
            .map(|trade| {
                (
                    trade.id.clone(),
                    trade.estimated_realized_gain,
                    trade.estimated_long_term_gain,
                )
            })
            .collect()
    }

    #[test]
    fn gains_are_the_same_wherever_a_split_falls_and_however_the_broker_records_it() {
        for seed in 1..=10 {
            let (today, shares, basis) = history(seed);
            let holdings = [holding(shares, basis.try_into().unwrap())];
            let mut expected = today.clone();
            add_estimated_realized_gains(&mut expected, &holdings, &|_| true);
            let expected = gains(&expected);
            assert!(
                expected.iter().all(|(_, gain, _)| gain.is_some()),
                "seed {seed}"
            );
            // Realized gains plus what stays held account for every dollar in and out.
            let realized: Decimal = expected
                .iter()
                .map(|(_, gain, _)| decimal_f64(gain.unwrap()).unwrap())
                .sum();
            let paid: Decimal = today
                .iter()
                .map(|trade| {
                    let amount = decimal_f64(trade.amount).unwrap();
                    if trade.r#type == "SELL" {
                        amount
                    } else {
                        -amount
                    }
                })
                .sum();
            assert_eq!(
                rounded(realized - paid - basis),
                Decimal::ZERO,
                "seed {seed}"
            );

            for ratio in [2, 3] {
                for position in 1..today.len() {
                    let ex_date =
                        NaiveDate::parse_from_str(&today[position].date, "%Y-%m-%d").unwrap();
                    let split = Split {
                        symbol: "EXMP".into(),
                        ex_date,
                        ratio: Decimal::from(ratio),
                    };
                    // The broker reports trades from before the split unadjusted.
                    let mut recorded = today
                        .iter()
                        .enumerate()
                        .map(|(index, trade)| {
                            let mut trade = trade.clone();
                            if index < position {
                                trade.units = trade.units.map(|units| units / ratio as f64);
                                trade.price = trade.price.map(|price| price * ratio as f64);
                            }
                            trade
                        })
                        .collect::<Vec<_>>();
                    adjust_for_splits(&mut recorded, &holdings, std::slice::from_ref(&split));
                    add_estimated_realized_gains(&mut recorded, &holdings, &|_| true);
                    assert_eq!(
                        gains(&recorded),
                        expected,
                        "seed {seed}, 1:{ratio} at {position}"
                    );
                    // History the broker already adjusted for the split isn't adjusted again.
                    let mut restated = today.clone();
                    adjust_for_splits(&mut restated, &holdings, std::slice::from_ref(&split));
                    assert!(restated.iter().all(|trade| trade.split_ratio.is_none()));
                }
            }
        }
    }

    #[test]
    fn a_sale_of_more_than_is_held_leaves_every_sale_unestimated() {
        for seed in 1..=10 {
            let (mut trades, shares, basis) = history(seed);
            let Some(sale) = trades.iter().position(|trade| trade.r#type == "SELL") else {
                continue;
            };
            let held_then: f64 = trades[..sale]
                .iter()
                .map(|trade| trade.units.unwrap() * if trade.r#type == "SELL" { -1.0 } else { 1.0 })
                .sum();
            trades[sale].units = Some(held_then + 6.0);
            add_estimated_realized_gains(
                &mut trades,
                &[holding(shares, basis.try_into().unwrap())],
                &|_| true,
            );
            assert!(
                gains(&trades).iter().all(|(_, gain, _)| gain.is_none()),
                "seed {seed}: a sale beyond what was held was estimated"
            );
        }

        // Selling what's held empties the lots exactly; one more share is beyond them.
        let mut state = FifoState::default();
        state
            .lots
            .push_back((Decimal::from(10), Decimal::from(5), day("2025-01-02")));
        state.sell(0, Decimal::from(4), Decimal::from(40), day("2025-02-03"));
        state.sell(1, Decimal::from(6), Decimal::from(60), day("2025-02-04"));
        assert!(state.lots.is_empty() && !state.invalid && state.sales.len() == 2);
        state.sell(2, Decimal::ONE, Decimal::from(10), day("2025-02-05"));
        assert!(state.invalid && state.sales.is_empty());
    }

    #[test]
    fn a_lot_turns_long_term_the_day_after_its_first_anniversary() {
        assert!(!held_long_term(day("2025-03-15"), day("2026-03-15")));
        assert!(held_long_term(day("2025-03-15"), day("2026-03-16")));
        // A leap day's anniversary falls on the last day of February.
        assert!(!held_long_term(day("2024-02-29"), day("2025-02-28")));
        assert!(held_long_term(day("2024-02-29"), day("2025-03-01")));

        // One sale across a lot a year and a day old and one bought later: only the first
        // lot's share of the gain is long-term.
        let mut state = FifoState::default();
        state
            .lots
            .push_back((Decimal::from(10), Decimal::from(10), day("2025-01-10")));
        state
            .lots
            .push_back((Decimal::from(10), Decimal::from(20), day("2025-06-10")));
        state.sell(0, Decimal::from(20), Decimal::from(600), day("2026-01-11"));
        let sale = &state.sales[0];
        assert_eq!(sale.basis, Decimal::from(300));
        assert_eq!(sale.gain, Decimal::from(300));
        assert_eq!(sale.long_term_gain, Decimal::from(200));
        // A day earlier, none of it is.
        let mut state = FifoState::default();
        state
            .lots
            .push_back((Decimal::from(10), Decimal::from(10), day("2025-01-10")));
        state.sell(0, Decimal::from(10), Decimal::from(300), day("2026-01-10"));
        assert_eq!(state.sales[0].long_term_gain, Decimal::ZERO);
    }
}

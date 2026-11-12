//! Account totals as the pages read them: each type's total, the spending cards apart, what
//! positive balances hold by type, what the counted accounts own and owe, investment gains across
//! accounts, and the totals without the cards. Computed from saved values for the snapshot and
//! from live values for each market projection, so the renderer adds nothing up.
use serde::Serialize;
use ts_rs::TS;

use crate::finance_contract::{Account, Holding, SPENDING_ONLY_PREFIX};
use crate::numeric::{from_minor, minor_units, round2};
use crate::settings::FinanceSettings;

/// The account that stands for every account.
const COMBINED: &str = "all";

#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Rollup {
    /// Every account but the combined one, by type, largest first.
    pub groups: Vec<GroupValue>,
    /// The accounts the totals count, leaving out the spending cards, by type.
    pub listed: Vec<GroupValue>,
    /// The spending cards' total, when there are any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cards: Option<GroupValue>,
    /// What accounts with a positive balance hold, by type, with each type's share.
    pub shares: Vec<Share>,
    /// What the counted accounts own and owe.
    pub sheet: Sheet,
    /// Gains across brokerage and retirement accounts: the sum of those known, or none.
    pub unrealized: Option<f64>,
    pub realized_ytd: Option<f64>,
    /// Every retirement account's value, or none while one is unknown.
    pub retirement: Option<f64>,
    /// Net worth and the combined account's value without the spending cards.
    pub net_worth: f64,
    pub combined: Option<f64>,
    /// Each security across the accounts holding it.
    pub holdings: Vec<HoldingTotal>,
}

/// One security's position across accounts: the user's whole position in it, which every page
/// about the security reads. A sum is none while any account's part is unknown, never the
/// known parts' sum. Positions in different currencies are not added: the money sums are none
/// and `currency` is none, while shares still sum.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct HoldingTotal {
    pub ticker: String,
    /// Name, kind, color, quote eligibility, and valuation note, from the first held position.
    pub name: String,
    pub instrument_kind: Option<String>,
    pub color: String,
    pub quote_eligible: Option<bool>,
    pub valuation_note: Option<String>,
    /// The currency every position is valued in (USD when a provider names none); none when
    /// they differ.
    pub currency: Option<String>,
    pub shares: Option<f64>,
    /// Vested shares, counting a position without a vesting schedule as vested; none when no
    /// position has one.
    pub vested_shares: Option<f64>,
    pub value: Option<f64>,
    pub cost_basis: Option<f64>,
    /// Value less cost basis, from the two sums.
    pub unrealized_gain: Option<f64>,
    /// The gain on cost, in percent.
    pub total_change_pct: Option<f64>,
    pub average_basis: Option<f64>,
    /// Its share of every known position's value, in percent.
    pub weight: Option<f64>,
    /// The price and its day's and week's move, all from one quote: the position priced most
    /// recently (the first held on a tie), so the three never mix two quotes.
    pub price: Option<f64>,
    pub daily_change_pct: Option<f64>,
    pub weekly_change_pct: Option<f64>,
    pub market_as_of: Option<String>,
    /// Each account's part, in the order first held.
    pub accounts: Vec<HoldingPart>,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct HoldingPart {
    pub account_id: String,
    /// The currency this account's part is valued in; none when its own positions differ.
    pub currency: Option<String>,
    pub shares: Option<f64>,
    pub value: Option<f64>,
    pub cost_basis: Option<f64>,
    pub unrealized_gain: Option<f64>,
}

/// The currency the positions share (USD when the provider names none); none when they differ.
fn shared_currency<'a>(positions: impl IntoIterator<Item = &'a &'a Holding>) -> Option<String> {
    let mut codes = positions
        .into_iter()
        .map(|position| position.currency.as_deref().unwrap_or("USD"));
    let first = codes.next()?;
    codes.all(|code| code == first).then(|| first.to_owned())
}

/// Value less cost, in cents; none while either is unknown.
fn gain(value: Option<f64>, basis: Option<f64>) -> Option<f64> {
    value
        .zip(basis)
        .map(|(value, basis)| from_minor(minor_units(value) - minor_units(basis)))
}

/// Share counts summed to the hundred-millionth, so float error never reaches a total.
fn sum_shares(values: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    values
        .into_iter()
        .map(|value| value.map(|shares| (shares * 1e8).round() as i64))
        .sum::<Option<i64>>()
        .map(|units| units as f64 / 1e8)
}

/// Each security across the accounts holding it, in the order first held.
pub fn holding_totals(holdings: &[Holding]) -> Vec<HoldingTotal> {
    let known = holdings
        .iter()
        .filter_map(|holding| holding.value)
        .filter(|value| *value > 0.0)
        .map(minor_units)
        .sum::<i64>();
    let mut tickers: Vec<&str> = Vec::new();
    for holding in holdings {
        if !tickers.contains(&holding.ticker.as_str()) {
            tickers.push(&holding.ticker);
        }
    }
    tickers
        .into_iter()
        .map(|ticker| {
            let positions = holdings
                .iter()
                .filter(|holding| holding.ticker == ticker)
                .collect::<Vec<_>>();
            let currency = shared_currency(&positions);
            // Money in different currencies is never added.
            let money = |pick: fn(&Holding) -> Option<f64>, parts: &[&Holding]| {
                shared_currency(parts)
                    .and_then(|_| sum(parts.iter().map(|position| pick(position))))
            };
            let shares = sum_shares(positions.iter().map(|position| position.shares));
            let value = money(|holding| holding.value, &positions);
            let basis = money(|holding| holding.cost_basis, &positions);
            let unrealized_gain = gain(value, basis);
            let first_some = |pick: fn(&Holding) -> Option<String>| {
                positions.iter().find_map(|position| pick(position))
            };
            // One quote for every per-share figure: the newest, the first held on a tie.
            let quoted = positions
                .iter()
                .enumerate()
                .max_by(|(left_index, left), (right_index, right)| {
                    let at = |holding: &Holding| {
                        holding
                            .market_as_of
                            .as_deref()
                            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                    };
                    at(left).cmp(&at(right)).then(right_index.cmp(left_index))
                })
                .map(|(_, position)| *position)
                .unwrap_or(positions[0]);
            let notes = positions
                .iter()
                .map(|position| position.valuation_note.as_deref())
                .collect::<Vec<_>>();
            let mut accounts: Vec<&str> = Vec::new();
            for position in &positions {
                if !accounts.contains(&position.account_id.as_str()) {
                    accounts.push(&position.account_id);
                }
            }
            HoldingTotal {
                ticker: ticker.to_owned(),
                name: positions[0].name.clone(),
                instrument_kind: first_some(|holding| holding.instrument_kind.clone()),
                color: positions[0].color.clone(),
                quote_eligible: positions
                    .iter()
                    .find_map(|position| position.quote_eligible),
                valuation_note: notes
                    .iter()
                    .all(|note| *note == notes[0])
                    .then(|| notes[0].map(str::to_owned))
                    .flatten(),
                currency,
                shares,
                vested_shares: positions
                    .iter()
                    .any(|position| position.vested_shares.is_some())
                    .then(|| {
                        // A position whose shares are unknown leaves the total unknown.
                        sum_shares(
                            positions
                                .iter()
                                .map(|position| position.vested_shares.or(position.shares)),
                        )
                    })
                    .flatten(),
                total_change_pct: unrealized_gain
                    .zip(basis)
                    .filter(|(_, basis)| *basis > 0.0)
                    .map(|(gain, basis)| {
                        round2(minor_units(gain) as f64 / minor_units(basis) as f64 * 100.0)
                    }),
                average_basis: basis
                    .zip(shares)
                    .filter(|(_, shares)| *shares > 0.0)
                    .map(|(basis, shares)| basis / shares),
                weight: value
                    .filter(|value| *value >= 0.0 && known > 0)
                    .map(|value| round2(minor_units(value) as f64 / known as f64 * 100.0)),
                price: quoted.price,
                daily_change_pct: quoted.daily_change_pct,
                weekly_change_pct: quoted.weekly_change_pct,
                market_as_of: quoted.market_as_of.clone(),
                accounts: accounts
                    .into_iter()
                    .map(|account_id| {
                        let parts = positions
                            .iter()
                            .filter(|position| position.account_id == account_id)
                            .copied()
                            .collect::<Vec<_>>();
                        let value = money(|holding| holding.value, &parts);
                        let basis = money(|holding| holding.cost_basis, &parts);
                        HoldingPart {
                            account_id: account_id.to_owned(),
                            currency: shared_currency(&parts),
                            shares: sum_shares(parts.iter().map(|part| part.shares)),
                            unrealized_gain: gain(value, basis),
                            value,
                            cost_basis: basis,
                        }
                    })
                    .collect(),
                value,
                cost_basis: basis,
                unrealized_gain,
            }
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct GroupValue {
    #[serde(rename = "type")]
    pub account_type: String,
    /// None while an account in it has no value; a manual account awaiting its first balance
    /// has nothing to add yet.
    pub value: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    #[serde(rename = "type")]
    pub account_type: String,
    pub value: f64,
    pub percent: f64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Sheet {
    pub assets: f64,
    pub liabilities: f64,
    /// Every counted account has a value.
    pub complete: bool,
}

/// The spending cards: the credit accounts the user spends from, left out of the totals. A
/// spending-only account is always one, since spending is all it tracks.
pub fn spending_cards<'a>(accounts: &'a [Account], settings: &FinanceSettings) -> Vec<&'a Account> {
    accounts
        .iter()
        .filter(|account| {
            account.r#type == "credit"
                && (account.id.starts_with(SPENDING_ONLY_PREFIX)
                    || settings.spending_account_ids().any(|id| id == account.id))
        })
        .collect()
}

/// What the cards owe, in cents' precision. A spending-only card owes nothing of the user's;
/// any other card without a value leaves the total unknown.
fn cards_balance(cards: &[&Account]) -> Option<f64> {
    sum(cards
        .iter()
        .filter(|card| card.counted())
        .map(|card| card.value))
}

/// Net worth without the spending cards' balances, which are not market-driven; as it is when a
/// card's balance is unknown.
pub fn without_cards(accounts: &[Account], net_worth: f64, settings: &FinanceSettings) -> f64 {
    cards_balance(&spending_cards(accounts, settings)).map_or(net_worth, |cards| {
        from_minor(minor_units(net_worth) - minor_units(cards))
    })
}

/// Values summed in cents; none when any is missing.
fn sum(values: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    values
        .into_iter()
        .map(|value| value.map(minor_units))
        .sum::<Option<i64>>()
        .map(from_minor)
}

/// The known values summed in cents; none when none is known.
fn sum_known(values: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    let known = values.into_iter().flatten().collect::<Vec<_>>();
    (!known.is_empty()).then(|| from_minor(known.into_iter().map(minor_units).sum()))
}

/// Accounts by type, largest total first.
fn groups<'a>(accounts: impl IntoIterator<Item = &'a Account>) -> Vec<GroupValue> {
    let mut by_type: Vec<(String, Vec<&Account>)> = Vec::new();
    for account in accounts {
        match by_type.iter_mut().find(|(kind, _)| *kind == account.r#type) {
            Some((_, members)) => members.push(account),
            None => by_type.push((account.r#type.clone(), vec![account])),
        }
    }
    let mut groups = by_type
        .into_iter()
        .map(|(account_type, members)| GroupValue {
            value: sum(members
                .iter()
                .filter(|account| account.counted())
                .map(|account| account.value)),
            account_type,
        })
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        right
            .value
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&left.value.unwrap_or(f64::NEG_INFINITY))
            .then_with(|| left.account_type.cmp(&right.account_type))
    });
    groups
}

/// The counted accounts' totals by type, leaving out the combined account and the spending
/// cards: `Rollup::listed` alone, which each market chart point needs without the rest.
pub fn listed(accounts: &[Account], settings: &FinanceSettings) -> Vec<GroupValue> {
    let card_ids = spending_cards(accounts, settings)
        .into_iter()
        .map(|card| card.id.as_str())
        .collect::<Vec<_>>();
    groups(
        accounts
            .iter()
            .filter(|account| account.id != COMBINED && !card_ids.contains(&account.id.as_str())),
    )
}

/// The totals over `accounts` and `holdings`, valued at `net_worth`, leaving out the spending
/// cards.
pub fn rollup(
    accounts: &[Account],
    holdings: &[Holding],
    net_worth: f64,
    settings: &FinanceSettings,
) -> Rollup {
    let card_ids = spending_cards(accounts, settings)
        .into_iter()
        .map(|card| card.id.as_str())
        .collect::<Vec<_>>();
    let is_card = |account: &&Account| card_ids.contains(&account.id.as_str());
    let every = accounts.iter().filter(|account| account.id != COMBINED);
    let listed = every
        .clone()
        .filter(|account| !is_card(account))
        .collect::<Vec<_>>();
    let cards = every.clone().filter(is_card).collect::<Vec<_>>();
    let positive = groups(
        every
            .clone()
            .filter(|account| account.value.is_some_and(|value| value > 0.0)),
    );
    let positive_total = positive
        .iter()
        .filter_map(|group| group.value)
        .map(minor_units)
        .sum::<i64>();
    let values = listed
        .iter()
        .filter_map(|account| account.value)
        .collect::<Vec<_>>();
    let investments = accounts
        .iter()
        .filter(|account| matches!(account.r#type.as_str(), "brokerage" | "retirement"))
        .collect::<Vec<_>>();
    let card_total = cards_balance(&cards);
    let without_cards = |value: f64| {
        card_total.map_or(value, |cards| {
            from_minor(minor_units(value) - minor_units(cards))
        })
    };
    Rollup {
        groups: groups(every.clone()),
        listed: self::listed(accounts, settings),
        cards: (!cards.is_empty()).then(|| GroupValue {
            account_type: "spending".into(),
            value: card_total,
        }),
        shares: positive
            .into_iter()
            .filter_map(|group| {
                let value = group.value?;
                Some(Share {
                    account_type: group.account_type,
                    value,
                    percent: if positive_total == 0 {
                        0.0
                    } else {
                        round2(minor_units(value) as f64 / positive_total as f64 * 100.0)
                    },
                })
            })
            .collect(),
        sheet: Sheet {
            assets: from_minor(
                values
                    .iter()
                    .filter(|value| **value > 0.0)
                    .map(|value| minor_units(*value))
                    .sum(),
            ),
            liabilities: from_minor(
                values
                    .iter()
                    .filter(|value| **value < 0.0)
                    .map(|value| minor_units(*value))
                    .sum(),
            ),
            complete: values.len() == listed.len(),
        },
        unrealized: sum_known(
            investments
                .iter()
                .map(|account| account.known_unrealized_gain),
        ),
        realized_ytd: sum_known(
            investments
                .iter()
                .map(|account| account.estimated_realized_gain_ytd),
        ),
        retirement: sum(investments
            .iter()
            .filter(|account| account.r#type == "retirement")
            .map(|account| account.value)),
        net_worth: without_cards(net_worth),
        combined: accounts
            .iter()
            .find(|account| account.id == COMBINED)
            .and_then(|account| account.value)
            .map(without_cards),
        holdings: holding_totals(holdings),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn account(id: &str, kind: &str, value: Option<f64>) -> Account {
        serde_json::from_value(json!({
            "id": id, "name": id, "institution": "Bank", "type": kind, "value": value,
        }))
        .unwrap()
    }

    #[test]
    fn totals_each_type_sets_the_cards_apart_and_shares_what_balances_hold() {
        let mut brokerage = account("brokerage", "brokerage", Some(600.0));
        brokerage.known_unrealized_gain = Some(50.25);
        let mut retirement = account("ira", "retirement", Some(300.0));
        retirement.known_unrealized_gain = None;
        retirement.estimated_realized_gain_ytd = Some(-10.0);
        let accounts = [
            account("all", "combined", Some(1_000.0)),
            account("checking", "cash", Some(100.1)),
            account("savings", "cash", Some(0.2)),
            brokerage,
            retirement,
            account("card", "credit", Some(-40.0)),
            account("other-card", "credit", Some(-5.0)),
            // A manual account awaiting its first balance adds nothing yet.
            account("manual:house", "cash", None),
        ];
        let settings = FinanceSettings {
            spending_account_id: Some("card".into()),
            ..Default::default()
        };
        let totals = rollup(&accounts, &[], 1_000.0, &settings);
        fn groups(groups: &[GroupValue]) -> Vec<(&str, Option<f64>)> {
            groups
                .iter()
                .map(|group| (group.account_type.as_str(), group.value))
                .collect()
        }
        assert_eq!(
            groups(&totals.groups),
            [
                ("brokerage", Some(600.0)),
                ("retirement", Some(300.0)),
                ("cash", Some(100.3)),
                ("credit", Some(-45.0)),
            ]
        );
        // The spending card is left out of what Accounts counts, and listed apart.
        assert_eq!(groups(&totals.listed)[3], ("credit", Some(-5.0)));
        assert_eq!(
            totals.cards.as_ref().and_then(|cards| cards.value),
            Some(-40.0)
        );
        assert_eq!(
            totals.sheet,
            Sheet {
                assets: 1_000.3,
                liabilities: -5.0,
                complete: false,
            }
        );
        // Positive balances by type, each type's share of them.
        assert_eq!(
            totals
                .shares
                .iter()
                .map(|share| (share.account_type.as_str(), share.percent))
                .collect::<Vec<_>>(),
            [("brokerage", 59.98), ("retirement", 29.99), ("cash", 10.03)]
        );
        assert_eq!(
            (totals.unrealized, totals.realized_ytd),
            (Some(50.25), Some(-10.0))
        );
        assert_eq!(totals.retirement, Some(300.0));
        // Without the card's balance, which the totals leave out.
        assert_eq!(
            (totals.net_worth, totals.combined),
            (1_040.0, Some(1_040.0))
        );
    }

    #[test]
    fn a_spending_only_card_is_always_a_spending_card_and_leaves_the_others_total_alone() {
        let accounts = vec![
            account("all", "combined", Some(900.0)),
            account("checking", "cash", Some(1_000.0)),
            account("card", "credit", Some(-100.0)),
            account("wallet:apple-card", "credit", None),
        ];
        let settings = FinanceSettings {
            spending_account_id: Some("card".into()),
            ..Default::default()
        };
        let cards = spending_cards(&accounts, &settings);
        assert_eq!(
            cards
                .iter()
                .map(|card| card.id.as_str())
                .collect::<Vec<_>>(),
            ["card", "wallet:apple-card"]
        );
        // Its missing balance must not make the card it sits beside unknown.
        assert_eq!(without_cards(&accounts, 900.0, &settings), 1_000.0);
        let totals = rollup(&accounts, &[], 900.0, &settings);
        assert_eq!(totals.cards.unwrap().value, Some(-100.0));
        // A card that should have a balance and doesn't still leaves the total unknown.
        let mut unknown = accounts.clone();
        unknown[2].value = None;
        assert_eq!(
            rollup(&unknown, &[], 1_000.0, &settings)
                .cards
                .unwrap()
                .value,
            None
        );
    }

    #[test]
    fn an_unknown_value_leaves_its_group_and_retirement_unknown() {
        let accounts = [
            account("checking", "cash", None),
            account("ira", "retirement", None),
        ];
        let totals = rollup(&accounts, &[], 0.0, &FinanceSettings::default());
        assert!(totals.groups.iter().all(|group| group.value.is_none()));
        assert_eq!(totals.retirement, None);
        assert_eq!((totals.cards, totals.unrealized), (None, None));
        assert!(totals.shares.is_empty());
    }

    #[test]
    fn each_security_totals_its_positions_across_accounts() {
        let holding = |account: &str, shares: f64, value: Option<f64>, basis: f64| -> Holding {
            serde_json::from_value(json!({
                "ticker": "VTI", "name": "VTI", "accountId": account, "shares": shares,
                "price": 100.0, "value": value, "costBasis": basis, "dailyChangePct": 1.5,
                "totalChangePct": null, "color": "", "currency": "USD",
            }))
            .unwrap()
        };
        let mut unvested = holding("b", 0.3, Some(30.0), 20.0);
        unvested.vested_shares = Some(0.1);
        let holdings = [
            holding("a", 1.1, Some(110.0), 100.0),
            unvested,
            holding("a", 0.2, Some(20.0), 20.0),
        ];
        let other: Holding = serde_json::from_value(json!({
            "ticker": "CASH", "name": "Cash fund", "accountId": "a", "shares": 40.0, "price": 1.0,
            "value": 40.0, "costBasis": null, "dailyChangePct": 0.0, "totalChangePct": null,
            "color": "", "currency": "USD",
        }))
        .unwrap();
        let all = [&holdings[..], &[other]].concat();
        let totals = rollup(&[], &all, 0.0, &FinanceSettings::default());
        let vti = &totals.holdings[0];
        assert_eq!(
            (vti.shares, vti.value, vti.cost_basis, vti.vested_shares),
            (Some(1.6), Some(160.0), Some(140.0), Some(1.4))
        );
        assert_eq!(vti.unrealized_gain, Some(20.0));
        assert_eq!(
            (vti.name.as_str(), vti.currency.as_deref(), vti.price),
            ("VTI", Some("USD"), Some(100.0))
        );
        assert_eq!(
            (vti.total_change_pct, vti.weight, vti.daily_change_pct),
            (Some(14.29), Some(80.0), Some(1.5))
        );
        assert_eq!(vti.average_basis, Some(87.5));
        // A position whose shares are unknown leaves vested shares unknown, not short.
        let mut unknown = holding("c", 0.0, None, 0.0);
        unknown.shares = None;
        let partial = rollup(
            &[],
            &[&holdings[..], &[unknown]].concat(),
            0.0,
            &FinanceSettings::default(),
        );
        assert_eq!(partial.holdings[0].vested_shares, None);
        // Each account's part, in the order first held.
        assert_eq!(
            vti.accounts,
            [
                HoldingPart {
                    account_id: "a".into(),
                    currency: Some("USD".into()),
                    shares: Some(1.3),
                    value: Some(130.0),
                    cost_basis: Some(120.0),
                    unrealized_gain: Some(10.0),
                },
                HoldingPart {
                    account_id: "b".into(),
                    currency: Some("USD".into()),
                    shares: Some(0.3),
                    value: Some(30.0),
                    cost_basis: Some(20.0),
                    unrealized_gain: Some(10.0),
                },
            ]
        );
        // A position without a known value leaves the total unknown, not partial.
        let unknown = rollup(
            &[],
            &[
                holding("a", 1.0, None, 10.0),
                holding("b", 1.0, Some(5.0), 4.0),
            ],
            0.0,
            &FinanceSettings::default(),
        );
        assert_eq!(
            (unknown.holdings[0].value, unknown.holdings[0].weight),
            (None, None)
        );
    }

    /// A position with a synthetic quote, in `currency`.
    fn position(account: &str, value: f64, basis: Option<f64>, currency: &str) -> Holding {
        serde_json::from_value(json!({
            "ticker": "SYN", "name": "Synthetic Co", "accountId": account, "shares": 2.0,
            "price": value / 2.0, "value": value, "costBasis": basis, "dailyChangePct": 1.0,
            "totalChangePct": null, "color": "", "currency": currency,
            "marketAsOf": "2026-10-16T20:00:00Z",
        }))
        .unwrap()
    }

    #[test]
    fn one_unknown_cost_basis_leaves_the_whole_positions_basis_and_gain_unknown() {
        let totals = rollup(
            &[],
            &[
                position("a", 300.0, Some(200.0), "USD"),
                position("b", 100.0, None, "USD"),
            ],
            0.0,
            &FinanceSettings::default(),
        );
        assert_eq!(totals.holdings.len(), 1);
        let syn = &totals.holdings[0];
        // The value is known in both accounts; the basis only in one, so it is unknown, not 200.
        assert_eq!((syn.shares, syn.value), (Some(4.0), Some(400.0)));
        assert_eq!(
            (syn.cost_basis, syn.unrealized_gain, syn.total_change_pct),
            (None, None, None)
        );
        assert_eq!(syn.average_basis, None);
        // Each account keeps what it knows.
        assert_eq!(
            (syn.accounts[0].cost_basis, syn.accounts[0].unrealized_gain),
            (Some(200.0), Some(100.0))
        );
        assert_eq!(
            (syn.accounts[1].cost_basis, syn.accounts[1].unrealized_gain),
            (None, None)
        );
    }

    #[test]
    fn positions_in_different_currencies_are_never_added() {
        let totals = rollup(
            &[],
            &[
                position("a", 300.0, Some(200.0), "USD"),
                position("b", 100.0, Some(50.0), "CAD"),
            ],
            0.0,
            &FinanceSettings::default(),
        );
        let syn = &totals.holdings[0];
        // One record for the ticker; shares are shares in any currency, money is not summed.
        assert_eq!(totals.holdings.len(), 1);
        assert_eq!((syn.shares, syn.currency.as_deref()), (Some(4.0), None));
        assert_eq!(
            (syn.value, syn.cost_basis, syn.unrealized_gain, syn.weight),
            (None, None, None, None)
        );
        assert_eq!(
            syn.accounts
                .iter()
                .map(|part| (part.currency.as_deref(), part.value, part.unrealized_gain))
                .collect::<Vec<_>>(),
            [
                (Some("USD"), Some(300.0), Some(100.0)),
                (Some("CAD"), Some(100.0), Some(50.0)),
            ]
        );
    }

    #[test]
    fn per_share_figures_come_from_the_newest_quote_together() {
        let mut older = position("a", 300.0, Some(200.0), "USD");
        older.market_as_of = Some("2026-10-15T20:00:00Z".into());
        older.daily_change_pct = Some(-2.0);
        let mut newer = position("b", 110.0, Some(100.0), "USD");
        newer.daily_change_pct = Some(3.0);
        let totals = rollup(&[], &[older, newer], 0.0, &FinanceSettings::default());
        let syn = &totals.holdings[0];
        assert_eq!(
            (syn.price, syn.daily_change_pct, syn.market_as_of.as_deref()),
            (Some(55.0), Some(3.0), Some("2026-10-16T20:00:00Z"))
        );
        // The gain and its percent come from the sums: 410 on a 300 basis.
        assert_eq!(
            (syn.unrealized_gain, syn.total_change_pct),
            (Some(110.0), Some(36.67))
        );
    }
}

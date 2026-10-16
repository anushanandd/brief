//! Positions as the snapshot shows them: Plaid holdings with their securities, and SnapTrade
//! positions valued in USD when the provider priced them.
use super::*;

/// Every holding, sorted by account and ticker.
pub(super) fn project_holdings(
    parsed: &Parsed,
    included: &[&PlaidAccount],
    included_ids: &BTreeSet<&str>,
) -> Vec<Holding> {
    let plaid = parsed
        .plaid_holdings
        .iter()
        .filter(|holding| included_ids.contains(holding.account_id.as_str()))
        .map(|holding| plaid_holding(holding, parsed, included));
    let snaptrade = parsed.snap_accounts.iter().flat_map(|account| {
        parsed
            .snap_positions
            .get(&account.id)
            .into_iter()
            .flatten()
            .map(move |position| snaptrade_holding(&account.id, position))
    });
    let mut holdings: Vec<Holding> = plaid
        .chain(snaptrade)
        .enumerate()
        .map(|(index, holding)| Holding {
            color: COLORS[index % COLORS.len()].into(),
            ..holding
        })
        .collect();
    holdings.sort_by(|left, right| {
        (&left.account_id, &left.ticker).cmp(&(&right.account_id, &right.ticker))
    });
    holdings
}

/// A Plaid position, priced by the institution, the security's close, or its value per share.
fn plaid_holding(holding: &PlaidHolding, parsed: &Parsed, included: &[&PlaidAccount]) -> Holding {
    let security = parsed.plaid_securities.get(&holding.security_id);
    let price = holding
        .institution_price
        .or_else(|| security.and_then(|security| security.close_price))
        .or_else(|| {
            let value = holding.institution_value.or(holding.value)?;
            (!holding.quantity.is_zero()).then_some(value / holding.quantity)
        });
    let currency = holding
        .iso_currency_code
        .as_deref()
        .or_else(|| security.and_then(|security| security.iso_currency_code.as_deref()))
        .or_else(|| {
            included
                .iter()
                .find(|account| account.account_id == holding.account_id)
                .and_then(|account| account.balances.iso_currency_code.as_deref())
        });
    let raw_value = holding
        .institution_value
        .or(holding.value)
        .or_else(|| price.map(|price| holding.quantity * price));
    let value = usd_value(raw_value, currency);
    let cost_basis = usd_value(holding.cost_basis, currency);
    let kind = security
        .and_then(|security| security.kind.as_deref())
        .unwrap_or("unknown");
    let ticker = security.and_then(|security| security.ticker_symbol.as_deref());
    let name = security.and_then(|security| security.name.as_deref());
    let mut projected = Holding {
        ticker: text(ticker.or(name), "—"),
        name: text(name.or(ticker), "Investment position"),
        account_id: format!("plaid:{}", holding.account_id),
        shares: Some(scaled(holding.quantity, 8)),
        price: price.map(|price| scaled(price, 8)),
        value: value.map(money),
        cost_basis: cost_basis.map(money),
        total_change_pct: total_change_pct(value, cost_basis),
        instrument_kind: Some(kind.into()),
        currency: currency.map(str::to_owned),
        quote_eligible: Some(currency == Some("USD") && kind == "equity"),
        valuation_note: value.is_none().then(|| "USD valuation unavailable".into()),
        market_as_of: holding
            .institution_price_datetime
            .clone()
            .or(holding.institution_price_as_of.clone()),
        ..Holding::default()
    };
    if let Some(security) = security {
        describe_security(&mut projected, security);
    }
    // Vesting matters only when some shares are still unvested.
    if let Some(vested) = holding
        .vested_quantity
        .filter(|vested| *vested < holding.quantity)
    {
        projected.vested_shares = Some(scaled(vested, 8));
        projected.vested_value = usd_value(holding.vested_value, currency).map(money);
    }
    projected
}

/// A SnapTrade position, valued only for share-priced instruments in USD.
fn snaptrade_holding(account_id: &str, position: &SnapPosition) -> Holding {
    let underlying = position.instrument.underlying.as_ref();
    let ticker = position
        .instrument
        .label
        .raw_symbol
        .as_ref()
        .or(position.instrument.label.symbol.as_ref())
        .or_else(|| {
            underlying.and_then(|label| label.raw_symbol.as_ref().or(label.symbol.as_ref()))
        });
    let name = position
        .instrument
        .label
        .description
        .as_ref()
        .or_else(|| underlying.and_then(|label| label.description.as_ref()))
        .or(position.instrument.label.symbol.as_ref());
    let kind = position.instrument.kind.as_deref().unwrap_or("unknown");
    let share_valued = ["stock", "etf", "adr", "cef", "mutualfund"].contains(&kind);
    let value = if share_valued {
        usd_value(
            position
                .units
                .zip(position.price)
                .map(|(units, price)| units * price),
            position.currency.as_deref(),
        )
    } else {
        None
    };
    let cost_basis = value.and_then(|_| {
        position
            .units
            .zip(position.cost_basis)
            .map(|(units, basis)| rounded(units * basis))
    });
    let valuation_note = if position.currency.as_deref() != Some("USD") {
        "USD valuation unavailable"
    } else if !share_valued {
        "Instrument valuation unsupported"
    } else {
        "Position not priced by provider"
    };
    Holding {
        ticker: text(ticker.map(String::as_str), "—"),
        name: text(name.map(String::as_str), "Investment position"),
        account_id: format!("snaptrade:{account_id}"),
        shares: position.units.map(|units| scaled(units, 8)),
        price: position.price.map(|price| scaled(price, 8)),
        value: value.map(money),
        cost_basis: cost_basis.map(money),
        total_change_pct: total_change_pct(value, cost_basis),
        instrument_kind: Some(kind.into()),
        currency: position.currency.clone(),
        quote_eligible: Some(
            value.is_some()
                && ["stock", "etf", "adr", "cef"].contains(&kind)
                && ticker.is_some_and(|ticker| ticker != "—"),
        ),
        valuation_note: value.is_none().then(|| valuation_note.into()),
        ..Holding::default()
    }
}

/// A position's change against its cost, in percent, when both are known.
fn total_change_pct(value: Option<Decimal>, cost_basis: Option<Decimal>) -> Option<f64> {
    let basis = cost_basis.filter(|basis| !basis.is_zero())?;
    Some(scaled(
        rounded((value? - basis) / basis.abs() * Decimal::ONE_HUNDRED),
        4,
    ))
}

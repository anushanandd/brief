use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    str::FromStr,
};

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
use rust_decimal::{prelude::ToPrimitive, Decimal, RoundingStrategy};
use serde::{de::Error as _, Deserialize, Deserializer};
use serde_json::{json, Map, Value};

use crate::{
    providers::{MarketSnapshot, ProviderSync},
    storage::Annotation,
};

const COLORS: [&str; 6] = [
    "#232424", "#71877c", "#b87543", "#9d9588", "#6f7680", "#a39b8e",
];
const SPENDING_COLORS: [&str; 6] = [
    "#477d75", "#4f78a8", "#b58a3f", "#7d6da5", "#a45f79", "#79924b",
];
pub(crate) const CALCULATION_VERSION: u32 = 15;

#[derive(Debug, Deserialize)]
struct PlaidBalances {
    #[serde(default, deserialize_with = "optional_decimal")]
    current: Option<Decimal>,
    #[serde(default)]
    iso_currency_code: Option<String>,
    #[serde(default)]
    last_updated_datetime: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlaidAccount {
    account_id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    institution_name: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    mask: Option<String>,
    #[serde(default)]
    balance_fetched_at: Option<String>,
    balances: PlaidBalances,
}

#[derive(Debug, Deserialize)]
struct PlaidCategory {
    primary: String,
}

#[derive(Debug, Deserialize)]
struct PlaidCounterparty {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    logo_url: Option<String>,
    #[serde(default)]
    website: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlaidTransaction {
    transaction_id: String,
    account_id: String,
    #[serde(deserialize_with = "decimal")]
    amount: Decimal,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    merchant_name: Option<String>,
    #[serde(default)]
    logo_url: Option<String>,
    #[serde(default)]
    website: Option<String>,
    pending: bool,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    datetime: Option<String>,
    #[serde(default)]
    authorized_date: Option<String>,
    #[serde(default)]
    authorized_datetime: Option<String>,
    #[serde(default)]
    category: Option<Vec<String>>,
    #[serde(default)]
    personal_finance_category: Option<PlaidCategory>,
    #[serde(default)]
    counterparties: Option<Vec<PlaidCounterparty>>,
}

#[derive(Debug, Deserialize)]
struct TaxLot {
    #[serde(default)]
    original_purchase_datetime: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    quantity: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
struct PlaidHolding {
    account_id: String,
    security_id: String,
    #[serde(deserialize_with = "decimal")]
    quantity: Decimal,
    #[serde(default, deserialize_with = "optional_decimal")]
    institution_price: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    institution_value: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    value: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    cost_basis: Option<Decimal>,
    #[serde(default)]
    iso_currency_code: Option<String>,
    #[serde(default)]
    institution_price_as_of: Option<String>,
    #[serde(default)]
    institution_price_datetime: Option<String>,
    #[serde(default)]
    tax_lots: Vec<TaxLot>,
}

#[derive(Debug, Deserialize)]
struct PlaidSecurity {
    security_id: String,
    #[serde(default)]
    ticker_symbol: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    close_price: Option<Decimal>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    is_cash_equivalent: Option<bool>,
    #[serde(default)]
    iso_currency_code: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SnapAmount {
    #[serde(default, deserialize_with = "optional_decimal")]
    amount: Option<Decimal>,
    #[serde(default)]
    currency: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SnapBalance {
    #[serde(default)]
    total: Option<SnapAmount>,
}

#[derive(Debug, Default, Deserialize)]
struct SnapSyncPoint {
    #[serde(default)]
    last_successful_sync: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SnapSyncStatus {
    #[serde(default)]
    holdings: Option<SnapSyncPoint>,
    #[serde(default)]
    transactions: Option<SnapSyncPoint>,
}

#[derive(Debug, Deserialize)]
struct SnapAccount {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    raw_type: Option<String>,
    #[serde(default)]
    institution_name: Option<String>,
    #[serde(default)]
    number: Option<String>,
    #[serde(default)]
    account_number: Option<String>,
    #[serde(default)]
    balance: SnapBalance,
    #[serde(default)]
    sync_status: Option<SnapSyncStatus>,
    #[serde(default)]
    balance_fetched_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct InstrumentLabel {
    #[serde(default)]
    raw_symbol: Option<String>,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SnapInstrument {
    #[serde(default)]
    kind: Option<String>,
    #[serde(flatten)]
    label: InstrumentLabel,
    #[serde(default)]
    underlying: Option<InstrumentLabel>,
}

#[derive(Debug, Deserialize)]
struct SnapPosition {
    #[serde(default, deserialize_with = "optional_decimal")]
    units: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    price: Option<Decimal>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    cost_basis: Option<Decimal>,
    instrument: SnapInstrument,
}

#[derive(Debug, Default, Deserialize)]
struct ActivitySymbol {
    #[serde(default)]
    raw_symbol: Option<String>,
    #[serde(default)]
    symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SnapActivity {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    #[serde(deserialize_with = "decimal")]
    amount: Decimal,
    #[serde(default)]
    trade_date: Option<String>,
    #[serde(default)]
    settlement_date: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    units: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    price: Option<Decimal>,
    #[serde(default)]
    symbol: Option<ActivitySymbol>,
}

#[derive(Clone, Debug, Deserialize)]
struct HistoryPoint {
    date: String,
    #[serde(default, deserialize_with = "optional_decimal")]
    total_value: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    value: Option<Decimal>,
}

#[derive(Clone)]
struct Acquisition {
    date: String,
    quantity: Decimal,
}

fn decimal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Decimal, D::Error> {
    let value = Value::deserialize(deserializer)?;
    decimal_value(&value).ok_or_else(|| D::Error::custom("expected a finite decimal"))
}

fn optional_decimal<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Decimal>, D::Error> {
    let value = Option::<Value>::deserialize(deserializer)?;
    value
        .as_ref()
        .map(|value| {
            decimal_value(value).ok_or_else(|| D::Error::custom("expected a finite decimal"))
        })
        .transpose()
}

fn decimal_value(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(number) => Decimal::from_str(&number.to_string()).ok(),
        Value::String(number) => Decimal::from_str(number).ok(),
        _ => None,
    }
}

fn rounded(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn json_decimal(value: Decimal) -> Value {
    Value::from(
        rounded(value)
            .to_f64()
            .expect("a Decimal value always fits in f64"),
    )
}

fn optional_json_decimal(value: Option<Decimal>) -> Value {
    value.map(json_decimal).unwrap_or(Value::Null)
}

fn json_scaled(value: Decimal, decimal_places: u32) -> Value {
    Value::from(
        value
            .round_dp_with_strategy(decimal_places, RoundingStrategy::MidpointAwayFromZero)
            .to_f64()
            .expect("a Decimal value always fits in f64"),
    )
}

fn optional_json_scaled(value: Option<Decimal>, decimal_places: u32) -> Value {
    value
        .map(|value| json_scaled(value, decimal_places))
        .unwrap_or(Value::Null)
}

fn usd_value(value: Option<Decimal>, currency: Option<&str>) -> Option<Decimal> {
    (currency == Some("USD"))
        .then_some(value)
        .flatten()
        .map(rounded)
}

fn parse_records<T: for<'de> Deserialize<'de>>(
    values: &[Value],
    label: &str,
    warnings: &mut Vec<String>,
) -> Vec<T> {
    values
        .iter()
        .enumerate()
        .filter_map(
            |(index, value)| match serde_json::from_value(value.clone()) {
                Ok(record) => Some(record),
                Err(_) => {
                    warnings.push(format!(
                        "{label} record {} was malformed and was excluded",
                        index + 1
                    ));
                    None
                }
            },
        )
        .collect()
}

fn iso_date(value: Option<&str>) -> Option<String> {
    let date = value?.get(..10)?;
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .map(|_| date.to_string())
}

fn plaid_posted_date(transaction: &PlaidTransaction) -> Option<String> {
    iso_date(
        transaction
            .datetime
            .as_deref()
            .or(transaction.date.as_deref()),
    )
}

fn text(value: Option<&str>, fallback: &str) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn normalized(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn possible_duplicate(
    plaid: &PlaidAccount,
    snap: &SnapAccount,
    holdings: &[PlaidHolding],
    securities: &BTreeMap<String, PlaidSecurity>,
    positions: &[SnapPosition],
) -> bool {
    let plaid_institution = normalized(plaid.institution_name.as_deref());
    let snap_institution = normalized(snap.institution_name.as_deref());
    let last_four = |value: &str| value.chars().rev().take(4).collect::<String>();
    let plaid_mask = plaid.mask.as_deref().map(last_four);
    let snap_number = snap.number.as_deref().or(snap.account_number.as_deref());
    let snap_mask = snap_number.map(last_four);
    if !plaid_institution.is_empty()
        && plaid_institution == snap_institution
        && plaid_mask.is_some()
        && plaid_mask == snap_mask
    {
        return true;
    }

    let Some(plaid_balance) = usd_value(
        plaid.balances.current,
        plaid.balances.iso_currency_code.as_deref(),
    ) else {
        return false;
    };
    let snap_total = snap.balance.total.as_ref();
    let Some(snap_balance) = usd_value(
        snap_total.and_then(|total| total.amount),
        snap_total.and_then(|total| total.currency.as_deref()),
    ) else {
        return false;
    };
    let balance_scale = plaid_balance.abs().max(snap_balance.abs());
    let tolerance = Decimal::ONE.max(balance_scale * Decimal::new(3, 2));
    if (plaid_balance - snap_balance).abs() > tolerance {
        return false;
    }

    let account_holdings = holdings
        .iter()
        .filter(|holding| holding.account_id == plaid.account_id)
        .collect::<Vec<_>>();
    let plaid_symbols = account_holdings
        .iter()
        .filter_map(|holding| securities.get(&holding.security_id))
        .filter(|security| {
            security.is_cash_equivalent != Some(true)
                && !security
                    .kind
                    .as_deref()
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("cash"))
        })
        .filter_map(|security| security.ticker_symbol.as_deref())
        .map(|ticker| ticker.trim().to_ascii_uppercase())
        .filter(|ticker| !ticker.is_empty())
        .collect::<BTreeSet<_>>();
    let snap_symbols = positions
        .iter()
        .filter(|position| {
            position
                .instrument
                .kind
                .as_deref()
                .is_none_or(|kind| !kind.eq_ignore_ascii_case("cash"))
        })
        .filter_map(|position| {
            position
                .instrument
                .label
                .raw_symbol
                .as_deref()
                .or(position.instrument.label.symbol.as_deref())
                .or_else(|| {
                    let underlying = position.instrument.underlying.as_ref()?;
                    underlying
                        .raw_symbol
                        .as_deref()
                        .or(underlying.symbol.as_deref())
                })
        })
        .map(|ticker| ticker.trim().to_ascii_uppercase())
        .filter(|ticker| !ticker.is_empty())
        .collect::<BTreeSet<_>>();
    if !plaid_symbols.is_empty() && plaid_symbols == snap_symbols {
        return true;
    }

    !account_holdings.is_empty()
        && plaid_symbols.is_empty()
        && positions.is_empty()
        && account_holdings.iter().all(|holding| {
            securities
                .get(&holding.security_id)
                .is_some_and(|security| {
                    security.is_cash_equivalent == Some(true)
                        || security
                            .kind
                            .as_deref()
                            .is_some_and(|kind| kind.eq_ignore_ascii_case("cash"))
                })
        })
}

fn category_name(transaction: &PlaidTransaction) -> String {
    transaction
        .personal_finance_category
        .as_ref()
        .map(|category| category.primary.as_str())
        .or_else(|| transaction.category.as_ref()?.first().map(String::as_str))
        .unwrap_or("Other")
        .to_ascii_lowercase()
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_spending(amount: Decimal, category: &str, merchant: &str) -> bool {
    let description = format!("{category} {merchant}").to_ascii_lowercase();
    amount.is_sign_negative()
        && !["income", "transfer", "payment"]
            .iter()
            .any(|needle| description.contains(needle))
}

#[derive(Default)]
struct FifoState {
    lots: VecDeque<(Decimal, Decimal)>,
    sales: Vec<(usize, Decimal, Decimal)>,
    invalid: bool,
}

fn add_estimated_realized_gains(trades: &mut [Value], holdings: &[Value], activity_complete: bool) {
    if !activity_complete {
        return;
    }
    let mut states = BTreeMap::<(String, String), FifoState>::new();
    let mut order = (0..trades.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        (trades[*left]["date"].as_str(), trades[*left]["id"].as_str()).cmp(&(
            trades[*right]["date"].as_str(),
            trades[*right]["id"].as_str(),
        ))
    });

    for index in order {
        let trade = &trades[index];
        let Some(account_id) = trade["accountId"].as_str() else {
            continue;
        };
        let Some(ticker) = trade["ticker"]
            .as_str()
            .map(|ticker| ticker.trim().to_ascii_uppercase())
            .filter(|ticker| !ticker.is_empty())
        else {
            continue;
        };
        let state = states.entry((account_id.to_string(), ticker)).or_default();
        let kind = trade["type"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let Some(units) = decimal_value(&trade["units"])
            .map(|units| units.abs())
            .filter(|units| !units.is_zero())
        else {
            state.invalid = true;
            continue;
        };
        let amount = decimal_value(&trade["amount"])
            .map(|amount| amount.abs())
            .filter(|amount| !amount.is_zero())
            .or_else(|| decimal_value(&trade["price"]).map(|price| price.abs() * units));
        let Some(amount) = amount else {
            state.invalid = true;
            continue;
        };

        if kind.contains("BUY") || kind.contains("REINVEST") {
            state.lots.push_back((units, amount / units));
            continue;
        }
        if !kind.contains("SELL") || state.invalid {
            state.invalid = true;
            continue;
        }

        let mut remaining = units;
        let mut basis = Decimal::ZERO;
        while remaining > Decimal::ZERO {
            let Some((available, unit_cost)) = state.lots.pop_front() else {
                state.invalid = true;
                state.sales.clear();
                break;
            };
            let used = available.min(remaining);
            basis += used * unit_cost;
            remaining -= used;
            if available > used {
                state.lots.push_front((available - used, unit_cost));
            }
        }
        if !state.invalid {
            state.sales.push((index, basis, amount - basis));
        }
    }

    for ((account_id, ticker), state) in states {
        let imported_units = state.lots.iter().map(|(units, _)| *units).sum::<Decimal>();
        let imported_basis = state
            .lots
            .iter()
            .map(|(units, unit_cost)| *units * *unit_cost)
            .sum::<Decimal>();
        let positions = holdings
            .iter()
            .filter(|holding| {
                holding["accountId"].as_str() == Some(account_id.as_str())
                    && holding["ticker"]
                        .as_str()
                        .is_some_and(|value| value.eq_ignore_ascii_case(&ticker))
            })
            .collect::<Vec<_>>();
        let current_units = positions
            .iter()
            .filter_map(|holding| decimal_value(&holding["shares"]))
            .sum::<Decimal>();
        let current_basis = positions
            .iter()
            .filter_map(|holding| decimal_value(&holding["costBasis"]))
            .sum::<Decimal>();
        let basis_complete = positions.is_empty()
            || positions
                .iter()
                .all(|holding| decimal_value(&holding["costBasis"]).is_some());
        let unit_difference = (imported_units - current_units).abs();
        let average_unit_cost = if imported_units.is_zero() {
            Decimal::ZERO
        } else {
            imported_basis.abs() / imported_units.abs()
        };
        // SnapTrade activity units can be rounded to 3 decimals while positions retain 4.
        let basis_tolerance = Decimal::new(11, 3) + unit_difference * average_unit_cost;
        if state.invalid
            || !basis_complete
            || unit_difference > Decimal::new(5, 4)
            || (imported_basis - current_basis).abs() > basis_tolerance
        {
            continue;
        }
        for (index, basis, gain) in state.sales {
            trades[index]["realizedCostBasis"] = json_decimal(basis);
            trades[index]["estimatedRealizedGain"] = json_decimal(gain);
            trades[index]["estimatedRealizedGainPct"] = if basis.is_zero() {
                Value::Null
            } else {
                json_scaled(gain / basis.abs() * Decimal::ONE_HUNDRED, 4)
            };
            trades[index]["realizedGainMethod"] = "estimated-fifo".into();
        }
    }
}

fn add_investment_metrics(
    accounts: &mut [Value],
    holdings: &mut [Value],
    transactions: &[Value],
    trades: &[Value],
    year: &str,
) {
    for holding in holdings.iter_mut() {
        holding["unrealizedGain"] = decimal_value(&holding["value"])
            .zip(decimal_value(&holding["costBasis"]))
            .map(|(value, basis)| json_decimal(value - basis))
            .unwrap_or(Value::Null);
    }

    for account in accounts
        .iter_mut()
        .filter(|account| matches!(account["type"].as_str(), Some("brokerage" | "retirement")))
    {
        let account_id = account["id"].as_str().unwrap_or_default().to_string();
        let positions = holdings
            .iter()
            .filter(|holding| holding["accountId"].as_str() == Some(account_id.as_str()))
            .collect::<Vec<_>>();
        let known = positions
            .iter()
            .filter_map(|holding| {
                decimal_value(&holding["value"]).zip(decimal_value(&holding["costBasis"]))
            })
            .collect::<Vec<_>>();
        let coverage = if known.is_empty() {
            "unavailable"
        } else if known.len() == positions.len() {
            "complete"
        } else {
            "partial"
        };
        account["costBasisCoverage"] = coverage.into();
        if !known.is_empty() {
            let basis = known.iter().map(|(_, basis)| *basis).sum::<Decimal>();
            let gain = known
                .iter()
                .map(|(value, basis)| *value - *basis)
                .sum::<Decimal>();
            account["knownCostBasis"] = json_decimal(basis);
            account["knownUnrealizedGain"] = json_decimal(gain);
            account["knownUnrealizedGainPct"] = if basis.is_zero() {
                Value::Null
            } else {
                json_scaled(gain / basis.abs() * Decimal::ONE_HUNDRED, 4)
            };
        }
        let income = transactions
            .iter()
            .filter(|transaction| transaction["accountId"].as_str() == Some(account_id.as_str()))
            .filter(|transaction| {
                transaction["date"]
                    .as_str()
                    .is_some_and(|date| date.starts_with(year))
            })
            .filter(|transaction| {
                let label = format!(
                    "{} {}",
                    transaction["category"].as_str().unwrap_or_default(),
                    transaction["merchant"].as_str().unwrap_or_default()
                )
                .to_ascii_lowercase();
                label.contains("dividend") || label.contains("interest")
            })
            .filter_map(|transaction| decimal_value(&transaction["amount"]))
            .filter(|amount| amount.is_sign_positive())
            .sum::<Decimal>();
        account["investmentIncomeYtd"] = json_decimal(income);
        let sales = trades
            .iter()
            .filter(|trade| trade["accountId"].as_str() == Some(account_id.as_str()))
            .filter(|trade| {
                trade["type"]
                    .as_str()
                    .is_some_and(|kind| kind.to_ascii_uppercase().contains("SELL"))
                    && trade["date"]
                        .as_str()
                        .is_some_and(|date| date.starts_with(year))
            })
            .collect::<Vec<_>>();
        account["salesYtd"] = sales.len().into();
        account["saleProceedsYtd"] = json_decimal(
            sales
                .iter()
                .filter_map(|trade| decimal_value(&trade["amount"]))
                .map(|amount| amount.abs())
                .sum(),
        );
        let estimated = sales
            .iter()
            .filter_map(|trade| decimal_value(&trade["estimatedRealizedGain"]))
            .collect::<Vec<_>>();
        account["realizedGainCoverage"] = if estimated.is_empty() {
            "unavailable"
        } else if estimated.len() == sales.len() {
            "complete"
        } else {
            "partial"
        }
        .into();
        if !estimated.is_empty() {
            account["estimatedRealizedGainYtd"] = json_decimal(estimated.into_iter().sum());
        }
    }
}

fn spending_projection(transactions: &[Value], month_start: NaiveDate) -> Value {
    let mut spending_by_category = BTreeMap::<String, Decimal>::new();
    for transaction in transactions {
        let Some(posted) = iso_date(transaction["postedOn"].as_str()) else {
            continue;
        };
        let Ok(date) = NaiveDate::parse_from_str(&posted, "%Y-%m-%d") else {
            continue;
        };
        let amount = decimal_value(&transaction["amount"]).unwrap_or_default();
        let category = transaction["category"].as_str().unwrap_or("Other");
        let merchant = transaction["merchant"].as_str().unwrap_or("Transaction");
        if date >= month_start && is_spending(amount, category, merchant) {
            *spending_by_category
                .entry(category.to_string())
                .or_default() += amount.abs();
        }
    }
    let month_total = spending_by_category.values().copied().sum::<Decimal>();
    let mut category_values = spending_by_category.into_iter().collect::<Vec<_>>();
    category_values.sort_by(|left, right| right.1.cmp(&left.1));
    let categories = category_values
        .into_iter()
        .enumerate()
        .map(|(index, (name, value))| {
            let color = if name.to_ascii_lowercase().contains("food")
                || name.to_ascii_lowercase().contains("dining")
            {
                "#c9684b"
            } else {
                SPENDING_COLORS[index % SPENDING_COLORS.len()]
            };
            let percent = if month_total.is_zero() {
                Decimal::ZERO
            } else {
                value / month_total * Decimal::ONE_HUNDRED
            };
            json!({ "name": name, "value": json_decimal(value), "percent": json_decimal(percent), "color": color })
        })
        .collect::<Vec<_>>();
    json!({ "monthTotal": json_decimal(month_total), "categories": categories })
}

fn parse_points(
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

fn is_reinvestment(activity: &SnapActivity) -> bool {
    let kind = activity.kind.to_ascii_uppercase();
    kind == "REI"
        || kind.contains("REINVEST")
        || (kind.contains("DIVIDEND")
            && activity.units.is_some_and(|units| units > Decimal::ZERO)
            && activity.amount.is_sign_negative()
            && activity
                .description
                .as_deref()
                .is_some_and(|description| description.to_ascii_uppercase().contains("REINVEST")))
}

fn activity_ticker(activity: &SnapActivity) -> String {
    activity
        .symbol
        .as_ref()
        .and_then(|symbol| symbol.raw_symbol.as_ref().or(symbol.symbol.as_ref()))
        .cloned()
        .unwrap_or_default()
}

fn reconciled_acquisitions(
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
            let kind = activity.kind.to_ascii_uppercase();
            let description = activity.description.as_deref()?.to_ascii_lowercase();
            let quantity = activity.units?;
            (kind.contains("TRANSFER")
                && activity.amount == Decimal::ZERO
                && quantity > Decimal::ZERO
                && description.contains("allocate shares")
                && activity_ticker(activity).eq_ignore_ascii_case(ticker))
            .then_some(Acquisition {
                date: iso_date(
                    activity
                        .trade_date
                        .as_deref()
                        .or(activity.settlement_date.as_deref()),
                )?,
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

fn holding_history(
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

fn combine_value_histories(histories: &[Vec<(String, Decimal)>]) -> Vec<(String, Decimal)> {
    if histories.is_empty() {
        return Vec::new();
    }
    let starts = histories
        .iter()
        .filter_map(|history| history.first().map(|point| point.0.clone()))
        .collect::<Vec<_>>();
    if starts.len() != histories.len() {
        return Vec::new();
    }
    let coverage_start = starts.into_iter().max().unwrap_or_default();
    let dates = histories
        .iter()
        .flat_map(|history| history.iter().map(|point| point.0.clone()))
        .filter(|date| date >= &coverage_start)
        .collect::<BTreeSet<_>>();
    dates
        .into_iter()
        .filter_map(|date| {
            let values = histories
                .iter()
                .map(|history| {
                    history
                        .iter()
                        .rev()
                        .find(|point| point.0 <= date)
                        .map(|point| point.1)
                })
                .collect::<Option<Vec<_>>>()?;
            Some((date, rounded(values.into_iter().sum())))
        })
        .collect()
}

fn reconstructed_plaid_histories(
    accounts: &[PlaidAccount],
    transactions: &[PlaidTransaction],
    history_starts: &BTreeMap<String, String>,
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

fn reconstructed_net_worth_history(
    mut histories: Vec<Vec<(String, Decimal)>>,
    investment_accounts: &[Value],
) -> Vec<(String, Decimal)> {
    for account in investment_accounts {
        if account["historySource"] == "unavailable" {
            return Vec::new();
        }
        let Some(points) = account["points"].as_array().and_then(|points| {
            points
                .iter()
                .map(|point| {
                    Some((
                        iso_date(point["date"].as_str())?,
                        decimal_value(&point["value"])?,
                    ))
                })
                .collect::<Option<Vec<_>>>()
        }) else {
            return Vec::new();
        };
        if points.len() < 2 {
            return Vec::new();
        }
        histories.push(points);
    }
    combine_value_histories(&histories)
}

fn merged_net_worth_history(
    estimated: Vec<(String, Decimal)>,
    observed: &[Value],
) -> (Vec<Value>, bool) {
    let observed = observed
        .iter()
        .filter_map(|point| {
            Some((
                iso_date(point["date"].as_str())?,
                decimal_value(&point["value"])?,
            ))
        })
        .collect::<BTreeMap<_, _>>();
    let observed_dates = observed.keys().cloned().collect::<BTreeSet<_>>();
    let mut history = estimated.into_iter().collect::<BTreeMap<_, _>>();
    history.extend(observed);
    let points = history
        .into_iter()
        .rev()
        .take(730)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    let has_estimates = points
        .iter()
        .any(|(date, _)| !observed_dates.contains(date));
    (
        points
            .into_iter()
            .map(|(date, value)| json!({ "date": date, "value": json_decimal(value) }))
            .collect(),
        has_estimates,
    )
}

fn external_flow(activity: &SnapActivity) -> Option<Decimal> {
    let kind = activity
        .kind
        .trim()
        .to_ascii_uppercase()
        .replace(|character: char| !character.is_ascii_alphanumeric(), "_");
    if kind.contains("CONTRIBUTION") || kind == "DEPOSIT" || kind.ends_with("_TRANSFER_IN") {
        Some(activity.amount.abs())
    } else if kind.contains("WITHDRAWAL") || kind.ends_with("_TRANSFER_OUT") {
        Some(-activity.amount.abs())
    } else if kind == "TRANSFER" || kind == "CASH_TRANSFER" {
        Some(activity.amount)
    } else {
        None
    }
}

fn external_flows(activities: &[SnapActivity]) -> BTreeMap<String, Decimal> {
    let mut flows = BTreeMap::new();
    for activity in activities {
        let Some(amount) = external_flow(activity) else {
            continue;
        };
        let Some(date) = iso_date(
            activity
                .trade_date
                .as_deref()
                .or(activity.settlement_date.as_deref()),
        ) else {
            continue;
        };
        *flows.entry(date).or_default() += amount;
    }
    flows
}

fn performance_record(
    account_id: String,
    name: String,
    institution: String,
    current_value: Decimal,
    mut points: Vec<(String, Decimal)>,
    today: &str,
    source: &str,
    flows: Option<&BTreeMap<String, Decimal>>,
    benchmark: &[(String, Decimal)],
) -> Value {
    points.retain(|(date, _)| date.as_str() <= today);
    if points.last().is_none_or(|point| point.0 != today) {
        points.push((today.to_string(), current_value));
    } else if let Some(last) = points.last_mut() {
        last.1 = current_value;
    }
    let history_source = if points.len() > 1 {
        source
    } else {
        "unavailable"
    };
    let comparisons_available = points.len() > 1 && flows.is_some();
    let history_start = points.first().map(|point| point.0.clone());
    let mut net_deposits = points.first().map(|point| point.1).unwrap_or_default();
    let mut benchmark_value = net_deposits;
    let mut benchmark_cursor = 0;
    let mut previous_benchmark_price = None;
    let mut prior_point_benchmark_price: Option<Decimal> = None;
    let known_flows = flows
        .map(|values| values.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut flow_cursor = 0;
    let mut prior_value: Option<Decimal> = None;
    let projected_points = points
        .into_iter()
        .enumerate()
        .map(|(index, (date, value))| {
            while benchmark_cursor < benchmark.len() && benchmark[benchmark_cursor].0 <= date {
                previous_benchmark_price = Some(benchmark[benchmark_cursor].1);
                benchmark_cursor += 1;
            }
            let benchmark_price = previous_benchmark_price;
            let mut flow = Decimal::ZERO;
            if index == 0 {
                // The opening observation already contains flows on or before its date.
                while flow_cursor < known_flows.len() && known_flows[flow_cursor].0 <= &date {
                    flow_cursor += 1;
                }
            } else {
                if let (Some(prior), Some(current)) =
                    (prior_point_benchmark_price, benchmark_price)
                {
                    if !prior.is_zero() {
                        benchmark_value *= current / prior;
                    }
                }
                while flow_cursor < known_flows.len() && known_flows[flow_cursor].0 <= &date {
                    flow += *known_flows[flow_cursor].1;
                    flow_cursor += 1;
                }
                net_deposits += flow;
                benchmark_value += flow;
            }
            prior_point_benchmark_price = benchmark_price;
            let market_change = prior_value
                .filter(|_| comparisons_available)
                .map(|prior| value - prior - flow);
            let market_change_pct = prior_value
                .filter(|prior| comparisons_available && !prior.is_zero())
                .zip(market_change)
                .map(|(prior, change)| change / prior.abs() * Decimal::from(100));
            prior_value = Some(value);
            json!({
                "date": date,
                "value": json_decimal(value),
                "netDeposits": comparisons_available.then(|| json_decimal(net_deposits)),
                "sp500": (comparisons_available && benchmark_price.is_some()).then(|| json_decimal(benchmark_value)),
                "marketChange": optional_json_decimal(market_change),
                "marketChangePct": optional_json_scaled(market_change_pct, 4),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "accountId": account_id,
        "name": name,
        "institution": institution,
        "currentValue": json_decimal(current_value),
        "historySource": history_source,
        "historyStart": history_start,
        "performanceMethod": if comparisons_available { "value-with-comparisons" } else { "value-only" },
        "points": projected_points,
    })
}

fn combined_performance(accounts: &[Value]) -> Vec<Value> {
    if accounts.is_empty() {
        return Vec::new();
    }
    let date_sets = accounts
        .iter()
        .map(|account| {
            account["points"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|point| point["date"].as_str().map(str::to_string))
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let common = date_sets.iter().skip(1).fold(
        date_sets.first().cloned().unwrap_or_default(),
        |dates, next| dates.intersection(next).cloned().collect(),
    );
    let comparisons_available = accounts
        .iter()
        .all(|account| account["performanceMethod"] == "value-with-comparisons");
    let mut previous_value: Option<Decimal> = None;
    let points = common
        .into_iter()
        .map(|date| {
            let account_points = accounts
                .iter()
                .filter_map(|account| {
                    account["points"]
                        .as_array()?
                        .iter()
                        .find_map(|point| (point["date"].as_str() == Some(&date)).then_some(point))
                })
                .collect::<Vec<_>>();
            let value = account_points
                .iter()
                .filter_map(|point| decimal_value(&point["value"]))
                .sum::<Decimal>();
            let net_deposits = if comparisons_available {
                account_points
                    .iter()
                    .map(|point| decimal_value(&point["netDeposits"]))
                    .collect::<Option<Vec<_>>>()
                    .map(|values| values.into_iter().sum::<Decimal>())
            } else {
                None
            };
            let sp500 = if comparisons_available {
                account_points
                    .iter()
                    .map(|point| decimal_value(&point["sp500"]))
                    .collect::<Option<Vec<_>>>()
                    .map(|values| values.into_iter().sum::<Decimal>())
            } else {
                None
            };
            let market_change = if comparisons_available && previous_value.is_some() {
                account_points
                    .iter()
                    .map(|point| decimal_value(&point["marketChange"]))
                    .collect::<Option<Vec<_>>>()
                    .map(|values| values.into_iter().sum::<Decimal>())
            } else {
                None
            };
            let market_change_pct = previous_value
                .filter(|previous| !previous.is_zero())
                .zip(market_change)
                .map(|(previous, change)| change / previous.abs() * Decimal::from(100));
            previous_value = Some(value);
            json!({
                "date": date,
                "value": json_decimal(value),
                "netDeposits": net_deposits.map(json_decimal),
                "sp500": sp500.map(json_decimal),
                "marketChange": optional_json_decimal(market_change),
                "marketChangePct": optional_json_scaled(market_change_pct, 4),
            })
        })
        .collect::<Vec<_>>();
    let current_value = accounts
        .iter()
        .filter_map(|account| decimal_value(&account["currentValue"]))
        .sum::<Decimal>();
    let unavailable = accounts
        .iter()
        .any(|account| account["historySource"] == "unavailable");
    let source = if unavailable {
        "unavailable"
    } else {
        "estimated"
    };
    let mut result = vec![json!({
        "accountId": "total",
        "name": "Combined brokerages",
        "institution": format!("{} account{}", accounts.len(), if accounts.len() == 1 { "" } else { "s" }),
        "currentValue": json_decimal(current_value),
        "historySource": source,
        "historyStart": points.first().and_then(|point| point["date"].as_str()),
        "performanceMethod": if comparisons_available { "value-with-comparisons" } else { "value-only" },
        "points": points,
    })];
    result.extend_from_slice(accounts);
    result
}

fn observed_history(
    previous: &Value,
    current: Decimal,
    now: DateTime<Utc>,
    fresh: bool,
) -> Vec<Value> {
    let mut observations = previous["observedNetWorthHistory"]
        .as_array()
        .or_else(|| {
            (!previous["netWorthHistoryEstimated"]
                .as_bool()
                .unwrap_or(false))
            .then(|| previous["netWorthHistory"].as_array())
            .flatten()
        })
        .into_iter()
        .flatten()
        .filter_map(|point| {
            Some((
                iso_date(point["date"].as_str())?,
                decimal_value(&point["value"])?,
            ))
        })
        .collect::<BTreeMap<_, _>>();
    if fresh {
        observations.insert(
            now.with_timezone(&Local).date_naive().to_string(),
            rounded(current),
        );
    }
    observations
        .into_iter()
        .rev()
        .take(730)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|(date, value)| json!({ "date": date, "value": json_decimal(value) }))
        .collect()
}

fn snapshot_change(
    previous: &Value,
    current_accounts: &[Value],
    current_transactions: &[Value],
    refreshed: &BTreeSet<String>,
    observed_at: &str,
    current_incomplete: bool,
) -> Option<Value> {
    let previous_accounts = previous["accounts"]
        .as_array()?
        .iter()
        .filter_map(|account| {
            let id = account["id"].as_str()?;
            (id != "all").then(|| (id.to_string(), account))
        })
        .collect::<BTreeMap<_, _>>();
    if previous_accounts.is_empty() || previous["updatedAt"].as_str() == Some(observed_at) {
        return None;
    }
    let mut changes = current_accounts
        .iter()
        .filter(|account| account["id"] != "all")
        .filter_map(|account| {
            let id = account["id"].as_str()?;
            let prior = decimal_value(&previous_accounts.get(id)?["value"])?;
            let current = decimal_value(&account["value"])?;
            if !refreshed.is_empty() && !refreshed.contains(id) {
                return None;
            }
            let change = rounded(current - prior);
            (change.abs() >= Decimal::new(1, 2)).then(|| {
                json!({ "accountId": id, "name": account["name"], "change": json_decimal(change) })
            })
        })
        .collect::<Vec<_>>();
    changes.sort_by(|left, right| {
        decimal_value(&right["change"])
            .unwrap_or_default()
            .abs()
            .cmp(&decimal_value(&left["change"]).unwrap_or_default().abs())
    });
    let previous_transaction_ids = previous["transactions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|transaction| transaction["id"].as_str())
        .collect::<BTreeSet<_>>();
    let current_ids = current_accounts
        .iter()
        .filter_map(|account| account["id"].as_str())
        .filter(|id| *id != "all")
        .collect::<BTreeSet<_>>();
    let comparison_complete = !previous["netWorthIncomplete"].as_bool().unwrap_or(false)
        && !current_incomplete
        && current_ids.len() == previous_accounts.len()
        && current_ids
            .iter()
            .all(|id| previous_accounts.contains_key(*id))
        && (refreshed.is_empty() || current_ids.iter().all(|id| refreshed.contains(*id)));
    let change_total = changes
        .iter()
        .filter_map(|change| decimal_value(&change["change"]))
        .sum::<Decimal>();
    Some(json!({
        "observedAt": observed_at,
        "previousUpdatedAt": previous["updatedAt"],
        "previousNetWorth": previous["netWorth"],
        "netWorthChange": json_decimal(change_total),
        "comparisonComplete": comparison_complete,
        "accountChanges": changes,
        "newTransactionIds": current_transactions.iter().filter_map(|transaction| {
            let id = transaction["id"].as_str()?;
            (!previous_transaction_ids.contains(id)).then(|| id.to_string())
        }).collect::<Vec<_>>(),
    }))
}

fn merge_movements(previous: &Value, change: Option<&Value>) -> Vec<Value> {
    let mut movements = previous["accountMovements"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|movement| Some((movement["id"].as_str()?.to_string(), movement.clone())))
        .collect::<BTreeMap<_, _>>();
    if let Some(change) = change {
        let observed_at = change["observedAt"].as_str().unwrap_or_default();
        for account in change["accountChanges"].as_array().into_iter().flatten() {
            if let Some(account_id) = account["accountId"].as_str() {
                let id = format!("{observed_at}:{account_id}");
                movements.insert(
                    id.clone(),
                    json!({
                        "id": id,
                        "observedAt": observed_at,
                        "accountId": account_id,
                        "name": account["name"],
                        "change": account["change"],
                    }),
                );
            }
        }
    }
    let mut movements = movements.into_values().collect::<Vec<_>>();
    movements.sort_by(|left, right| {
        left["observedAt"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["observedAt"].as_str().unwrap_or_default())
    });
    movements
        .into_iter()
        .rev()
        .take(2_000)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

pub fn project(
    sync: &ProviderSync,
    account_links: &BTreeMap<String, String>,
    now: DateTime<Utc>,
) -> Result<Value, String> {
    let mut warnings = Vec::new();
    let plaid_accounts =
        parse_records::<PlaidAccount>(&sync.plaid.accounts, "Plaid account", &mut warnings);
    let plaid_investment_accounts = parse_records::<PlaidAccount>(
        &sync.plaid.investment_accounts,
        "Plaid investment account",
        &mut warnings,
    );
    let plaid_transactions = parse_records::<PlaidTransaction>(
        &sync.plaid.transactions,
        "Plaid transaction",
        &mut warnings,
    );
    let plaid_holdings =
        parse_records::<PlaidHolding>(&sync.plaid.holdings, "Plaid holding", &mut warnings);
    let plaid_securities =
        parse_records::<PlaidSecurity>(&sync.plaid.securities, "Plaid security", &mut warnings)
            .into_iter()
            .map(|security| (security.security_id.clone(), security))
            .collect::<BTreeMap<_, _>>();
    let snap_accounts =
        parse_records::<SnapAccount>(&sync.snaptrade.accounts, "SnapTrade account", &mut warnings);
    let snap_positions = sync
        .snaptrade
        .positions
        .iter()
        .map(|(id, records)| {
            (
                id.clone(),
                parse_records::<SnapPosition>(records, "SnapTrade position", &mut warnings),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let snap_activities = sync
        .snaptrade
        .activities
        .iter()
        .map(|(id, records)| {
            (
                id.clone(),
                parse_records::<SnapActivity>(records, "SnapTrade activity", &mut warnings),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut possible_duplicates = Vec::new();
    for plaid in &plaid_investment_accounts {
        for snap in &snap_accounts {
            if possible_duplicate(
                plaid,
                snap,
                &plaid_holdings,
                &plaid_securities,
                snap_positions
                    .get(&snap.id)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            ) {
                let plaid_id = format!("plaid:{}", plaid.account_id);
                let snap_id = format!("snaptrade:{}", snap.id);
                if account_links.get(&plaid_id) != Some(&snap_id) {
                    possible_duplicates.push(json!({
                        "plaidAccountId": plaid_id,
                        "snaptradeAccountId": snap_id,
                        "description": format!("{} and {} may represent the same account", text(plaid.name.as_deref(), "Plaid stock plan"), text(snap.name.as_deref(), "SnapTrade account")),
                    }));
                }
            }
        }
    }
    possible_duplicates.sort_by(|left, right| {
        left["plaidAccountId"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["plaidAccountId"].as_str().unwrap_or_default())
    });
    if !possible_duplicates.is_empty() {
        warnings.push("Possible duplicate investment accounts are included until you confirm a link in Settings".into());
    }
    let included_plaid_investments = plaid_investment_accounts
        .iter()
        .filter(|account| !account_links.contains_key(&format!("plaid:{}", account.account_id)))
        .collect::<Vec<_>>();
    let included_plaid_ids = included_plaid_investments
        .iter()
        .map(|account| account.account_id.as_str())
        .collect::<BTreeSet<_>>();

    let committed_at = now.to_rfc3339();
    let today = now.with_timezone(&Local).date_naive();
    let mut accounts = Vec::new();
    for account in &plaid_accounts {
        let is_credit = account.kind == "credit";
        let raw = usd_value(
            account.balances.current,
            account.balances.iso_currency_code.as_deref(),
        );
        let value = raw.map(|value| {
            if is_credit && !value.is_zero() {
                -value
            } else {
                value
            }
        });
        accounts.push(json!({
            "id": format!("plaid:{}", account.account_id),
            "name": text(account.name.as_deref(), "Account"),
            "institution": text(account.institution_name.as_deref(), "Unknown institution"),
            "type": if is_credit { "credit" } else { "cash" },
            "value": optional_json_decimal(value),
            "currency": account.balances.iso_currency_code,
            "balanceAsOf": account.balances.last_updated_datetime,
            "balanceFetchedAt": account.balance_fetched_at,
        }));
    }
    for account in &included_plaid_investments {
        let subtype = account
            .subtype
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let retirement = [
            "401",
            "403",
            "457",
            "ira",
            "roth",
            "retirement",
            "pension",
            "profit sharing",
            "thrift",
        ]
        .iter()
        .any(|kind| subtype.contains(kind));
        accounts.push(json!({
            "id": format!("plaid:{}", account.account_id),
            "name": text(account.name.as_deref(), "Investment account"),
            "institution": text(account.institution_name.as_deref(), "Unknown institution"),
            "type": if retirement { "retirement" } else { "brokerage" },
            "value": optional_json_decimal(usd_value(account.balances.current, account.balances.iso_currency_code.as_deref())),
            "currency": account.balances.iso_currency_code,
            "balanceAsOf": account.balances.last_updated_datetime,
            "balanceFetchedAt": account.balance_fetched_at,
        }));
    }
    for account in &snap_accounts {
        let total = account.balance.total.as_ref();
        let value = usd_value(
            total.and_then(|total| total.amount),
            total.and_then(|total| total.currency.as_deref()),
        );
        let descriptor = format!(
            "{} {}",
            account.name.as_deref().unwrap_or_default(),
            account.raw_type.as_deref().unwrap_or_default()
        )
        .to_ascii_lowercase();
        accounts.push(json!({
            "id": format!("snaptrade:{}", account.id),
            "name": text(account.name.as_deref().or(account.raw_type.as_deref()), "Investment account"),
            "institution": text(account.institution_name.as_deref(), "Unknown institution"),
            "type": if descriptor.contains("roth") { "retirement" } else { "brokerage" },
            "value": optional_json_decimal(value),
            "currency": total.and_then(|total| total.currency.clone()),
            "balanceAsOf": account.sync_status.as_ref().and_then(|status| status.holdings.as_ref()).and_then(|point| point.last_successful_sync.clone()),
            "balanceFetchedAt": account.balance_fetched_at,
            "positionsAsOf": sync.snaptrade.positions_as_of.get(&account.id).cloned().flatten(),
            "activityAsOf": account.sync_status.as_ref().and_then(|status| status.transactions.as_ref()).and_then(|point| point.last_successful_sync.clone()),
        }));
    }
    accounts.sort_by(|left, right| {
        left["id"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["id"].as_str().unwrap_or_default())
    });
    let incomplete = accounts.iter().any(|account| account["value"].is_null());
    let net_worth = accounts
        .iter()
        .filter_map(|account| decimal_value(&account["value"]))
        .sum::<Decimal>();
    accounts.insert(0, json!({
        "id": "all", "name": "All accounts", "institution": "Brief", "type": "combined", "value": json_decimal(net_worth), "balanceFetchedAt": committed_at,
    }));

    let mut holdings = Vec::new();
    for holding in plaid_holdings
        .iter()
        .filter(|holding| included_plaid_ids.contains(holding.account_id.as_str()))
    {
        let security = plaid_securities.get(&holding.security_id);
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
                included_plaid_investments
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
        let total_change = cost_basis
            .filter(|basis| !basis.is_zero())
            .and_then(|basis| {
                value.map(|value| rounded((value - basis) / basis.abs() * Decimal::ONE_HUNDRED))
            });
        let kind = security
            .and_then(|security| security.kind.as_deref())
            .unwrap_or("unknown");
        holdings.push(json!({
            "ticker": text(security.and_then(|security| security.ticker_symbol.as_deref()).or_else(|| security.and_then(|security| security.name.as_deref())), "—"),
            "name": text(security.and_then(|security| security.name.as_deref()).or_else(|| security.and_then(|security| security.ticker_symbol.as_deref())), "Investment position"),
            "accountId": format!("plaid:{}", holding.account_id),
            "shares": optional_json_scaled(Some(holding.quantity), 8),
            "price": optional_json_scaled(price, 8),
            "value": optional_json_decimal(value),
            "costBasis": optional_json_decimal(cost_basis),
            "dailyChangePct": Value::Null,
            "weeklyChangePct": Value::Null,
            "weeklyReferencePrice": Value::Null,
            "weeklyReferenceDate": Value::Null,
            "totalChangePct": optional_json_scaled(total_change, 4),
            "instrumentKind": kind,
            "currency": currency,
            "quoteEligible": currency == Some("USD") && kind == "equity",
            "valuationNote": value.is_none().then_some("USD valuation unavailable"),
            "marketAsOf": holding.institution_price_datetime.clone().or(holding.institution_price_as_of.clone()),
            "priceSource": "provider",
            "color": COLORS[holdings.len() % COLORS.len()],
        }));
    }
    for account in &snap_accounts {
        for position in snap_positions.get(&account.id).into_iter().flatten() {
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
            let total_change = cost_basis
                .filter(|basis| !basis.is_zero())
                .and_then(|basis| {
                    value.map(|value| rounded((value - basis) / basis.abs() * Decimal::ONE_HUNDRED))
                });
            holdings.push(json!({
                "ticker": text(ticker.map(String::as_str), "—"),
                "name": text(name.map(String::as_str), "Investment position"),
                "accountId": format!("snaptrade:{}", account.id),
                "shares": optional_json_scaled(position.units, 8),
                "price": optional_json_scaled(position.price, 8),
                "value": optional_json_decimal(value),
                "costBasis": optional_json_decimal(cost_basis),
                "dailyChangePct": Value::Null,
                "weeklyChangePct": Value::Null,
                "weeklyReferencePrice": Value::Null,
                "weeklyReferenceDate": Value::Null,
                "totalChangePct": optional_json_scaled(total_change, 4),
                "instrumentKind": kind,
                "currency": position.currency,
                "quoteEligible": value.is_some() && ["stock", "etf", "adr", "cef"].contains(&kind) && ticker.is_some_and(|ticker| ticker != "—"),
                "valuationNote": value.is_none().then_some(if position.currency.as_deref() != Some("USD") { "USD valuation unavailable" } else if !share_valued { "Instrument valuation unsupported" } else { "Position not priced by provider" }),
                "marketAsOf": Value::Null,
                "priceSource": "provider",
                "color": COLORS[holdings.len() % COLORS.len()],
            }));
        }
    }
    holdings.sort_by(|left, right| {
        (
            left["accountId"].as_str().unwrap_or_default(),
            left["ticker"].as_str().unwrap_or_default(),
        )
            .cmp(&(
                right["accountId"].as_str().unwrap_or_default(),
                right["ticker"].as_str().unwrap_or_default(),
            ))
    });

    let plaid_histories = reconstructed_plaid_histories(
        &plaid_accounts,
        &plaid_transactions,
        &sync.plaid.transaction_history_start,
        today,
    );
    let account_names = plaid_accounts
        .iter()
        .map(|account| {
            (
                account.account_id.as_str(),
                text(account.name.as_deref(), "Account"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut transactions = plaid_transactions.into_iter().filter_map(|transaction| {
        let posted_date = plaid_posted_date(&transaction)?;
        let occurred_date = iso_date(transaction.authorized_datetime.as_deref().or(transaction.authorized_date.as_deref()));
        let counterparty = transaction.counterparties.as_ref().and_then(|values| values.iter().find(|value| value.kind.as_deref() == Some("merchant")));
        let merchant = transaction.merchant_name.as_deref().or_else(|| counterparty.and_then(|value| value.name.as_deref())).or(transaction.name.as_deref()).unwrap_or("Transaction");
        let mut projected = json!({
            "id": transaction.transaction_id,
            "merchant": merchant,
            "description": transaction.name,
            "category": category_name(&transaction),
            "date": occurred_date.clone().unwrap_or_else(|| posted_date.clone()),
            "occurredOn": occurred_date,
            "postedOn": posted_date,
            "amount": json_decimal(-transaction.amount),
            "account": account_names.get(transaction.account_id.as_str()).cloned().unwrap_or_else(|| "Account".into()),
            "accountId": format!("plaid:{}", transaction.account_id),
            "pending": transaction.pending,
        });
        let logo_url = transaction.logo_url.as_deref().or_else(|| counterparty.and_then(|value| value.logo_url.as_deref()));
        let website = transaction.website.as_deref().or_else(|| counterparty.and_then(|value| value.website.as_deref()));
        if let Some(logo_url) = logo_url {
            projected["logoUrl"] = logo_url.into();
        }
        if let Some(website) = website {
            projected["website"] = website.into();
        }
        if let Some(logo_name) = transaction.merchant_name.as_deref().or_else(|| counterparty.and_then(|value| value.name.as_deref())) {
            projected["logoName"] = logo_name.into();
        }
        Some(projected)
    }).collect::<Vec<_>>();
    for account in &snap_accounts {
        for (index, activity) in snap_activities
            .get(&account.id)
            .into_iter()
            .flatten()
            .enumerate()
        {
            if is_reinvestment(activity) {
                continue;
            }
            let kind = activity.kind.to_ascii_uppercase();
            let (label, category, amount) = if kind.contains("DIVIDEND") {
                ("Dividend", "Dividend", activity.amount)
            } else if kind.contains("INTEREST") {
                ("Interest", "Interest", activity.amount)
            } else if kind.contains("FEE") {
                ("Investment fee", "Fees", activity.amount)
            } else if kind.contains("TAX") {
                ("Investment tax", "Taxes", activity.amount)
            } else if let Some(amount) = external_flow(activity) {
                (
                    if amount.is_sign_negative() {
                        "Transfer out"
                    } else {
                        "Transfer in"
                    },
                    "Transfer",
                    amount,
                )
            } else {
                continue;
            };
            let Some(date) = iso_date(
                activity
                    .trade_date
                    .as_deref()
                    .or(activity.settlement_date.as_deref()),
            ) else {
                continue;
            };
            let ticker = activity_ticker(activity);
            let merchant = if ticker.is_empty() {
                label.to_string()
            } else {
                format!("{label} · {ticker}")
            };
            transactions.push(json!({
                "id": format!("snaptrade:{}:{}", account.id, activity.id.clone().unwrap_or_else(|| format!("{date}:{kind}:{}:{index}", activity.amount))),
                "merchant": merchant,
                "description": activity.description,
                "category": category,
                "date": date,
                "occurredOn": iso_date(activity.trade_date.as_deref()),
                "postedOn": iso_date(activity.settlement_date.as_deref()),
                "amount": json_decimal(amount),
                "account": text(account.name.as_deref().or(account.raw_type.as_deref()), "Investment account"),
                "accountId": format!("snaptrade:{}", account.id),
                "pending": false,
            }));
        }
    }
    transactions.sort_by(|left, right| {
        right["date"]
            .as_str()
            .unwrap_or_default()
            .cmp(left["date"].as_str().unwrap_or_default())
            .then_with(|| {
                left["id"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right["id"].as_str().unwrap_or_default())
            })
    });

    let mut trades = Vec::new();
    for account in &snap_accounts {
        for (index, activity) in snap_activities
            .get(&account.id)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let kind = activity.kind.to_ascii_uppercase();
            let reinvestment = is_reinvestment(activity);
            if !reinvestment
                && !["BUY", "SELL", "TRADE"]
                    .iter()
                    .any(|needle| kind.contains(needle))
            {
                continue;
            }
            let Some(date) = iso_date(
                activity
                    .trade_date
                    .as_deref()
                    .or(activity.settlement_date.as_deref()),
            ) else {
                continue;
            };
            let ticker = activity_ticker(activity);
            let amount = if kind.contains("BUY") || reinvestment {
                -activity.amount.abs()
            } else {
                activity.amount.abs()
            };
            trades.push(json!({
                "id": activity.id.clone().unwrap_or_else(|| format!("snaptrade:{}:{date}:{kind}:{ticker}:{}:{index}", account.id, activity.amount)),
                "type": if reinvestment { "REINVESTMENT" } else { kind.as_str() },
                "date": date,
                "amount": json_decimal(amount),
                "account": text(account.name.as_deref().or(account.raw_type.as_deref()), "Investment account"),
                "accountId": format!("snaptrade:{}", account.id),
                "ticker": (!ticker.is_empty()).then_some(ticker),
                "description": activity.description,
                "units": activity.units.map(|value| json_scaled(value, 8)),
                "price": activity.price.map(|value| json_scaled(value, 8)),
            }));
        }
    }
    trades.sort_by(|left, right| {
        right["date"]
            .as_str()
            .unwrap_or_default()
            .cmp(left["date"].as_str().unwrap_or_default())
            .then_with(|| {
                left["id"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(right["id"].as_str().unwrap_or_default())
            })
    });
    add_estimated_realized_gains(&mut trades, &holdings, sync.snaptrade.activity_complete);

    let today = today.to_string();
    let benchmark = parse_points(
        sync.benchmark_history
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default(),
        "Alpaca benchmark history",
        &mut warnings,
    );
    let mut individual_performance = Vec::new();
    for account in &snap_accounts {
        let total = account.balance.total.as_ref();
        let Some(current) = usd_value(
            total.and_then(|value| value.amount),
            total.and_then(|value| value.currency.as_deref()),
        ) else {
            continue;
        };
        let history = sync
            .snaptrade
            .balance_history
            .get(&account.id)
            .map(|points| parse_points(points, "SnapTrade history", &mut warnings))
            .unwrap_or_default();
        let flows = external_flows(
            snap_activities
                .get(&account.id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
        let comparison_flows =
            (sync.snaptrade.activity_complete && sync.snaptrade.history_complete).then_some(&flows);
        individual_performance.push(performance_record(
            format!("snaptrade:{}", account.id),
            text(
                account.name.as_deref().or(account.raw_type.as_deref()),
                "Brokerage account",
            ),
            text(account.institution_name.as_deref(), "Brokerage"),
            current,
            history,
            &today,
            "provider-estimated",
            comparison_flows,
            &benchmark,
        ));
    }
    for account in &included_plaid_investments {
        let Some(current) = usd_value(
            account.balances.current,
            account.balances.iso_currency_code.as_deref(),
        ) else {
            continue;
        };
        let account_holdings = plaid_holdings
            .iter()
            .filter(|holding| holding.account_id == account.account_id)
            .collect::<Vec<_>>();
        let mut flows = BTreeMap::new();
        let histories = account_holdings
            .iter()
            .filter_map(|holding| {
                let security = plaid_securities.get(&holding.security_id)?;
                let ticker = security.ticker_symbol.as_deref()?;
                let acquisitions = reconciled_acquisitions(holding, ticker, &snap_activities);
                let prices = sync
                    .security_history
                    .get(&ticker.to_ascii_uppercase())
                    .map(|points| parse_points(points, "Alpaca security history", &mut warnings))
                    .unwrap_or_default();
                for acquisition in &acquisitions {
                    if let Some((_, price)) =
                        prices.iter().find(|(date, _)| date >= &acquisition.date)
                    {
                        *flows.entry(acquisition.date.clone()).or_default() +=
                            acquisition.quantity * *price;
                    }
                }
                (!acquisitions.is_empty() && !prices.is_empty())
                    .then(|| holding_history(acquisitions, &prices))
            })
            .collect::<Vec<_>>();
        let history = if histories.len() == account_holdings.len() {
            combine_value_histories(&histories)
        } else {
            Vec::new()
        };
        let comparison_flows = (!history.is_empty()).then_some(&flows);
        individual_performance.push(performance_record(
            format!("plaid:{}", account.account_id),
            text(account.name.as_deref(), "Stock Plan"),
            text(account.institution_name.as_deref(), "Brokerage"),
            current,
            history,
            &today,
            "estimated",
            comparison_flows,
            &benchmark,
        ));
    }
    individual_performance.sort_by(|left, right| {
        left["accountId"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["accountId"].as_str().unwrap_or_default())
    });
    let brokerage_performance = combined_performance(&individual_performance);
    let account_balance_history = plaid_accounts
        .iter()
        .filter_map(|account| {
            let history = plaid_histories.get(&account.account_id)?.clone();
            let current = history.last()?.1;
            Some(performance_record(
                format!("plaid:{}", account.account_id),
                text(account.name.as_deref(), "Account"),
                text(account.institution_name.as_deref(), "Unknown institution"),
                current,
                history,
                &today,
                "transaction-derived",
                None,
                &[],
            ))
        })
        .collect::<Vec<_>>();
    let estimated_history = if incomplete {
        Vec::new()
    } else if plaid_histories.len() == plaid_accounts.len() {
        reconstructed_net_worth_history(
            plaid_histories.values().cloned().collect(),
            &individual_performance,
        )
    } else {
        Vec::new()
    };
    add_investment_metrics(
        &mut accounts,
        &mut holdings,
        &transactions,
        &trades,
        &today[..4],
    );

    let month_start = now
        .with_timezone(&Local)
        .date_naive()
        .with_day(1)
        .unwrap_or_else(|| now.with_timezone(&Local).date_naive());
    let spending = spending_projection(&transactions, month_start);

    if incomplete {
        warnings.push("Some account balances have no supported USD value. The total includes known USD balances only.".into());
    }
    if holdings.iter().any(|holding| holding["value"].is_null()) {
        warnings.push("Some positions are unpriced or use unsupported instruments/currencies. Account totals remain provider-reported.".into());
    }
    let refreshed = sync
        .refreshed_account_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let change = snapshot_change(
        &sync.previous_snapshot,
        &accounts,
        &transactions,
        &refreshed,
        &committed_at,
        incomplete,
    );
    let movements = merge_movements(&sync.previous_snapshot, change.as_ref());
    let observations = observed_history(
        &sync.previous_snapshot,
        net_worth,
        now,
        sync.balances_fresh && !incomplete,
    );
    let (net_worth_history, net_worth_history_estimated) =
        merged_net_worth_history(estimated_history, &observations);
    let benchmark = benchmark
        .into_iter()
        .map(|(date, value)| json!({ "date": date, "value": json_decimal(value) }))
        .collect::<Vec<_>>();
    warnings.sort();
    warnings.dedup();

    let mut snapshot = Map::new();
    snapshot.insert("calculationVersion".into(), CALCULATION_VERSION.into());
    snapshot.insert("updatedAt".into(), committed_at.clone().into());
    snapshot.insert("netWorth".into(), json_decimal(net_worth));
    snapshot.insert("netWorthIncomplete".into(), incomplete.into());
    snapshot.insert("syncWarnings".into(), warnings.into());
    snapshot.insert("accounts".into(), accounts.into());
    snapshot.insert("holdings".into(), holdings.into());
    snapshot.insert("transactions".into(), transactions.into());
    snapshot.insert("trades".into(), trades.into());
    snapshot.insert("accountMovements".into(), movements.into());
    snapshot.insert(
        "observedNetWorthHistory".into(),
        observations.clone().into(),
    );
    snapshot.insert("netWorthHistory".into(), net_worth_history.into());
    snapshot.insert(
        "netWorthHistoryEstimated".into(),
        net_worth_history_estimated.into(),
    );
    snapshot.insert("benchmarkHistory".into(), benchmark.into());
    snapshot.insert("brokeragePerformance".into(), brokerage_performance.into());
    snapshot.insert(
        "accountBalanceHistory".into(),
        account_balance_history.into(),
    );
    snapshot.insert(
        "possibleDuplicateAccounts".into(),
        possible_duplicates.into(),
    );
    snapshot.insert(
        "accountLinks".into(),
        serde_json::to_value(account_links).map_err(|error| error.to_string())?,
    );
    snapshot.insert("spending".into(), spending);
    snapshot.insert(
        "provenance".into(),
        json!({
            "calculation": "rust",
            "benchmark": { "provider": "Alpaca", "adjustment": "all" },
            "stockPlanHistory": { "provider": "Alpaca", "adjustment": "split" },
        }),
    );
    if let Some(change) = change {
        snapshot.insert("lastChange".into(), change);
    }
    Ok(Value::Object(snapshot))
}

pub fn apply_annotations(
    snapshot: &Value,
    annotations: &BTreeMap<String, Annotation>,
) -> Result<Value, String> {
    let mut projected = snapshot.clone();
    let updated_at = projected["updatedAt"]
        .as_str()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .ok_or("Committed snapshot timestamp is invalid")?;
    let month_start = updated_at
        .with_timezone(&Local)
        .date_naive()
        .with_day(1)
        .ok_or("Committed snapshot calendar date is invalid")?;
    let transactions = projected["transactions"]
        .as_array_mut()
        .ok_or("Committed transactions are unavailable")?;
    for transaction in transactions.iter_mut() {
        let Some(annotation) = transaction["id"]
            .as_str()
            .and_then(|id| annotations.get(id))
        else {
            continue;
        };
        if let Some(category) = &annotation.category {
            transaction["category"] = category.clone().into();
        }
        if let Some(confirmed) = annotation.benefit_confirmed {
            transaction["benefitConfirmed"] = confirmed.into();
        }
    }
    projected["spending"] = spending_projection(transactions, month_start);
    Ok(projected)
}

pub fn apply_market_snapshots(
    snapshot: &Value,
    market: &BTreeMap<String, MarketSnapshot>,
) -> Result<Value, String> {
    let mut projected = snapshot.clone();
    let accounts = projected["accounts"]
        .as_array()
        .ok_or("Committed accounts are unavailable")?;
    let valued_accounts = accounts
        .iter()
        .filter_map(|account| {
            (!account["value"].is_null()).then(|| account["id"].as_str().map(str::to_string))?
        })
        .collect::<BTreeSet<_>>();
    let mut deltas = BTreeMap::<String, Decimal>::new();
    let mut costed_deltas = BTreeMap::<String, Decimal>::new();
    for holding in projected["holdings"]
        .as_array_mut()
        .ok_or("Committed holdings are unavailable")?
    {
        let Some(ticker) = holding["ticker"]
            .as_str()
            .map(|ticker| ticker.trim().to_ascii_uppercase())
        else {
            continue;
        };
        let Some(quote) = market.get(&ticker) else {
            continue;
        };
        let Ok(quote_time) = DateTime::parse_from_rfc3339(&quote.as_of) else {
            continue;
        };
        let account_id = holding["accountId"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if holding["marketAsOf"]
            .as_str()
            .and_then(|source| DateTime::parse_from_rfc3339(source).ok())
            .is_some_and(|source| quote_time < source)
            || holding["quoteEligible"].as_bool() != Some(true)
        {
            continue;
        }
        let Some(shares) = decimal_value(&holding["shares"]) else {
            continue;
        };
        let Some(prior) = decimal_value(&holding["value"]) else {
            continue;
        };
        let Ok(price) = Decimal::from_str(&quote.price.to_string()) else {
            continue;
        };
        let value = rounded(shares * price);
        if valued_accounts.contains(&account_id) {
            *deltas.entry(account_id.clone()).or_default() += value - prior;
            if decimal_value(&holding["costBasis"]).is_some() {
                *costed_deltas.entry(account_id).or_default() += value - prior;
            }
        }
        holding["price"] = json_scaled(price, 8);
        holding["value"] = json_decimal(value);
        holding["dailyChangePct"] = Value::from(quote.daily_change_pct);
        holding["weeklyChangePct"] = quote.weekly_change_pct.into();
        holding["weeklyReferencePrice"] = quote.weekly_reference_price.into();
        holding["weeklyReferenceDate"] = quote.weekly_reference_date.clone().into();
        holding["marketAsOf"] = quote.as_of.clone().into();
        holding["priceSource"] = "live-alpaca".into();
        holding["unrealizedGain"] = decimal_value(&holding["costBasis"])
            .map(|basis| json_decimal(value - basis))
            .unwrap_or(Value::Null);
        holding["totalChangePct"] = decimal_value(&holding["costBasis"])
            .filter(|basis| !basis.is_zero())
            .map(|basis| json_scaled((value - basis) / basis.abs() * Decimal::ONE_HUNDRED, 4))
            .unwrap_or(Value::Null);
    }
    let total_delta = deltas.values().copied().sum::<Decimal>();
    for account in projected["accounts"]
        .as_array_mut()
        .ok_or("Committed accounts are unavailable")?
    {
        let Some(value) = decimal_value(&account["value"]) else {
            continue;
        };
        let delta = if account["id"] == "all" {
            total_delta
        } else {
            deltas
                .get(account["id"].as_str().unwrap_or_default())
                .copied()
                .unwrap_or_default()
        };
        account["value"] = json_decimal(value + delta);
        if let (Some(gain), Some(costed_delta), Some(basis)) = (
            decimal_value(&account["knownUnrealizedGain"]),
            costed_deltas.get(account["id"].as_str().unwrap_or_default()),
            decimal_value(&account["knownCostBasis"]),
        ) {
            let gain = gain + *costed_delta;
            account["knownUnrealizedGain"] = json_decimal(gain);
            account["knownUnrealizedGainPct"] = if basis.is_zero() {
                Value::Null
            } else {
                json_scaled(gain / basis.abs() * Decimal::ONE_HUNDRED, 4)
            };
        }
    }
    let brokerage_delta = projected["brokeragePerformance"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|performance| performance["accountId"] != "total")
        .filter_map(|performance| deltas.get(performance["accountId"].as_str()?).copied())
        .sum::<Decimal>();
    for performance in projected["brokeragePerformance"]
        .as_array_mut()
        .ok_or("Committed performance is unavailable")?
    {
        let Some(value) = decimal_value(&performance["currentValue"]) else {
            continue;
        };
        let delta = if performance["accountId"] == "total" {
            brokerage_delta
        } else {
            deltas
                .get(performance["accountId"].as_str().unwrap_or_default())
                .copied()
                .unwrap_or_default()
        };
        performance["currentValue"] = json_decimal(value + delta);
    }
    let net_worth =
        decimal_value(&projected["netWorth"]).ok_or("Committed net worth is invalid")?;
    projected["netWorth"] = json_decimal(net_worth + total_delta);
    Ok(projected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{PlaidData, SnapTradeData};

    fn sync() -> ProviderSync {
        ProviderSync {
            sync_id: "sync".into(),
            balances_fresh: true,
            refreshed_account_ids: vec!["plaid:cash".into(), "snaptrade:brokerage".into()],
            previous_snapshot: serde_json::from_str(include_str!("../../src/data/empty.json"))
                .unwrap(),
            plaid: PlaidData {
                accounts: vec![json!({
                    "account_id": "cash", "name": "Checking", "institution_name": "Bank", "type": "depository",
                    "balance_fetched_at": "2026-09-09T07:29:00Z",
                    "balances": { "current": "100.105", "iso_currency_code": "USD", "last_updated_datetime": null }
                })],
                transactions: vec![json!({
                    "transaction_id": "coffee", "account_id": "cash", "amount": "5.125", "name": "Coffee", "merchant_name": "Coffee",
                    "pending": false, "date": "2026-09-08", "authorized_date": "2026-09-07",
                    "logo_url": "https://plaid-merchant-logos.plaid.com/coffee.png", "website": "https://coffee.example",
                    "personal_finance_category": { "primary": "FOOD_AND_DRINK" }
                })],
                ..PlaidData::default()
            },
            snaptrade: SnapTradeData {
                accounts: vec![json!({
                    "id": "brokerage", "name": "Brokerage", "institution_name": "Broker", "balance": { "total": { "amount": "200.105", "currency": "USD" } }
                })],
                balance_history: BTreeMap::from([(
                    "brokerage".into(),
                    vec![json!({ "date": "2026-09-08", "total_value": "190.00" })],
                )]),
                ..SnapTradeData::default()
            },
            benchmark_history: Value::Array(Vec::new()),
            security_history: BTreeMap::new(),
        }
    }

    #[test]
    fn projects_exact_totals_and_keeps_posted_and_authorized_dates() {
        let snapshot = project(
            &sync(),
            &BTreeMap::new(),
            DateTime::parse_from_rfc3339("2026-09-09T07:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
        assert_eq!(snapshot["netWorth"], 300.22);
        assert_eq!(snapshot["transactions"][0]["date"], "2026-09-07");
        assert_eq!(snapshot["transactions"][0]["postedOn"], "2026-09-08");
        assert_eq!(
            snapshot["transactions"][0]["logoUrl"],
            "https://plaid-merchant-logos.plaid.com/coffee.png"
        );
        assert_eq!(snapshot["transactions"][0]["logoName"], "Coffee");
        assert_eq!(
            snapshot["accounts"][1]["balanceFetchedAt"],
            "2026-09-09T07:29:00Z"
        );
        assert!(snapshot["accounts"][1]["balanceAsOf"].is_null());
        assert_eq!(
            snapshot["brokeragePerformance"][1]["historySource"],
            "provider-estimated"
        );
        assert!(snapshot["brokeragePerformance"][1]["points"][0]["netDeposits"].is_null());
        assert_eq!(snapshot["provenance"]["calculation"], "rust");
    }

    #[test]
    fn fractional_quantities_and_unit_prices_keep_fixed_scale_precision() {
        let mut sync = sync();
        sync.plaid.investment_accounts.push(json!({
            "account_id": "fractional", "name": "Fractional", "institution_name": "Broker",
            "type": "investment", "subtype": "brokerage", "balances": { "current": "0.15", "iso_currency_code": "USD" }
        }));
        sync.plaid.holdings.push(json!({
            "account_id": "fractional", "security_id": "security", "quantity": "0.00012345",
            "institution_price": "1234.56789123", "institution_value": "0.15", "iso_currency_code": "USD"
        }));
        sync.plaid.securities.push(json!({
            "security_id": "security", "ticker_symbol": "TEST", "name": "Test", "type": "equity", "iso_currency_code": "USD"
        }));
        let snapshot = project(&sync, &BTreeMap::new(), Utc::now()).unwrap();
        let holding = snapshot["holdings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|holding| holding["ticker"] == "TEST")
            .unwrap();
        assert_eq!(holding["shares"], 0.00012345);
        assert_eq!(holding["price"], 1234.56789123);
        assert_eq!(holding["value"], 0.15);
    }

    #[test]
    fn annotations_are_applied_by_the_native_view_projection() {
        let snapshot = project(
            &sync(),
            &BTreeMap::new(),
            DateTime::parse_from_rfc3339("2026-09-09T07:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
        let annotations = BTreeMap::from([(
            "coffee".into(),
            Annotation {
                category: Some("Travel".into()),
                reviewed: Some(true),
                benefit_confirmed: Some(true),
            },
        )]);
        let projected = apply_annotations(&snapshot, &annotations).unwrap();
        assert_eq!(projected["transactions"][0]["category"], "Travel");
        assert_eq!(projected["transactions"][0]["benefitConfirmed"], true);
        assert_eq!(projected["spending"]["monthTotal"], 5.13);
        assert_eq!(projected["spending"]["categories"][0]["name"], "Travel");
    }

    #[test]
    fn possible_duplicates_are_never_silently_removed() {
        let mut sync = sync();
        sync.plaid.investment_accounts.push(json!({
            "account_id": "plan", "name": "Stock Plan", "institution_name": "Broker", "type": "investment", "subtype": "stock plan", "mask": "1234",
            "balances": { "current": 50, "iso_currency_code": "USD" }
        }));
        sync.snaptrade.accounts[0]["number"] = "XXXX1234".into();
        let snapshot = project(&sync, &BTreeMap::new(), Utc::now()).unwrap();
        assert_eq!(
            snapshot["possibleDuplicateAccounts"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(snapshot["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|account| account["id"] == "plaid:plan"));

        let snapshot = project(
            &sync,
            &BTreeMap::from([("plaid:plan".into(), "snaptrade:brokerage".into())]),
            Utc::now(),
        )
        .unwrap();
        assert!(!snapshot["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|account| account["id"] == "plaid:plan"));
    }

    #[test]
    fn complete_history_applies_off_date_cash_transfers_to_comparisons() {
        let mut sync = sync();
        sync.snaptrade.history_complete = true;
        sync.snaptrade.activity_complete = true;
        sync.snaptrade.activities.insert(
            "brokerage".into(),
            vec![json!({
                "id": "deposit", "type": "TRANSFER", "amount": 10,
                "trade_date": "2026-09-09"
            })],
        );
        sync.benchmark_history = json!([
            { "date": "2026-09-08", "value": 100 },
            { "date": "2026-09-10", "value": 101 }
        ]);

        let snapshot = project(
            &sync,
            &BTreeMap::new(),
            DateTime::parse_from_rfc3339("2026-09-10T07:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
        let account = &snapshot["brokeragePerformance"][1];
        assert_eq!(account["performanceMethod"], "value-with-comparisons");
        assert_eq!(account["points"][1]["netDeposits"], 200.0);
        assert_eq!(account["points"][1]["sp500"], 201.9);
        assert_eq!(account["points"][1]["marketChange"], 0.11);
        assert_eq!(account["points"][1]["marketChangePct"], 0.0579);
        let transfer = snapshot["transactions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|transaction| transaction["category"] == "Transfer")
            .unwrap();
        assert_eq!(transfer["merchant"], "Transfer in");
        assert_eq!(transfer["amount"], 10.0);
        assert_eq!(transfer["accountId"], "snaptrade:brokerage");
    }

    #[test]
    fn projects_brokerage_income_and_known_unrealized_gain() {
        let mut sync = sync();
        sync.snaptrade.positions.insert(
            "brokerage".into(),
            vec![json!({
                "units": 2.0049, "price": 100, "cost_basis": 75.0611003, "currency": "USD",
                "instrument": { "kind": "stock", "symbol": "TEST", "description": "Test Security" }
            })],
        );
        sync.snaptrade
            .positions_as_of
            .insert("brokerage".into(), Some("2026-09-09T07:30:00Z".into()));
        sync.snaptrade.activity_complete = true;
        sync.snaptrade.activities.insert(
            "brokerage".into(),
            vec![
                json!({
                    "id": "purchase", "type": "BUY", "amount": 225,
                    "trade_date": "2025-12-01", "units": 3, "price": 75,
                    "symbol": { "symbol": "TEST" }
                }),
                json!({
                    "id": "dividend", "type": "DIVIDEND", "amount": 12.34,
                    "trade_date": "2026-06-01", "settlement_date": "2026-06-02",
                    "symbol": { "symbol": "TEST" }
                }),
                json!({
                    "id": "sale", "type": "SELL", "amount": 180,
                    "trade_date": "2026-07-01", "units": 1, "price": 180,
                    "symbol": { "symbol": "TEST" }
                }),
                json!({
                    "id": "reinvestment", "type": "DIVIDEND", "amount": -0.5,
                    "trade_date": "2026-08-01", "units": 0.005, "price": 100,
                    "description": "Test Security dividend reinvestment",
                    "symbol": { "symbol": "TEST" }
                }),
            ],
        );

        let snapshot = project(
            &sync,
            &BTreeMap::new(),
            DateTime::parse_from_rfc3339("2026-09-09T07:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
        let account = snapshot["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|account| account["id"] == "snaptrade:brokerage")
            .unwrap();
        assert_eq!(account["knownCostBasis"], 150.49);
        assert_eq!(account["knownUnrealizedGain"], 50.0);
        assert_eq!(account["knownUnrealizedGainPct"], 33.2248);
        assert_eq!(account["investmentIncomeYtd"], 12.34);
        assert_eq!(account["salesYtd"], 1);
        assert_eq!(account["saleProceedsYtd"], 180.0);
        assert_eq!(account["estimatedRealizedGainYtd"], 105.0);
        assert_eq!(account["realizedGainCoverage"], "complete");
        assert_eq!(account["costBasisCoverage"], "complete");
        assert_eq!(snapshot["holdings"][0]["unrealizedGain"], 50.0);
        assert!(snapshot["holdings"][0]["marketAsOf"].is_null());
        assert_eq!(
            snapshot["trades"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|trade| trade["type"] == "REINVESTMENT")
                .count(),
            1
        );
        let dividend = snapshot["transactions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|transaction| transaction["merchant"] == "Dividend · TEST")
            .unwrap();
        assert_eq!(dividend["postedOn"], "2026-06-02");
        let sale = snapshot["trades"]
            .as_array()
            .unwrap()
            .iter()
            .find(|trade| trade["type"] == "SELL")
            .unwrap();
        assert_eq!(sale["realizedCostBasis"], 75.0);
        assert_eq!(sale["estimatedRealizedGain"], 105.0);
        assert_eq!(sale["estimatedRealizedGainPct"], 140.0);
        assert_eq!(sale["realizedGainMethod"], "estimated-fifo");
    }

    #[test]
    fn matching_portfolios_offer_a_duplicate_candidate_without_provider_identity() {
        let mut sync = sync();
        sync.plaid.investment_accounts.push(json!({
            "account_id": "other-brokerage", "name": "Investments", "institution_name": "Provider A", "type": "investment", "subtype": "brokerage",
            "balances": { "current": 200, "iso_currency_code": "USD" }
        }));
        sync.plaid.holdings.push(json!({
            "account_id": "other-brokerage", "security_id": "test-security", "quantity": 2,
            "institution_price": 100, "institution_value": 200, "iso_currency_code": "USD"
        }));
        sync.plaid.securities.push(json!({
            "security_id": "test-security", "ticker_symbol": "TEST", "name": "Test Security", "type": "equity", "iso_currency_code": "USD"
        }));
        sync.snaptrade.positions.insert(
            "brokerage".into(),
            vec![json!({
                "units": 2, "price": 100, "currency": "USD",
                "instrument": { "kind": "stock", "symbol": "TEST", "description": "Test Security" }
            })],
        );

        let snapshot = project(&sync, &BTreeMap::new(), Utc::now()).unwrap();
        assert_eq!(
            snapshot["possibleDuplicateAccounts"][0]["plaidAccountId"],
            "plaid:other-brokerage"
        );
        assert_eq!(
            snapshot["possibleDuplicateAccounts"][0]["snaptradeAccountId"],
            "snaptrade:brokerage"
        );
    }

    #[test]
    fn malformed_optional_records_are_quarantined() {
        let mut sync = sync();
        sync.plaid
            .transactions
            .push(json!({ "not": "a transaction" }));
        let snapshot = project(&sync, &BTreeMap::new(), Utc::now()).unwrap();
        assert!(snapshot["syncWarnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("malformed")));
        assert_eq!(snapshot["transactions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn currency_isolation_marks_totals_incomplete_without_conversion() {
        let mut sync = sync();
        sync.plaid.accounts.push(json!({
            "account_id": "cad", "name": "Canadian", "institution_name": "Bank", "type": "depository",
            "balances": { "current": 999, "iso_currency_code": "CAD" }
        }));
        let snapshot = project(&sync, &BTreeMap::new(), Utc::now()).unwrap();
        assert_eq!(snapshot["netWorth"], 300.22);
        assert_eq!(snapshot["netWorthIncomplete"], true);
        assert!(snapshot["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|account| { account["id"] == "plaid:cad" && account["value"].is_null() }));
    }

    #[test]
    fn projection_is_independent_of_provider_record_order() {
        let mut first = sync();
        first.plaid.accounts.push(json!({
            "account_id": "another", "name": "Savings", "institution_name": "Bank", "type": "depository",
            "balances": { "current": 25, "iso_currency_code": "USD" }
        }));
        first.plaid.transactions.push(json!({
            "transaction_id": "breakfast", "account_id": "cash", "amount": 3, "name": "Breakfast",
            "pending": false, "date": "2026-09-08"
        }));
        let mut second = first.clone();
        second.plaid.accounts.reverse();
        second.plaid.transactions.reverse();
        let now = DateTime::parse_from_rfc3339("2026-09-09T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let first = project(&first, &BTreeMap::new(), now).unwrap();
        let second = project(&second, &BTreeMap::new(), now).unwrap();
        assert_eq!(first["accounts"], second["accounts"]);
        assert_eq!(first["transactions"], second["transactions"]);
        assert_eq!(first["netWorth"], second["netWorth"]);
    }

    #[test]
    fn stock_plan_history_includes_zero_before_the_first_acquisition() {
        assert_eq!(
            holding_history(
                vec![Acquisition {
                    date: "2026-09-08".into(),
                    quantity: Decimal::from(2),
                }],
                &[
                    ("2026-09-07".into(), Decimal::from(10)),
                    ("2026-09-08".into(), Decimal::from(11)),
                ],
            ),
            vec![
                ("2026-09-07".into(), Decimal::ZERO),
                ("2026-09-08".into(), Decimal::from(22)),
            ]
        );
    }

    #[test]
    fn reconstructs_net_worth_from_cash_transactions_and_investment_history() {
        let mut sync = sync();
        sync.plaid
            .transaction_history_start
            .insert("cash".into(), "2026-09-07".into());
        sync.snaptrade.balance_history.insert(
            "brokerage".into(),
            vec![
                json!({ "date": "2026-09-07", "total_value": 180 }),
                json!({ "date": "2026-09-08", "total_value": 190 }),
            ],
        );
        let snapshot = project(
            &sync,
            &BTreeMap::new(),
            DateTime::parse_from_rfc3339("2026-09-09T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
        assert_eq!(
            snapshot["netWorthHistory"],
            json!([
                { "date": "2026-09-07", "value": 285.24 },
                { "date": "2026-09-08", "value": 290.11 },
                { "date": "2026-09-09", "value": 300.22 }
            ])
        );
        assert_eq!(
            snapshot["accountBalanceHistory"][0],
            json!({
                "accountId": "plaid:cash",
                "name": "Checking",
                "institution": "Bank",
                "currentValue": 100.11,
                "historySource": "transaction-derived",
                "historyStart": "2026-09-07",
                "performanceMethod": "value-only",
                "points": [
                    { "date": "2026-09-07", "value": 105.24, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null },
                    { "date": "2026-09-08", "value": 100.11, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null },
                    { "date": "2026-09-09", "value": 100.11, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null }
                ]
            })
        );
        assert_eq!(
            snapshot["observedNetWorthHistory"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(snapshot["netWorthHistoryEstimated"], true);
    }

    #[test]
    fn reconstructs_credit_history_as_a_negative_liability() {
        let mut sync = sync();
        sync.plaid.accounts.push(json!({
            "account_id": "card", "name": "Card", "institution_name": "Bank", "type": "credit",
            "balances": { "current": 250, "iso_currency_code": "USD" }
        }));
        sync.plaid.transactions.push(json!({
            "transaction_id": "purchase", "account_id": "card", "amount": 40,
            "name": "Purchase", "pending": false, "date": "2026-09-08"
        }));
        sync.plaid
            .transaction_history_start
            .insert("card".into(), "2026-09-07".into());

        let snapshot = project(
            &sync,
            &BTreeMap::new(),
            DateTime::parse_from_rfc3339("2026-09-09T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .unwrap();
        let history = snapshot["accountBalanceHistory"]
            .as_array()
            .unwrap()
            .iter()
            .find(|history| history["accountId"] == "plaid:card")
            .unwrap();
        assert_eq!(history["currentValue"], -250.0);
        assert_eq!(
            history["points"],
            json!([
                { "date": "2026-09-07", "value": -210.0, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null },
                { "date": "2026-09-08", "value": -250.0, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null },
                { "date": "2026-09-09", "value": -250.0, "netDeposits": null, "sp500": null, "marketChange": null, "marketChangePct": null }
            ])
        );
    }

    #[test]
    fn live_quotes_use_price_freshness_not_position_freshness() {
        let mut committed = project(&sync(), &BTreeMap::new(), Utc::now()).unwrap();
        committed["accounts"][1]["positionsAsOf"] = "2026-09-09T16:00:00Z".into();
        committed["holdings"] = json!([{
            "ticker": "ONE", "accountId": "plaid:cash", "shares": 2, "price": 50,
            "value": 100, "costBasis": 80, "quoteEligible": true
        }]);
        let old = BTreeMap::from([(
            "ONE".into(),
            MarketSnapshot {
                symbol: "ONE".into(),
                price: 60.0,
                previous_close: 55.0,
                daily_change_pct: 9.09,
                weekly_change_pct: Some(20.0),
                weekly_reference_price: Some(50.0),
                weekly_reference_date: Some("2026-09-02".into()),
                as_of: "2026-09-09T15:00:00Z".into(),
            },
        )]);
        let projected = apply_market_snapshots(&committed, &old).unwrap();
        assert_eq!(projected["netWorth"], 320.22);
        assert_eq!(projected["holdings"][0]["dailyChangePct"], 9.09);
        assert_eq!(projected["holdings"][0]["weeklyChangePct"], 20.0);

        committed["holdings"][0]["marketAsOf"] = "2026-09-09T16:00:00Z".into();
        assert_eq!(
            apply_market_snapshots(&committed, &old).unwrap()["netWorth"],
            committed["netWorth"]
        );

        let mut fresh = old;
        fresh.get_mut("ONE").unwrap().as_of = "2026-09-09T17:00:00Z".into();
        let projected = apply_market_snapshots(&committed, &fresh).unwrap();
        assert_eq!(projected["netWorth"], 320.22);
        assert_eq!(projected["holdings"][0]["weeklyReferencePrice"], 50.0);
        assert_eq!(
            projected["holdings"][0]["weeklyReferenceDate"],
            "2026-09-02"
        );
        assert_eq!(committed["netWorth"], 300.22);
    }
}

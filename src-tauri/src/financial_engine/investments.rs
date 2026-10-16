//! Brokerage valuation, income, and return calculations.
use super::*;

pub(super) fn calculated_snap_valuation(
    positions: &[SnapPosition],
    balances: &[Value],
) -> Option<SnapValuation> {
    let balances = balances
        .iter()
        .map(|value| serde_json::from_value::<SnapCashBalance>(value.clone()).ok())
        .collect::<Option<Vec<_>>>()?;
    if balances.is_empty()
        || balances
            .iter()
            .any(|balance| balance.currency.code != "USD")
    {
        return None;
    }
    let cash = balances
        .iter()
        .map(|balance| balance.cash)
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .sum::<Decimal>();
    let positions = positions
        .iter()
        .map(|position| {
            if position.currency.as_deref() != Some("USD") {
                return None;
            }
            if position.cash_equivalent {
                return Some(Decimal::ZERO);
            }
            if !["stock", "etf", "adr", "cef", "mutualfund"]
                .contains(&position.instrument.kind.as_deref()?)
            {
                return None;
            }
            Some(position.units? * position.price?)
        })
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .sum::<Decimal>();
    Some(SnapValuation {
        cash: rounded(cash),
        invested: rounded(positions),
    })
}

/// Cash dividends in USD for the symbols held, from a year back through those announced.
pub(super) fn dividend_history(
    actions: &[CorporateAction],
    holdings: &[Holding],
    today: NaiveDate,
) -> Vec<DividendRecord> {
    let held = holdings
        .iter()
        .map(|holding| holding.ticker.to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    let from = today - Duration::days(400);
    actions
        .iter()
        .filter_map(|action| {
            let CorporateAction::Dividend {
                symbol,
                ex_date,
                payable_date,
                rate,
                special,
                currency,
            } = action
            else {
                return None;
            };
            let symbol = symbol.to_ascii_uppercase();
            (currency == "USD" && held.contains(&symbol) && *ex_date >= from).then(|| {
                DividendRecord {
                    ex_date: ex_date.to_string(),
                    payable_date: payable_date.map(|date| date.to_string()),
                    rate: decimal_f64(*rate).map(|rate| scaled(rate, 6)),
                    special: *special,
                    symbol,
                }
            })
        })
        .collect()
}

pub(super) fn add_investment_metrics(
    accounts: &mut [Account],
    holdings: &mut [Holding],
    transactions: &[Transaction],
    trades: &[Trade],
    year: &str,
) {
    let known = |holding: &Holding| {
        holding
            .value
            .and_then(decimal_f64)
            .zip(holding.cost_basis.and_then(decimal_f64))
    };
    for holding in holdings.iter_mut() {
        holding.unrealized_gain = known(holding).map(|(value, basis)| money(value - basis));
    }
    let coverage = |known: usize, all: usize| {
        if known == 0 {
            "unavailable"
        } else if known == all {
            "complete"
        } else {
            "partial"
        }
        .to_owned()
    };

    for account in accounts
        .iter_mut()
        .filter(|account| matches!(account.r#type.as_str(), "brokerage" | "retirement"))
    {
        let positions = holdings
            .iter()
            .filter(|holding| holding.account_id == account.id)
            .collect::<Vec<_>>();
        let costed = positions
            .iter()
            .filter_map(|holding| known(holding))
            .collect::<Vec<_>>();
        account.cost_basis_coverage = Some(coverage(costed.len(), positions.len()));
        if !costed.is_empty() {
            let basis = costed.iter().map(|(_, basis)| *basis).sum::<Decimal>();
            let gain = costed
                .iter()
                .map(|(value, basis)| *value - *basis)
                .sum::<Decimal>();
            account.known_cost_basis = Some(money(basis));
            account.known_unrealized_gain = Some(money(gain));
            account.known_unrealized_gain_pct =
                (!basis.is_zero()).then(|| scaled(gain / basis.abs() * Decimal::ONE_HUNDRED, 4));
        }
        account.investment_income_ytd = Some(investment_income(transactions, &account.id, year));
        let sales = trades
            .iter()
            .filter(|trade| {
                trade.account_id == account.id
                    && trade.r#type.to_ascii_uppercase().contains("SELL")
                    && trade.date.starts_with(year)
            })
            .collect::<Vec<_>>();
        account.sales_ytd = Some(sales.len() as u64);
        account.sale_proceeds_ytd = Some(money(
            sales
                .iter()
                .filter_map(|trade| decimal_f64(trade.amount))
                .map(|amount| amount.abs())
                .sum(),
        ));
        let estimated = sales
            .iter()
            .filter_map(|trade| trade.estimated_realized_gain.and_then(decimal_f64))
            .collect::<Vec<_>>();
        account.realized_gain_coverage = Some(coverage(estimated.len(), sales.len()));
        if !estimated.is_empty() {
            account.estimated_realized_gain_ytd = Some(money(estimated.into_iter().sum()));
        }
    }
}

/// Dividends and interest posted to an account in `year`.
pub(super) fn investment_income(transactions: &[Transaction], account_id: &str, year: &str) -> f64 {
    money(
        transactions
            .iter()
            .filter(|t| t.account_id.as_deref() == Some(account_id) && !t.pending)
            .filter(|t| t.posted_date().starts_with(year))
            .filter(|t| {
                t.classification.as_ref().is_some_and(|classification| {
                    matches!(classification.mark, Mark::Dividend | Mark::Interest)
                })
            })
            .filter_map(|t| decimal_f64(t.amount))
            .filter(|amount| *amount > Decimal::ZERO)
            .sum::<Decimal>(),
    )
}

pub(super) fn is_reinvestment(activity: &SnapActivity) -> bool {
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

pub(super) fn activity_ticker(activity: &SnapActivity) -> String {
    activity
        .symbol
        .as_ref()
        .and_then(|symbol| symbol.raw_symbol.as_ref().or(symbol.symbol.as_ref()))
        .cloned()
        .unwrap_or_default()
}

pub(super) fn external_flow(activity: &SnapActivity) -> Option<Decimal> {
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

/// Shares a stock plan allocates (a vest), which arrive with no cash amount: the ticker and the
/// number of shares.
pub(super) fn share_allocation(activity: &SnapActivity) -> Option<(String, Decimal)> {
    let units = activity.units.filter(|units| *units > Decimal::ZERO)?;
    let ticker = activity_ticker(activity).trim().to_ascii_uppercase();
    (activity.kind.to_ascii_uppercase().contains("TRANSFER")
        && activity.amount.is_zero()
        && !ticker.is_empty()
        && activity
            .description
            .as_deref()?
            .to_ascii_lowercase()
            .contains("allocate shares"))
    .then_some((ticker, units))
}

/// Money and shares entering or leaving each account by date. Allocated shares are worth that
/// day's close (`close` gives a ticker's close on or before a date); an allocation without a
/// price is left out, as before.
pub(super) fn external_flows(
    activities: &[SnapActivity],
    close: &dyn Fn(&str, &str) -> Option<Decimal>,
) -> BTreeMap<String, Decimal> {
    let mut flows = BTreeMap::new();
    for activity in activities {
        let Some(date) = activity.date() else {
            continue;
        };
        let amount = match share_allocation(activity) {
            Some((ticker, units)) => close(&ticker, &date).map(|price| rounded(units * price)),
            None => external_flow(activity),
        };
        let Some(amount) = amount else {
            continue;
        };
        *flows.entry(date).or_default() += amount;
    }
    flows
}

/// The account a performance series belongs to.
pub(super) struct SeriesAccount {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) institution: String,
}

pub(super) fn performance_record(
    account: SeriesAccount,
    current_value: Decimal,
    mut points: Vec<(String, Decimal)>,
    today: &str,
    source: &str,
    flows: Option<&BTreeMap<String, Decimal>>,
    benchmark: &[(String, Decimal)],
) -> Performance {
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
                if let (Some(prior), Some(current)) = (prior_point_benchmark_price, benchmark_price)
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
            PerformancePoint {
                date,
                value: money(value),
                net_deposits: comparisons_available.then(|| money(net_deposits)),
                sp500: (comparisons_available && benchmark_price.is_some())
                    .then(|| money(benchmark_value)),
                market_change: market_change.map(money),
                market_change_pct: market_change_pct.map(|percent| scaled(percent, 4)),
            }
        })
        .collect::<Vec<_>>();
    let performance_method = if !comparisons_available {
        "value-only"
    } else if source == "estimated" {
        "modified-dietz"
    } else {
        "time-weighted"
    };
    Performance {
        account_id: account.id,
        name: account.name,
        institution: account.institution,
        current_value: money(current_value),
        history_source: Some(history_source.into()),
        history_start,
        performance_method: Some(performance_method.into()),
        points: projected_points,
    }
}

/// An account whose provider reported no balance history.
fn unavailable_history(account: &Performance) -> bool {
    account.history_source.as_deref() == Some("unavailable")
}

/// The combined history's dates: from the earliest date any account starts, each counting from
/// its first point. An account with no history would enter the total as a jump on its only day,
/// so then only the dates every account reports are summed.
fn combined_dates(accounts: &[Performance]) -> BTreeSet<String> {
    let date_sets = accounts
        .iter()
        .map(|account| {
            account
                .points
                .iter()
                .map(|point| point.date.clone())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let histories_known = accounts
        .iter()
        .zip(&date_sets)
        .all(|(account, dates)| !unavailable_history(account) && dates.len() > 1);
    if histories_known {
        date_sets.iter().flatten().cloned().collect::<BTreeSet<_>>()
    } else {
        date_sets.iter().skip(1).fold(
            date_sets.first().cloned().unwrap_or_default(),
            |dates, next| dates.intersection(next).cloned().collect(),
        )
    }
}

/// How the combined history measures performance: time-weighted only when every account is,
/// value-only when any account can't be compared with a benchmark.
fn combined_method(accounts: &[Performance]) -> &'static str {
    let methods = accounts
        .iter()
        .map(|account| account.performance_method.as_deref())
        .collect::<Vec<_>>();
    let comparisons_available = methods
        .iter()
        .all(|method| matches!(method, Some("time-weighted" | "modified-dietz")));
    if !comparisons_available {
        "value-only"
    } else if methods
        .iter()
        .all(|method| *method == Some("time-weighted"))
    {
        "time-weighted"
    } else {
        "modified-dietz"
    }
}

pub(super) fn combined_performance(accounts: &[Performance]) -> Vec<Performance> {
    if accounts.is_empty() {
        return Vec::new();
    }
    let dates = combined_dates(accounts);
    let performance_method = combined_method(accounts);
    let comparisons_available = performance_method != "value-only";
    let sum = |values: Vec<Option<Decimal>>| {
        values
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .map(|values| values.into_iter().sum::<Decimal>())
    };
    let mut previous_value: Option<Decimal> = None;
    let points = dates
        .into_iter()
        .map(|date| {
            // Each started account's point on this date, or its last one before it. A carried
            // point has no market change of its own; the account's next point reports it.
            let account_points = accounts
                .iter()
                .filter_map(|account| {
                    account
                        .points
                        .iter()
                        .rev()
                        .find(|point| point.date <= date)
                        .map(|point| (point, point.date == date))
                })
                .collect::<Vec<_>>();
            let value = account_points
                .iter()
                .filter_map(|(point, _)| decimal_f64(point.value))
                .sum::<Decimal>();
            let net_deposits = comparisons_available
                .then(|| {
                    sum(account_points
                        .iter()
                        .map(|(point, _)| point.net_deposits.and_then(decimal_f64))
                        .collect())
                })
                .flatten();
            let sp500 = comparisons_available
                .then(|| {
                    sum(account_points
                        .iter()
                        .map(|(point, _)| point.sp500.and_then(decimal_f64))
                        .collect())
                })
                .flatten();
            let market_change = (comparisons_available && previous_value.is_some())
                .then(|| {
                    sum(account_points
                        .iter()
                        .map(|(point, current)| {
                            if *current {
                                point.market_change.and_then(decimal_f64)
                            } else {
                                Some(Decimal::ZERO)
                            }
                        })
                        .collect())
                })
                .flatten();
            let market_change_pct = previous_value
                .filter(|previous| !previous.is_zero())
                .zip(market_change)
                .map(|(previous, change)| change / previous.abs() * Decimal::from(100));
            previous_value = Some(value);
            PerformancePoint {
                date,
                value: money(value),
                net_deposits: net_deposits.map(money),
                sp500: sp500.map(money),
                market_change: market_change.map(money),
                market_change_pct: market_change_pct.map(|percent| scaled(percent, 4)),
            }
        })
        .collect::<Vec<_>>();
    let current_value = accounts
        .iter()
        .filter_map(|account| decimal_f64(account.current_value))
        .sum::<Decimal>();
    let source = if accounts.iter().any(unavailable_history) {
        "unavailable"
    } else {
        "estimated"
    };
    let mut result = vec![Performance {
        account_id: "total".into(),
        name: "Combined brokerages".into(),
        institution: format!(
            "{} account{}",
            accounts.len(),
            if accounts.len() == 1 { "" } else { "s" }
        ),
        current_value: money(current_value),
        history_source: Some(source.into()),
        history_start: points.first().map(|point| point.date.clone()),
        performance_method: Some(performance_method.into()),
        points,
    }];
    result.extend_from_slice(accounts);
    result
}

pub(super) fn brokerage_balances_current(accounts: &[Account], now: DateTime<Utc>) -> bool {
    accounts
        .iter()
        .filter(|account| account.id.starts_with("snaptrade:"))
        .all(|account| {
            let Some(as_of) = account
                .balance_as_of
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            else {
                return false;
            };
            let age = now.signed_duration_since(as_of.with_timezone(&Utc));
            age >= Duration::zero() && age <= Duration::minutes(30)
        })
}

/// Trades and income from Plaid Investments Transactions, shaped like SnapTrade's.
pub(super) struct InvestmentActivity {
    pub(super) trades: Vec<Trade>,
    pub(super) transactions: Vec<Transaction>,
}

/// The row a Plaid cash investment record becomes: its label, category, and dividend type.
/// `leaves` says the cash left the account. Records with no cash row return `None`.
fn cash_activity(
    kind: &str,
    subtype: &str,
    has_security: bool,
    leaves: bool,
) -> Option<(&'static str, &'static str, Option<&'static str>)> {
    Some(match (kind, subtype) {
        ("cash", "qualified dividend") => ("Dividend", "Dividend", Some("qualified")),
        ("cash", "non-qualified dividend") => ("Dividend", "Dividend", Some("nonQualified")),
        ("cash", "dividend") => ("Dividend", "Dividend", None),
        // Fund distributions: long-term ones are taxed like qualified dividends.
        ("cash", "long-term capital gain") => {
            ("Capital gain distribution", "Dividend", Some("qualified"))
        }
        ("cash", "short-term capital gain") => (
            "Capital gain distribution",
            "Dividend",
            Some("nonQualified"),
        ),
        ("cash", "interest" | "interest receivable") => ("Interest", "Interest", None),
        ("cash", "tax" | "tax withheld" | "non-resident tax") => ("Investment tax", "Taxes", None),
        ("fee", _) => ("Investment fee", "Fees", None),
        ("cash", subtype) if subtype.contains("fee") => ("Investment fee", "Fees", None),
        ("cash", "contribution" | "deposit") => ("Transfer in", "Transfer", None),
        ("cash", "withdrawal" | "distribution") => ("Transfer out", "Transfer", None),
        ("transfer", _) if !has_security => (
            if leaves {
                "Transfer out"
            } else {
                "Transfer in"
            },
            "Transfer",
            None,
        ),
        _ => return None,
    })
}

/// Maps Plaid's investment transaction types to trades and to the dividend, interest, fee,
/// tax, and transfer rows SnapTrade activity produces. Plaid's amount is positive when cash
/// leaves the account; Brief's is positive when it arrives. Cancelled and cancelling records,
/// non-USD records, and in-kind transfers are left out.
pub(super) fn plaid_investment_activity(
    plaid: &crate::providers::PlaidData,
    included: &BTreeSet<&str>,
    securities: &BTreeMap<String, PlaidSecurity>,
    accounts: &[&PlaidAccount],
    warnings: &mut Vec<String>,
) -> InvestmentActivity {
    let records = parse_records::<PlaidInvestmentTransaction>(
        &plaid.investment_transactions,
        "Plaid investment transaction",
        warnings,
    );
    let cancelled = records
        .iter()
        .filter_map(|record| record.cancel_transaction_id.as_deref())
        .collect::<BTreeSet<_>>();
    let mut activity = InvestmentActivity {
        trades: Vec::new(),
        transactions: Vec::new(),
    };
    for record in &records {
        let kind = record.kind.to_ascii_lowercase();
        let subtype = record
            .subtype
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !included.contains(record.account_id.as_str())
            || kind == "cancel"
            || cancelled.contains(record.investment_transaction_id.as_str())
        {
            continue;
        }
        let account = accounts
            .iter()
            .find(|account| account.account_id == record.account_id);
        let currency = record
            .iso_currency_code
            .as_deref()
            .or_else(|| account.and_then(|account| account.balances.iso_currency_code.as_deref()));
        let (Some(amount), Some(date)) = (
            usd_value(Some(record.amount), currency),
            iso_date(Some(record.date.as_str())),
        ) else {
            continue;
        };
        let account_name = text(
            account.and_then(|account| account.name.as_deref()),
            "Investment account",
        );
        let account_id = format!("plaid:{}", record.account_id);
        let id = format!("plaid-investment:{}", record.investment_transaction_id);
        let ticker = record
            .security_id
            .as_deref()
            .and_then(|id| securities.get(id))
            .and_then(|security| security.ticker_symbol.as_deref())
            .map(|ticker| ticker.trim().to_ascii_uppercase())
            .filter(|ticker| !ticker.is_empty());
        if let Some(fee) = record.fees.filter(|fee| !fee.is_zero() && kind != "fee") {
            activity.transactions.push(Transaction {
                description: record.name.clone(),
                ..cash_row(
                    format!("{id}:fee"),
                    "Investment fee".into(),
                    "Fees",
                    date.clone(),
                    -fee.abs(),
                    &account_name,
                    &account_id,
                )
            });
        }
        if kind == "buy" || kind == "sell" {
            let units = record
                .quantity
                .map(|units| units.abs())
                .filter(|units| !units.is_zero());
            let reinvestment = subtype.contains("reinvest");
            activity.trades.push(Trade {
                id,
                r#type: if reinvestment {
                    "REINVESTMENT"
                } else if kind == "buy" {
                    "BUY"
                } else {
                    "SELL"
                }
                .into(),
                date,
                amount: money(if kind == "buy" {
                    -amount.abs()
                } else {
                    amount.abs()
                }),
                account: account_name,
                account_id,
                ticker,
                description: record.name.clone(),
                units: units.map(|value| scaled(value, 8)),
                price: record.price.map(|value| scaled(value.abs(), 8)),
                ..Trade::default()
            });
            continue;
        }
        let Some((label, category, dividend_type)) = cash_activity(
            &kind,
            &subtype,
            record.security_id.is_some(),
            amount.is_sign_positive(),
        ) else {
            continue;
        };
        let merchant = match &ticker {
            Some(ticker) if category == "Dividend" => format!("{label} · {ticker}"),
            _ => label.to_owned(),
        };
        let flow = (category == "Transfer").then_some(match subtype.as_str() {
            "contribution" => "contribution",
            "deposit" => "deposit",
            "withdrawal" | "distribution" => "withdrawal",
            _ => "transfer",
        });
        activity.transactions.push(Transaction {
            description: record.name.clone(),
            dividend_type: dividend_type.map(str::to_owned),
            flow: flow.map(str::to_owned),
            ..cash_row(
                id,
                merchant,
                category,
                date,
                -amount,
                &account_name,
                &account_id,
            )
        });
    }
    activity
}

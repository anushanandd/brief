//! Transaction categories, duplicate detection, and month spending.
use super::*;

pub(super) fn possible_duplicate(
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
    let snap_mask = snap.printed_number().map(last_four);
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

/// The top-level category and concise label for a Plaid transaction. Payment-app transfers
/// are labeled with the app Plaid names, such as Venmo or Zelle.
pub(super) fn category_name(
    transaction: &PlaidTransaction,
    payment_app: Option<&str>,
) -> super::categories::Categorized {
    match &transaction.personal_finance_category {
        Some(category) => {
            let mut categorized = super::categories::categorize(
                &category.primary,
                category.detailed.as_deref(),
                category.confidence_level.as_deref(),
            );
            if category
                .detailed
                .as_deref()
                .is_some_and(|code| code.ends_with("_FROM_APPS"))
            {
                categorized.label = non_blank(payment_app).or(categorized.label);
            }
            categorized
        }
        None => super::categories::Categorized {
            category: super::categories::title_case(
                transaction
                    .category
                    .as_ref()
                    .and_then(|values| values.first())
                    .map_or("Other", String::as_str),
            ),
            label: None,
        },
    }
}

/// This month's posted spending by category, largest first; a split counts each part under
/// its own category.
pub(super) fn spending(transactions: &[Transaction], month_start: NaiveDate) -> Spending {
    let mut by_category = BTreeMap::<String, Decimal>::new();
    for transaction in transactions {
        let Some(date) = iso_date(transaction.posted_on.as_deref())
            .and_then(|posted| NaiveDate::parse_from_str(&posted, "%Y-%m-%d").ok())
        else {
            continue;
        };
        let spent = transaction
            .classification
            .as_ref()
            .is_some_and(|classification| classification.spending);
        if date < month_start || !spent {
            continue;
        }
        for (category, amount) in transaction.parts() {
            *by_category.entry(category.to_owned()).or_default() +=
                decimal_f64(amount).unwrap_or_default().abs();
        }
    }
    let month_total = by_category.values().copied().sum::<Decimal>();
    let mut categories = by_category.into_iter().collect::<Vec<_>>();
    categories.sort_by_key(|(_, value)| std::cmp::Reverse(*value));
    Spending {
        month_total: money(month_total),
        categories: categories
            .into_iter()
            .enumerate()
            .map(|(index, (name, value))| {
                let lower = name.to_ascii_lowercase();
                let color = if lower.contains("food") || lower.contains("dining") {
                    "#c9684b"
                } else {
                    SPENDING_COLORS[index % SPENDING_COLORS.len()]
                };
                let percent = if month_total.is_zero() {
                    Decimal::ZERO
                } else {
                    value / month_total * Decimal::ONE_HUNDRED
                };
                Allocation {
                    name,
                    value: money(value),
                    percent: money(percent),
                    color: color.into(),
                }
            })
            .collect(),
    }
}

/// A Plaid transaction as the snapshot shows it, or `None` without a posted date. Fields
/// Plaid left empty are left out.
pub(super) fn plaid_transaction(
    transaction: PlaidTransaction,
    account_names: &BTreeMap<&str, String>,
) -> Option<Transaction> {
    let posted_date = plaid_posted_date(&transaction)?;
    let occurred_date = iso_date(
        transaction
            .authorized_datetime
            .as_deref()
            .or(transaction.authorized_date.as_deref()),
    );
    let counterparty = transaction.counterparties.as_ref().and_then(|values| {
        values
            .iter()
            .find(|value| value.kind.as_deref() == Some("merchant"))
    });
    let payment_app = transaction.counterparties.as_ref().and_then(|values| {
        values
            .iter()
            .find(|value| value.kind.as_deref() == Some("payment_app"))
    });
    let counterparty_name = counterparty.and_then(|value| value.name.as_deref());
    // A bank's raw descriptor is titled without its references; the description keeps them.
    let merchant = match transaction.merchant_name.as_deref().or(counterparty_name) {
        Some(name) => name.to_owned(),
        None => transaction
            .name
            .as_deref()
            .map_or_else(|| "Transaction".into(), super::descriptor::readable_title),
    };
    let categorized = category_name(
        &transaction,
        payment_app.and_then(|value| value.name.as_deref()),
    );
    let category = transaction.personal_finance_category.as_ref();
    Some(Transaction {
        id: transaction.transaction_id.clone(),
        merchant,
        description: transaction.name.clone(),
        category: categorized.category,
        subcategory: categorized.label,
        date: occurred_date.clone().unwrap_or_else(|| posted_date.clone()),
        occurred_on: occurred_date,
        posted_on: Some(posted_date),
        amount: money(-transaction.amount),
        account: account_names
            .get(transaction.account_id.as_str())
            .cloned()
            .unwrap_or_else(|| "Account".into()),
        account_id: Some(format!("plaid:{}", transaction.account_id)),
        pending: transaction.pending,
        location: transaction
            .location
            .as_ref()
            .and_then(PlaidLocation::project),
        payment_channel: non_blank(transaction.payment_channel.as_deref()),
        logo_url: transaction
            .logo_url
            .clone()
            .or_else(|| counterparty.and_then(|value| value.logo_url.clone())),
        website: transaction
            .website
            .clone()
            .or_else(|| counterparty.and_then(|value| value.website.clone())),
        logo_name: transaction
            .merchant_name
            .as_deref()
            .or(counterparty_name)
            .map(str::to_owned),
        category_detail: category.and_then(|category| category.detailed.clone()),
        category_confidence: category.and_then(|category| category.confidence_level.clone()),
        counterparty_type: payment_app
            .and_then(|value| value.kind.clone())
            .or_else(|| counterparty.and_then(|value| value.kind.clone())),
        counterparties: transaction
            .counterparties
            .iter()
            .flatten()
            .filter_map(|value| {
                Some(Counterparty {
                    name: non_blank(value.name.as_deref())?,
                    r#type: value.kind.clone(),
                    confidence: value.confidence_level.clone(),
                    website: value.website.clone(),
                })
            })
            .collect(),
        merchant_entity_id: non_blank(transaction.merchant_entity_id.as_deref()),
        // Shown only when it says more than the cleaned name.
        original_description: non_blank(transaction.original_description.as_deref()).filter(
            |original| Some(original.as_str()) != transaction.name.as_deref().map(str::trim),
        ),
        check_number: non_blank(transaction.check_number.as_deref()),
        account_owner: non_blank(transaction.account_owner.as_deref()),
        transaction_code: transaction.transaction_code.clone(),
        pending_transaction_id: transaction.pending_transaction_id.clone(),
        ..Transaction::default()
    })
}

//! Account descriptors: the provider's own names for an account and its tax treatment.
use super::*;

/// How an investment account's income and gains are taxed, from its provider subtype or name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TaxClass {
    pub(super) treatment: &'static str,
    /// Grouped with retirement accounts in the interface.
    pub(super) retirement: bool,
}

impl TaxClass {
    /// Matches whole words, so a name like "Mirae" never reads as an IRA. Tax-free wins over
    /// tax-deferred: a Roth 401(k) is a Roth.
    pub(super) fn of(descriptor: &str) -> Self {
        let lower = descriptor.to_ascii_lowercase();
        let words = lower
            .split(|character: char| !character.is_ascii_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect::<Vec<_>>();
        let word = |candidates: &[&str]| words.iter().any(|word| candidates.contains(word));
        let prefix = |prefixes: &[&str]| {
            words
                .iter()
                .any(|word| prefixes.iter().any(|prefix| word.starts_with(prefix)))
        };
        let phrase = |phrases: &[&str]| phrases.iter().any(|phrase| lower.contains(phrase));
        if word(&["roth", "hsa", "tfsa"]) || phrase(&["health savings"]) {
            return Self {
                treatment: "taxFree",
                retirement: true,
            };
        }
        if word(&["529", "coverdell", "isa"])
            || phrase(&["education savings", "health reimbursement"])
        {
            return Self {
                treatment: "taxFree",
                retirement: false,
            };
        }
        if prefix(&["401", "403", "457"])
            || word(&[
                "ira",
                "sarsep",
                "pension",
                "retirement",
                "thrift",
                "keogh",
                "annuity",
                "rrsp",
                "rrif",
                "lira",
                "lif",
                "lrif",
                "lrsp",
                "prif",
                "rlif",
                "rdsp",
                "resp",
                "sipp",
                "tsp",
                "qshr",
            ])
            || phrase(&["profit sharing", "non-taxable brokerage"])
        {
            return Self {
                treatment: "taxDeferred",
                retirement: true,
            };
        }
        Self {
            treatment: "taxable",
            retirement: false,
        }
    }
}

/// Adds the institution's own descriptors that the projection doesn't otherwise use.
pub(super) fn describe_account(projected: &mut Account, account: &PlaidAccount) {
    projected.official_name = non_blank(account.official_name.as_deref());
    projected.subtype = non_blank(account.subtype.as_deref());
    projected.holder_category = non_blank(account.holder_category.as_deref());
    projected.domain = domain(account.institution_url.as_deref());
    projected.mask = account_mask(account.mask.as_deref());
}

/// The last four characters of an account number, as statements and payslips print them.
pub(super) fn account_mask(number: Option<&str>) -> Option<String> {
    let number = number?.trim();
    let tail = number
        .chars()
        .rev()
        .take_while(char::is_ascii_alphanumeric)
        .take(4)
        .collect::<Vec<_>>();
    (tail.len() >= 2).then(|| tail.into_iter().rev().collect())
}

/// An institution's website reduced to its registered domain (`https://us.etrade.com/home`
/// becomes `etrade.com`), so its logo is looked up exactly rather than by name.
pub(super) fn domain(website: Option<&str>) -> Option<String> {
    let website = website?.trim().to_ascii_lowercase();
    let rest = website
        .split_once("://")
        .map_or(website.as_str(), |(_, rest)| rest);
    let host = rest.split(['/', '?', '#', ':']).next()?;
    let labels = host
        .split('.')
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    let valid = |label: &&str| {
        label
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    };
    if labels.len() < 2 || !labels.iter().all(valid) {
        return None;
    }
    Some(labels[labels.len() - 2..].join("."))
}

/// Adds a Plaid security's classification and identifiers to a projected holding.
pub(super) fn describe_security(projected: &mut Holding, security: &PlaidSecurity) {
    projected.instrument_subtype = non_blank(security.subtype.as_deref());
    projected.sector = non_blank(security.sector.as_deref());
    projected.industry = non_blank(security.industry.as_deref());
    let identifiers = SecurityIdentifiers {
        isin: non_blank(security.isin.as_deref()),
        cusip: non_blank(security.cusip.as_deref()),
        figi: non_blank(security.figi.as_deref()),
        exchange: non_blank(security.market_identifier_code.as_deref()),
    };
    projected.identifiers = (identifiers != SecurityIdentifiers::default()).then_some(identifiers);
    projected.option = security
        .option_contract
        .as_ref()
        .map(|option| OptionContract {
            contract_type: non_blank(option.contract_type.as_deref()),
            expiration_date: iso_date(option.expiration_date.as_deref()),
            strike_price: option.strike_price.map(|price| scaled(price, 4)),
            underlying_ticker: non_blank(option.underlying_security_ticker.as_deref()),
        });
    projected.fixed_income = security.fixed_income.as_ref().map(|bond| FixedIncome {
        yield_percent: bond
            .yield_rate
            .as_ref()
            .and_then(|rate| rate.percentage)
            .map(|percent| scaled(percent, 4)),
        yield_type: bond
            .yield_rate
            .as_ref()
            .and_then(|rate| non_blank(rate.kind.as_deref())),
        maturity_date: iso_date(bond.maturity_date.as_deref()),
        issue_date: iso_date(bond.issue_date.as_deref()),
        face_value: bond.face_value.map(money),
    });
}

/// Plaid's recurring streams in the contract's shape; a stream missing its identity is left out.
pub(super) fn recurring_streams(streams: &[Value]) -> Vec<RecurringStream> {
    let mut projected = streams
        .iter()
        .filter_map(|stream| {
            let text = |field: &str| stream.get(field).and_then(Value::as_str).map(str::to_owned);
            Some(RecurringStream {
                stream_id: text("stream_id")?,
                account_id: format!("plaid:{}", text("account_id")?),
                direction: text("direction")?,
                description: text("description").unwrap_or_default(),
                merchant_name: text("merchant_name"),
                frequency: text("frequency").unwrap_or_else(|| "UNKNOWN".into()),
                status: text("status").unwrap_or_else(|| "UNKNOWN".into()),
                is_active: stream
                    .get("is_active")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                last_date: iso_date(stream.get("last_date").and_then(Value::as_str)),
                predicted_next_date: iso_date(
                    stream.get("predicted_next_date").and_then(Value::as_str),
                ),
                average_amount: stream
                    .pointer("/average_amount/amount")
                    .and_then(Value::as_f64),
                transaction_ids: stream
                    .get("transaction_ids")
                    .and_then(|ids| serde_json::from_value(ids.clone()).ok())
                    .unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    projected.sort_by(|left, right| left.stream_id.cmp(&right.stream_id));
    projected
}

/// Plaid stock-plan accounts that look like SnapTrade accounts, unless the user linked them.
pub(super) fn possible_duplicate_accounts(
    parsed: &Parsed,
    account_links: &BTreeMap<String, String>,
    warnings: &mut Vec<String>,
) -> Vec<PossibleDuplicate> {
    let mut possible_duplicates = Vec::new();
    for plaid in &parsed.plaid_investment_accounts {
        for snap in &parsed.snap_accounts {
            if possible_duplicate(
                plaid,
                snap,
                &parsed.plaid_holdings,
                &parsed.plaid_securities,
                parsed
                    .snap_positions
                    .get(&snap.id)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            ) {
                let plaid_id = format!("plaid:{}", plaid.account_id);
                let snap_id = format!("snaptrade:{}", snap.id);
                if account_links.get(&plaid_id) != Some(&snap_id) {
                    possible_duplicates.push(PossibleDuplicate {
                        description: format!(
                            "{} and {} may represent the same account",
                            text(plaid.name.as_deref(), "Plaid stock plan"),
                            text(snap.name.as_deref(), "SnapTrade account")
                        ),
                        plaid_account_id: plaid_id,
                        snaptrade_account_id: snap_id,
                    });
                }
            }
        }
    }
    possible_duplicates.sort_by(|left, right| left.plaid_account_id.cmp(&right.plaid_account_id));
    if !possible_duplicates.is_empty() {
        warnings.push("Possible duplicate accounts; confirm in Settings".into());
    }
    possible_duplicates
}

/// The snapshot's accounts, with the net worth they sum to.
pub(super) struct AccountRows {
    pub rows: Vec<Account>,
    pub net_worth: Decimal,
    /// Some account has no USD value, so totals leave it out.
    pub incomplete: bool,
}

/// Every account as the snapshot shows it: Plaid cash and card accounts, Plaid investment
/// accounts not linked to a SnapTrade account, and SnapTrade accounts, sorted by id behind
/// the combined "All accounts" row.
pub(super) fn project_accounts(
    sync: &ProviderSync,
    parsed: &Parsed,
    included: &[&PlaidAccount],
    committed_at: &str,
) -> AccountRows {
    let mut accounts: Vec<Account> = parsed
        .plaid_accounts
        .iter()
        .map(|account| plaid_account(account, parsed))
        .chain(
            included
                .iter()
                .map(|account| plaid_investment_account(account)),
        )
        .chain(
            parsed
                .snap_accounts
                .iter()
                .map(|account| snaptrade_account(account, sync, parsed)),
        )
        .collect();
    accounts.sort_by(|left, right| left.id.cmp(&right.id));
    let incomplete = accounts.iter().any(|account| account.value.is_none());
    let net_worth = accounts
        .iter()
        .filter_map(|account| account.value.and_then(decimal_f64))
        .sum::<Decimal>();
    accounts.insert(
        0,
        Account {
            id: "all".into(),
            name: "All accounts".into(),
            institution: "Brief".into(),
            r#type: "combined".into(),
            value: Some(money(net_worth)),
            balance_fetched_at: Some(committed_at.into()),
            ..Account::default()
        },
    );
    AccountRows {
        rows: accounts,
        net_worth,
        incomplete,
    }
}

/// A Plaid cash or card account; a card's balance is owed, so it counts against net worth.
fn plaid_account(account: &PlaidAccount, parsed: &Parsed) -> Account {
    let is_credit = account.kind == "credit";
    let currency = account.balances.iso_currency_code.as_deref();
    let value = usd_value(account.balances.current, currency).map(|value| {
        if is_credit && !value.is_zero() {
            -value
        } else {
            value
        }
    });
    let mut projected = Account {
        id: format!("plaid:{}", account.account_id),
        name: text(account.name.as_deref(), "Account"),
        institution: text(account.institution_name.as_deref(), "Unknown institution"),
        r#type: if is_credit { "credit" } else { "cash" }.into(),
        value: value.map(money),
        currency: account.balances.iso_currency_code.clone(),
        balance_as_of: account.balances.last_updated_datetime.clone(),
        balance_fetched_at: account.balance_fetched_at.clone(),
        balance_mode: account.balance_mode.clone(),
        available_balance: usd_value(account.balances.available, currency).map(money),
        credit_limit: usd_value(account.balances.limit, currency)
            .filter(|limit| is_credit && limit.is_sign_positive() && !limit.is_zero())
            .map(money),
        liability: parsed
            .liabilities
            .get(&account.account_id)
            .filter(|_| is_credit)
            .map(PlaidCreditLiability::project),
        ..Account::default()
    };
    describe_account(&mut projected, account);
    projected
}

/// A Plaid investment account not linked to a SnapTrade account.
fn plaid_investment_account(account: &PlaidAccount) -> Account {
    let tax = TaxClass::of(account.subtype.as_deref().unwrap_or_default());
    let mut projected = Account {
        id: format!("plaid:{}", account.account_id),
        name: text(account.name.as_deref(), "Investment account"),
        institution: text(account.institution_name.as_deref(), "Unknown institution"),
        r#type: if tax.retirement {
            "retirement"
        } else {
            "brokerage"
        }
        .into(),
        value: usd_value(
            account.balances.current,
            account.balances.iso_currency_code.as_deref(),
        )
        .map(money),
        currency: account.balances.iso_currency_code.clone(),
        balance_as_of: account.balances.last_updated_datetime.clone(),
        balance_fetched_at: account.balance_fetched_at.clone(),
        tax_treatment: Some(tax.treatment.into()),
        ..Account::default()
    };
    describe_account(&mut projected, account);
    projected
}

/// A SnapTrade account, valued from its cash and positions rather than its reported total.
fn snaptrade_account(account: &SnapAccount, sync: &ProviderSync, parsed: &Parsed) -> Account {
    let total = account.balance.total.as_ref();
    let reported = usd_value(
        total.and_then(|total| total.amount),
        total.and_then(|total| total.currency.as_deref()),
    );
    let calculated = parsed.calculated_snap_values.get(&account.id).copied();
    let descriptor = format!(
        "{} {}",
        account.name.as_deref().unwrap_or_default(),
        account.raw_type.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();
    let tax = TaxClass::of(&descriptor);
    let last_sync =
        |point: Option<&SnapSyncPoint>| point.and_then(|point| point.last_successful_sync.clone());
    let status = account.sync_status.as_ref();
    Account {
        id: format!("snaptrade:{}", account.id),
        name: text(
            account.name.as_deref().or(account.raw_type.as_deref()),
            "Investment account",
        ),
        institution: text(account.institution_name.as_deref(), "Unknown institution"),
        r#type: if tax.retirement {
            "retirement"
        } else {
            "brokerage"
        }
        .into(),
        tax_treatment: Some(tax.treatment.into()),
        subtype: non_blank(account.raw_type.as_deref()),
        mask: account_mask(account.printed_number()),
        domain: domain(account.institution_url.as_deref()),
        value: calculated.map(|value| money(value.total())),
        cash_value: calculated.map(|value| money(value.cash)),
        invested_value: calculated.map(|value| money(value.invested)),
        balance_source: Some(
            if calculated.is_some() {
                "cash-and-positions"
            } else {
                "unavailable"
            }
            .into(),
        ),
        reported_balance: reported.map(money),
        currency: if calculated.is_some() {
            Some("USD".into())
        } else {
            total.and_then(|total| total.currency.clone())
        },
        balance_as_of: last_sync(status.and_then(|status| status.holdings.as_ref())),
        balance_fetched_at: account.balance_fetched_at.clone(),
        positions_as_of: sync
            .snaptrade
            .positions_as_of
            .get(&account.id)
            .cloned()
            .flatten(),
        activity_as_of: last_sync(status.and_then(|status| status.transactions.as_ref())),
        ..Account::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduces_websites_to_their_registered_domain() {
        for (website, expected) in [
            ("https://us.etrade.com/home", Some("etrade.com")),
            ("https://www.chase.com", Some("chase.com")),
            ("americanexpress.com", Some("americanexpress.com")),
            ("HTTPS://WWW.SCHWAB.COM:443/", Some("schwab.com")),
            ("localhost", None),
            ("https://bad host.com", None),
        ] {
            assert_eq!(domain(Some(website)).as_deref(), expected, "{website}");
        }
        assert!(domain(None).is_none());
    }

    #[test]
    fn classifies_tax_treatment_from_subtypes_and_names() {
        for (descriptor, treatment, retirement) in [
            ("401k", "taxDeferred", true),
            ("403B", "taxDeferred", true),
            ("sep ira", "taxDeferred", true),
            ("Traditional IRA", "taxDeferred", true),
            ("roth 401k", "taxFree", true),
            ("Roth IRA", "taxFree", true),
            ("hsa", "taxFree", true),
            ("529", "taxFree", false),
            ("education savings account", "taxFree", false),
            ("non-taxable brokerage account", "taxDeferred", true),
            ("brokerage", "taxable", false),
            ("stock plan", "taxable", false),
            ("ugma", "taxable", false),
            ("Mirae Individual margin", "taxable", false),
            ("", "taxable", false),
        ] {
            assert_eq!(
                TaxClass::of(descriptor),
                TaxClass {
                    treatment,
                    retirement
                },
                "{descriptor}"
            );
        }
    }
}

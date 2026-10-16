//! Typed Plaid and SnapTrade records decoded from provider compatibility JSON.
use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct PlaidBalances {
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) current: Option<Decimal>,
    /// Spendable funds (deposits) or remaining credit (cards), after pending activity.
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) available: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) limit: Option<Decimal>,
    #[serde(default)]
    pub(super) iso_currency_code: Option<String>,
    #[serde(default)]
    pub(super) last_updated_datetime: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidAccount {
    pub(super) account_id: String,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) institution_name: Option<String>,
    #[serde(rename = "type")]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) subtype: Option<String>,
    #[serde(default)]
    pub(super) mask: Option<String>,
    #[serde(default)]
    pub(super) official_name: Option<String>,
    #[serde(default)]
    pub(super) holder_category: Option<String>,
    /// The institution's website, from Plaid.
    #[serde(default)]
    pub(super) institution_url: Option<String>,
    #[serde(default)]
    pub(super) balance_fetched_at: Option<String>,
    #[serde(default)]
    pub(super) balance_mode: Option<String>,
    pub(super) balances: PlaidBalances,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidApr {
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) apr_percentage: Option<Decimal>,
    #[serde(default)]
    pub(super) apr_type: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) balance_subject_to_apr: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) interest_charge_amount: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidCreditLiability {
    pub(super) account_id: String,
    #[serde(default)]
    pub(super) aprs: Vec<PlaidApr>,
    #[serde(default)]
    pub(super) is_overdue: Option<bool>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) last_payment_amount: Option<Decimal>,
    #[serde(default)]
    pub(super) last_payment_date: Option<String>,
    #[serde(default)]
    pub(super) last_statement_issue_date: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) last_statement_balance: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) minimum_payment_amount: Option<Decimal>,
    #[serde(default)]
    pub(super) next_payment_due_date: Option<String>,
}

impl PlaidCreditLiability {
    pub(super) fn project(&self) -> CreditLiability {
        CreditLiability {
            last_statement_date: iso_date(self.last_statement_issue_date.as_deref()),
            last_statement_balance: self.last_statement_balance.map(money),
            minimum_payment: self.minimum_payment_amount.map(money),
            next_payment_due_date: iso_date(self.next_payment_due_date.as_deref()),
            last_payment_date: iso_date(self.last_payment_date.as_deref()),
            last_payment_amount: self.last_payment_amount.map(money),
            purchase_apr: self
                .aprs
                .iter()
                .find(|apr| apr.apr_type.as_deref() == Some("purchase_apr"))
                .and_then(|apr| apr.apr_percentage)
                .map(money),
            is_overdue: self.is_overdue,
            aprs: self
                .aprs
                .iter()
                .filter_map(|apr| {
                    Some(CreditApr {
                        r#type: apr.apr_type.clone()?,
                        percentage: apr.apr_percentage.map(money),
                        balance_subject_to_apr: apr.balance_subject_to_apr.map(money),
                        interest_charged: apr.interest_charge_amount.map(money),
                    })
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidCategory {
    pub(super) primary: String,
    #[serde(default)]
    pub(super) detailed: Option<String>,
    #[serde(default)]
    pub(super) confidence_level: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidCounterparty {
    #[serde(default, rename = "type")]
    pub(super) kind: Option<String>,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) logo_url: Option<String>,
    #[serde(default)]
    pub(super) website: Option<String>,
    #[serde(default)]
    pub(super) confidence_level: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidTransaction {
    pub(super) transaction_id: String,
    pub(super) account_id: String,
    #[serde(deserialize_with = "decimal")]
    pub(super) amount: Decimal,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) merchant_name: Option<String>,
    #[serde(default)]
    pub(super) logo_url: Option<String>,
    #[serde(default)]
    pub(super) website: Option<String>,
    pub(super) pending: bool,
    #[serde(default)]
    pub(super) date: Option<String>,
    #[serde(default)]
    pub(super) datetime: Option<String>,
    #[serde(default)]
    pub(super) authorized_date: Option<String>,
    #[serde(default)]
    pub(super) authorized_datetime: Option<String>,
    #[serde(default)]
    pub(super) location: Option<PlaidLocation>,
    #[serde(default)]
    pub(super) payment_channel: Option<String>,
    #[serde(default)]
    pub(super) category: Option<Vec<String>>,
    #[serde(default)]
    pub(super) personal_finance_category: Option<PlaidCategory>,
    #[serde(default)]
    pub(super) counterparties: Option<Vec<PlaidCounterparty>>,
    #[serde(default)]
    pub(super) transaction_code: Option<String>,
    #[serde(default)]
    pub(super) pending_transaction_id: Option<String>,
    /// Plaid's stable merchant identity across store locations and descriptor variants.
    #[serde(default)]
    pub(super) merchant_entity_id: Option<String>,
    /// The institution's raw descriptor, present once requested from `/transactions/sync`.
    #[serde(default)]
    pub(super) original_description: Option<String>,
    #[serde(default)]
    pub(super) check_number: Option<String>,
    /// The cardholder on an account with several, when the institution reports it.
    #[serde(default)]
    pub(super) account_owner: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidLocation {
    pub(super) address: Option<String>,
    pub(super) city: Option<String>,
    pub(super) region: Option<String>,
    pub(super) postal_code: Option<String>,
    pub(super) country: Option<String>,
    #[serde(default)]
    pub(super) lat: Option<f64>,
    #[serde(default)]
    pub(super) lon: Option<f64>,
    #[serde(default)]
    pub(super) store_number: Option<String>,
}

impl PlaidLocation {
    pub(super) fn project(&self) -> Option<TransactionLocation> {
        let location = TransactionLocation {
            address: non_blank(self.address.as_deref()),
            city: non_blank(self.city.as_deref()),
            region: non_blank(self.region.as_deref()),
            postal_code: non_blank(self.postal_code.as_deref()),
            country: non_blank(self.country.as_deref()),
            // Coordinates only as a pair, and only when they are on Earth.
            lat: self
                .lat
                .zip(self.lon)
                .filter(|(lat, lon)| lat.abs() <= 90.0 && lon.abs() <= 180.0)
                .map(|(lat, _)| lat),
            lon: self
                .lat
                .zip(self.lon)
                .filter(|(lat, lon)| lat.abs() <= 90.0 && lon.abs() <= 180.0)
                .map(|(_, lon)| lon),
            store_number: non_blank(self.store_number.as_deref()),
        };
        (location.address.is_some()
            || location.city.is_some()
            || location.region.is_some()
            || location.postal_code.is_some()
            || location.country.is_some()
            || location.lat.is_some()
            || location.store_number.is_some())
        .then_some(location)
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct TaxLot {
    #[serde(default)]
    pub(super) original_purchase_datetime: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) quantity: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidHolding {
    pub(super) account_id: String,
    pub(super) security_id: String,
    #[serde(deserialize_with = "decimal")]
    pub(super) quantity: Decimal,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) institution_price: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) institution_value: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) value: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) cost_basis: Option<Decimal>,
    #[serde(default)]
    pub(super) iso_currency_code: Option<String>,
    #[serde(default)]
    pub(super) institution_price_as_of: Option<String>,
    #[serde(default)]
    pub(super) institution_price_datetime: Option<String>,
    #[serde(default)]
    pub(super) tax_lots: Vec<TaxLot>,
    /// Shares already vested, for equity compensation; absent for ordinary positions.
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) vested_quantity: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) vested_value: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidSecurity {
    pub(super) security_id: String,
    #[serde(default)]
    pub(super) ticker_symbol: Option<String>,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) close_price: Option<Decimal>,
    #[serde(default, rename = "type")]
    pub(super) kind: Option<String>,
    #[serde(default)]
    pub(super) is_cash_equivalent: Option<bool>,
    #[serde(default)]
    pub(super) iso_currency_code: Option<String>,
    #[serde(default)]
    pub(super) subtype: Option<String>,
    #[serde(default)]
    pub(super) isin: Option<String>,
    #[serde(default)]
    pub(super) cusip: Option<String>,
    #[serde(default)]
    pub(super) figi: Option<String>,
    #[serde(default)]
    pub(super) market_identifier_code: Option<String>,
    #[serde(default)]
    pub(super) sector: Option<String>,
    #[serde(default)]
    pub(super) industry: Option<String>,
    #[serde(default)]
    pub(super) option_contract: Option<PlaidOptionContract>,
    #[serde(default)]
    pub(super) fixed_income: Option<PlaidFixedIncome>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidOptionContract {
    #[serde(default)]
    pub(super) contract_type: Option<String>,
    #[serde(default)]
    pub(super) expiration_date: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) strike_price: Option<Decimal>,
    #[serde(default)]
    pub(super) underlying_security_ticker: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidYieldRate {
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) percentage: Option<Decimal>,
    #[serde(default, rename = "type")]
    pub(super) kind: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidFixedIncome {
    #[serde(default)]
    pub(super) yield_rate: Option<PlaidYieldRate>,
    #[serde(default)]
    pub(super) maturity_date: Option<String>,
    #[serde(default)]
    pub(super) issue_date: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) face_value: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
pub(super) struct PlaidInvestmentTransaction {
    pub(super) investment_transaction_id: String,
    pub(super) account_id: String,
    #[serde(default)]
    pub(super) security_id: Option<String>,
    pub(super) date: String,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) quantity: Option<Decimal>,
    /// Positive when cash leaves the account, as for a purchase.
    #[serde(deserialize_with = "decimal")]
    pub(super) amount: Decimal,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) price: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) fees: Option<Decimal>,
    #[serde(rename = "type")]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) subtype: Option<String>,
    #[serde(default)]
    pub(super) iso_currency_code: Option<String>,
    #[serde(default)]
    pub(super) cancel_transaction_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SnapAmount {
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) amount: Option<Decimal>,
    #[serde(default)]
    pub(super) currency: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SnapBalance {
    #[serde(default)]
    pub(super) total: Option<SnapAmount>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SnapSyncPoint {
    #[serde(default)]
    pub(super) last_successful_sync: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SnapSyncStatus {
    #[serde(default)]
    pub(super) holdings: Option<SnapSyncPoint>,
    #[serde(default)]
    pub(super) transactions: Option<SnapSyncPoint>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SnapAccount {
    pub(super) id: String,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) raw_type: Option<String>,
    #[serde(default)]
    pub(super) institution_name: Option<String>,
    #[serde(default)]
    pub(super) number: Option<String>,
    #[serde(default)]
    pub(super) account_number: Option<String>,
    /// The number the brokerage itself uses. `number` can be the brokerage's API key instead
    /// (E*Trade's `accountIdKey`), so this one names the account's last four.
    #[serde(default)]
    pub(super) institution_account_id: Option<String>,
    #[serde(default)]
    pub(super) balance: SnapBalance,
    #[serde(default)]
    pub(super) sync_status: Option<SnapSyncStatus>,
    #[serde(default)]
    pub(super) balance_fetched_at: Option<String>,
    /// The brokerage's website, from SnapTrade.
    #[serde(default)]
    pub(super) institution_url: Option<String>,
}

impl SnapAccount {
    /// The account number as the brokerage prints it, as far as SnapTrade says.
    pub(super) fn printed_number(&self) -> Option<&str> {
        self.institution_account_id
            .as_deref()
            .filter(|number| number.chars().filter(char::is_ascii_digit).count() >= 4)
            .or(self.number.as_deref())
            .or(self.account_number.as_deref())
    }
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct InstrumentLabel {
    #[serde(default)]
    pub(super) raw_symbol: Option<String>,
    #[serde(default)]
    pub(super) symbol: Option<String>,
    #[serde(default)]
    pub(super) description: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SnapInstrument {
    #[serde(default)]
    pub(super) kind: Option<String>,
    #[serde(flatten)]
    pub(super) label: InstrumentLabel,
    #[serde(default)]
    pub(super) underlying: Option<InstrumentLabel>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SnapPosition {
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) units: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) price: Option<Decimal>,
    #[serde(default)]
    pub(super) currency: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) cost_basis: Option<Decimal>,
    #[serde(default)]
    pub(super) cash_equivalent: bool,
    pub(super) instrument: SnapInstrument,
}

#[derive(Debug, Deserialize)]
pub(super) struct SnapCashBalance {
    pub(super) currency: SnapCashCurrency,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) cash: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SnapCashCurrency {
    pub(super) code: String,
}

#[derive(Clone, Copy)]
pub(super) struct SnapValuation {
    pub(super) cash: Decimal,
    pub(super) invested: Decimal,
}

impl SnapValuation {
    pub(super) fn total(self) -> Decimal {
        rounded(self.cash + self.invested)
    }
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ActivitySymbol {
    #[serde(default)]
    pub(super) raw_symbol: Option<String>,
    #[serde(default)]
    pub(super) symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SnapActivity {
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(rename = "type")]
    pub(super) kind: String,
    #[serde(deserialize_with = "decimal")]
    pub(super) amount: Decimal,
    #[serde(default)]
    pub(super) trade_date: Option<String>,
    #[serde(default)]
    pub(super) settlement_date: Option<String>,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) units: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) price: Option<Decimal>,
    #[serde(default)]
    pub(super) symbol: Option<ActivitySymbol>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) fee: Option<Decimal>,
    #[serde(default)]
    pub(super) external_reference_id: Option<String>,
}

impl SnapActivity {
    /// The day the activity happened: its trade date, or else its settlement date.
    pub(super) fn date(&self) -> Option<String> {
        iso_date(
            self.trade_date
                .as_deref()
                .or(self.settlement_date.as_deref()),
        )
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct HistoryPoint {
    pub(super) date: String,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) total_value: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub(super) value: Option<Decimal>,
}

#[derive(Clone)]
pub(super) struct Acquisition {
    pub(super) date: String,
    pub(super) quantity: Decimal,
}

pub(super) fn decimal<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Decimal, D::Error> {
    let value = Value::deserialize(deserializer)?;
    decimal_value(&value).ok_or_else(|| D::Error::custom("expected a finite decimal"))
}

pub(super) fn optional_decimal<'de, D: Deserializer<'de>>(
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

pub(super) fn parse_records<T: for<'de> Deserialize<'de>>(
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
                    warnings.push(format!("Skipped malformed {label} record {}", index + 1));
                    None
                }
            },
        )
        .collect()
}

pub(super) fn iso_date(value: Option<&str>) -> Option<String> {
    let date = value?.get(..10)?;
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .map(|_| date.to_string())
}

pub(super) fn plaid_posted_date(transaction: &PlaidTransaction) -> Option<String> {
    iso_date(
        transaction
            .datetime
            .as_deref()
            .or(transaction.date.as_deref()),
    )
}

pub(super) fn text(value: Option<&str>, fallback: &str) -> String {
    non_blank(value).unwrap_or_else(|| fallback.to_string())
}

/// A provider's text trimmed, or `None` when it is missing or blank.
pub(super) fn non_blank(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(super) fn normalized(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Every provider record parsed once; a malformed record is skipped with a warning.
pub(super) struct Parsed {
    pub plaid_accounts: Vec<PlaidAccount>,
    pub plaid_investment_accounts: Vec<PlaidAccount>,
    pub plaid_transactions: Vec<PlaidTransaction>,
    pub plaid_holdings: Vec<PlaidHolding>,
    pub plaid_securities: BTreeMap<String, PlaidSecurity>,
    pub liabilities: BTreeMap<String, PlaidCreditLiability>,
    pub snap_accounts: Vec<SnapAccount>,
    pub snap_positions: BTreeMap<String, Vec<SnapPosition>>,
    /// Each SnapTrade account's value from its cash and positions, when every position parsed.
    pub calculated_snap_values: BTreeMap<String, SnapValuation>,
    pub snap_activities: BTreeMap<String, Vec<SnapActivity>>,
}

impl Parsed {
    pub(super) fn from_sync(sync: &ProviderSync, warnings: &mut Vec<String>) -> Self {
        let plaid_accounts =
            parse_records::<PlaidAccount>(&sync.plaid.accounts, "Plaid account", warnings);
        let plaid_investment_accounts = parse_records::<PlaidAccount>(
            &sync.plaid.investment_accounts,
            "Plaid investment account",
            warnings,
        );
        let plaid_transactions = parse_records::<PlaidTransaction>(
            &sync.plaid.transactions,
            "Plaid transaction",
            warnings,
        );
        let plaid_holdings =
            parse_records::<PlaidHolding>(&sync.plaid.holdings, "Plaid holding", warnings);
        let plaid_securities =
            parse_records::<PlaidSecurity>(&sync.plaid.securities, "Plaid security", warnings)
                .into_iter()
                .map(|security| (security.security_id.clone(), security))
                .collect::<BTreeMap<_, _>>();
        let liabilities = parse_records::<PlaidCreditLiability>(
            &sync.plaid.liabilities,
            "Plaid card statement",
            warnings,
        )
        .into_iter()
        .map(|liability| (liability.account_id.clone(), liability))
        .collect::<BTreeMap<_, _>>();
        let snap_accounts =
            parse_records::<SnapAccount>(&sync.snaptrade.accounts, "SnapTrade account", warnings);
        let snap_positions = sync
            .snaptrade
            .positions
            .iter()
            .map(|(id, records)| {
                (
                    id.clone(),
                    parse_records::<SnapPosition>(records, "SnapTrade position", warnings),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let calculated_snap_values = sync
            .snaptrade
            .cash_balances
            .iter()
            .filter_map(|(id, balances)| {
                let positions = snap_positions.get(id)?;
                if sync.snaptrade.positions.get(id)?.len() != positions.len() {
                    return None;
                }
                calculated_snap_valuation(positions, balances).map(|value| (id.clone(), value))
            })
            .collect::<BTreeMap<_, _>>();
        let snap_activities = sync
            .snaptrade
            .activities
            .iter()
            .map(|(id, records)| {
                (
                    id.clone(),
                    parse_records::<SnapActivity>(records, "SnapTrade activity", warnings),
                )
            })
            .collect::<BTreeMap<_, _>>();
        Self {
            plaid_accounts,
            plaid_investment_accounts,
            plaid_transactions,
            plaid_holdings,
            plaid_securities,
            liabilities,
            snap_accounts,
            snap_positions,
            calculated_snap_values,
            snap_activities,
        }
    }

    /// Plaid investment accounts the snapshot shows: those not linked to a SnapTrade account
    /// that holds the same positions.
    pub(super) fn included_plaid_investments(
        &self,
        account_links: &BTreeMap<String, String>,
    ) -> Vec<&PlaidAccount> {
        self.plaid_investment_accounts
            .iter()
            .filter(|account| !account_links.contains_key(&format!("plaid:{}", account.account_id)))
            .collect()
    }
}

//! Durable choices that change financial results. They live in the finance database so
//! they are backed up, restored, and erased with the data they describe.
use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::{de::IgnoredAny, Deserialize, Serialize};
use ts_rs::TS;

const MAX_ACCOUNTS: usize = 500;
const MAX_ID_LENGTH: usize = 220;
const MAX_NAME_LENGTH: usize = 80;
const MAX_HIDDEN_BENEFITS: usize = 100;
const MAX_CARD_PRODUCTS: usize = 20;
const MAX_CARD_BENEFITS: usize = 60;
/// A card's rules are about 10 KB of text; this leaves room for a generous one, not a dump.
const MAX_CARD_RULES_BYTES: usize = 64 * 1024;
const MAX_MANUAL_ACCOUNTS: usize = 50;
const MAX_MANUAL_ENTRIES: usize = 1_000;
const MAX_MANUAL_VALUE: f64 = 1_000_000_000_000.0;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, TS)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct FinanceSettings {
    /// The saved credit account used by Spending and card benefits. Never inferred.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub spending_account_id: Option<String>,
    /// More credit accounts used for spending. Like the saved spending card, Money presents
    /// them, so the accounts total leaves their balances out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spending_cards: Vec<String>,
    /// What the user records about an account that no provider reports, keyed by account ID.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub account_details: BTreeMap<String, AccountDetails>,
    /// Display names keyed by account ID. Provider names remain the fallback.
    pub account_names: BTreeMap<String, String>,
    /// First day each account's history should be charted, as `YYYY-MM-DD`.
    pub account_start_dates: BTreeMap<String, String>,
    /// The day an account was closed, keyed by account ID. A closed account leaves today's
    /// accounts and totals; its history and activity stay.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub account_closed_dates: BTreeMap<String, String>,
    /// Accounts no provider reports, valued by hand: a 401(k), a car, a mortgage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_accounts: Vec<ManualAccount>,
    pub hidden_benefit_ids: Vec<String>,
    /// Monthly targets were retired; settings saved with them still load and drop the key.
    #[serde(rename = "categoryTargets", skip_serializing)]
    #[ts(skip)]
    pub retired_category_targets: Option<IgnoredAny>,
    /// Request card statement details from Plaid Liabilities, a separately billed product.
    pub plaid_liabilities: bool,
    /// Keep running after the window closes and refresh saved data once a day.
    pub background_refresh: bool,
    /// Post a macOS notification when a background refresh finds something that needs action.
    pub notifications: bool,
    /// Keep the Brief calendar in macOS Calendar matched to the agenda.
    pub calendar_sync: bool,
    /// The Brief calendar's EventKit identifier, kept while sync is off so turning it back on
    /// reuses the calendar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub calendar_id: Option<String>,
    /// Card products tracked for benefits, keyed by credit account ID. The spending card is
    /// tracked without an entry. A product with no rules in `card_rules` tracks nothing.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub benefit_cards: BTreeMap<String, String>,
    /// The benefit rules of each card product Brief knows: applied from the open card catalog,
    /// or edited or imported by the user. Brief ships none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub card_rules: Vec<crate::insights::benefits::CardRules>,
    /// The open card benefits catalog is downloaded and applied unless this is set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub card_catalog_off: bool,
    /// Cards whose catalog match was too close to call, with the catalog cards to choose
    /// from; choosing one applies it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub card_choices: Vec<CardChoice>,
    /// How an account is taxed when its provider subtype says otherwise or nothing:
    /// `taxable`, `taxDeferred`, or `taxFree`, keyed by account ID.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub account_tax_treatments: BTreeMap<String, String>,
    /// What tax estimates stack on. Without it Brief reports amounts but no tax.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tax_profile: Option<TaxProfile>,
    /// What retirement figures need beyond the accounts: age, planned costs, and IRA decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub retirement: Option<RetirementProfile>,
    /// Request Plaid Investments Transactions, a separately billed subscription per connection.
    pub plaid_investment_transactions: bool,
    /// Request Plaid Recurring Transactions, a separately billed subscription per connection.
    pub plaid_recurring: bool,
    /// Read Apple Wallet's records on this Mac. Off until turned on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub apple_wallet: bool,
    /// The on-device summary wording choice was retired with the model's phrases; settings
    /// saved with it still load and drop the key.
    #[serde(rename = "templateWording", skip_serializing)]
    #[ts(skip)]
    pub retired_template_wording: Option<IgnoredAny>,
    /// The people transactions and trips can be tagged with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<Person>,
    /// Who went on each trip. Trip ids move as a trip grows, so each entry names the trip by
    /// its transactions when saved, and joins any trip that shares one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trip_companions: Vec<TripCompanions>,
    /// Where Brief writes a readable copy of what it knows, as plain CSV and Markdown; off when
    /// absent. Brief writes it after changes and never reads it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub data_folder: Option<DataFolder>,
    /// Life chapters and program rules were retired; settings saved with your word on them, your
    /// planned chapters, or your approved rules still load and drop the keys.
    #[serde(rename = "chapters", skip_serializing)]
    #[ts(skip)]
    pub retired_chapters: Option<IgnoredAny>,
    #[serde(rename = "plannedChapters", skip_serializing)]
    #[ts(skip)]
    pub retired_planned_chapters: Option<IgnoredAny>,
    #[serde(rename = "explanationReviews", skip_serializing)]
    #[ts(skip)]
    pub retired_explanation_reviews: Option<IgnoredAny>,
    #[serde(rename = "programs", skip_serializing)]
    #[ts(skip)]
    pub retired_programs: Option<IgnoredAny>,
    /// Your answers to suggested categories, by transaction id. A rejected category is never
    /// suggested for that transaction again and counts against it for the merchant; a kept one
    /// is saved as your category, so here it only feeds the detector's precision.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub category_reviews: BTreeMap<String, CategoryReview>,
    // shortcut: unused since Exa was removed; drop with the Phase 3 storage change.
    pub research: bool,
    /// Unused since Ollama was removed; it goes with Phase 3's storage changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub local_model: Option<String>,
    /// Merchant sites, Mac sources, link reviews, and person handles left with Artha; settings
    /// saved with them still load and drop the keys.
    #[serde(rename = "merchantSites", skip_serializing)]
    #[ts(skip)]
    pub retired_merchant_sites: Option<IgnoredAny>,
    #[serde(rename = "macSources", skip_serializing)]
    #[ts(skip)]
    pub retired_mac_sources: Option<IgnoredAny>,
    #[serde(rename = "linkReviews", skip_serializing)]
    #[ts(skip)]
    pub retired_link_reviews: Option<IgnoredAny>,
    #[serde(rename = "personHandles", skip_serializing)]
    #[ts(skip)]
    pub retired_person_handles: Option<IgnoredAny>,
}

/// Your answer to one suggested category.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CategoryReview {
    pub category: String,
    pub accepted: bool,
}

/// The readable copy's place: Brief writes `Brief Data` inside the chosen folder.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DataFolder {
    /// The chosen folder, an absolute path.
    pub path: String,
    /// Write every amount as `•••`.
    #[serde(default)]
    pub masked: bool,
}

/// A card whose catalog match was too close to call, and the catalog cards it could be.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CardChoice {
    pub account_id: String,
    pub candidates: Vec<CardChoiceOption>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CardChoiceOption {
    pub id: String,
    pub name: String,
    pub issuer: String,
}

/// Someone transactions and trips can be tagged with. Tagging never changes an amount or a
/// category.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Person {
    /// `person:` followed by an identifier the renderer chose.
    pub id: String,
    pub name: String,
    /// The Mac Contacts card this person was added from, read only for its photo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub contact_id: Option<String>,
    /// Names a bank prints for this person, such as `JANE DOE`. A payment to or from a person
    /// whose descriptor contains one is tagged with them; without any, never automatically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub match_names: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TripCompanions {
    /// The trip's transactions when the companions were saved.
    pub transaction_ids: Vec<String>,
    /// Person ids.
    pub people: Vec<String>,
}

const MAX_REVIEWS: usize = 2_000;
const MAX_PEOPLE: usize = 500;
const MAX_MATCH_NAMES: usize = 8;
const MAX_MATCH_NAME_LENGTH: usize = 60;
const MAX_TRIP_COMPANIONS: usize = 1_000;
const MAX_TRIP_TRANSACTIONS: usize = 2_000;
const MAX_COMPANIONS: usize = 24;

/// A match name as matching compares it: upper case, with each run of anything other than a
/// letter or digit as one space.
pub fn match_text(text: &str) -> String {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_uppercase)
        .collect::<Vec<_>>()
        .join(" ")
}

impl Person {
    fn normalized(mut self) -> Result<Self, String> {
        self.name = self.name.trim().chars().take(MAX_NAME_LENGTH).collect();
        self.contact_id = self
            .contact_id
            .map(|id| id.trim().to_owned())
            .filter(|id| !id.is_empty());
        if !self.id.starts_with("person:")
            || self.id.len() > 64
            || self.id.chars().any(char::is_control)
            || self.name.is_empty()
            || self.name.chars().any(char::is_control)
            || self
                .contact_id
                .as_ref()
                .is_some_and(|id| id.len() > 200 || id.chars().any(char::is_control))
            || self.match_names.len() > MAX_MATCH_NAMES
            || self
                .match_names
                .iter()
                .any(|name| name.chars().count() > MAX_MATCH_NAME_LENGTH)
        {
            return Err("Invalid person".into());
        }
        let mut match_names = Vec::<String>::new();
        for name in self.match_names {
            let name = match_text(&name);
            if !name.is_empty() && !match_names.contains(&name) {
                match_names.push(name);
            }
        }
        self.match_names = match_names;
        Ok(self)
    }
}

pub const FILING_STATUSES: [&str; 4] = [
    "single",
    "marriedJoint",
    "marriedSeparate",
    "headOfHousehold",
];
const MAX_INCOME: f64 = 100_000_000.0;
const MAX_STATE_RATE: f64 = 20.0;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaxProfile {
    pub filing_status: String,
    /// Taxable income for the year from everything Brief doesn't see (wages, business
    /// income), after deductions.
    pub taxable_income: f64,
    /// A flat state income tax rate in percent, when the user wants state tax included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub state_rate: Option<f64>,
}

pub const TAX_TREATMENTS: [&str; 3] = ["taxable", "taxDeferred", "taxFree"];

const MAX_CONTRIBUTION_CHOICES: usize = 2_000;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, TS)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct RetirementProfile {
    /// Only decides catch-up eligibility, which follows age at the end of the year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub birth_year: Option<i32>,
    /// Monthly costs the user expects to add to today's spending, such as rent once they
    /// move out, or health coverage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub planned_monthly: Option<f64>,
    /// Roth IRA contributions made before the account's history in Brief, by account ID.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub earlier_contributions: BTreeMap<String, f64>,
    /// IRA inflows the user decided, by transaction ID: the tax year a contribution counts
    /// toward, or `null` when it isn't one (a rollover, or a move between IRAs).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub contribution_years: BTreeMap<String, Option<i32>>,
}

/// What a manual account is, which decides its icon and whether its balance is owed.
pub const MANUAL_ACCOUNT_KINDS: [&str; 6] = [
    "cash",
    "brokerage",
    "retirement",
    "property",
    "vehicle",
    "loan",
];

/// Details the user keeps about an account: where to manage it and the rate it advertises.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AccountDetails {
    /// The institution's page for the account, `https://` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub website: Option<String>,
    /// The annual percentage yield the institution states, in percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub apy: Option<f64>,
}

impl AccountDetails {
    fn normalized(self) -> Result<Option<Self>, String> {
        let website = self
            .website
            .map(|website| website.trim().to_owned())
            .filter(|website| !website.is_empty());
        if website.as_deref().is_some_and(|website| {
            website.len() > 500
                || !website.starts_with("https://")
                || website
                    .chars()
                    .any(|character| character.is_control() || character == ' ')
        }) {
            return Err("Invalid account website setting".into());
        }
        if self
            .apy
            .is_some_and(|apy| !apy.is_finite() || !(0.0..=100.0).contains(&apy))
        {
            return Err("Invalid account APY setting".into());
        }
        let apy = self.apy.map(|apy| (apy * 1000.0).round() / 1000.0);
        Ok((website.is_some() || apy.is_some()).then_some(Self { website, apy }))
    }
}

/// An account valued by hand. Its balance on any day is the latest entry on or before it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ManualAccount {
    /// `manual:` followed by an identifier the renderer chose.
    pub id: String,
    pub name: String,
    /// One of `MANUAL_ACCOUNT_KINDS`.
    pub kind: String,
    /// Dated balances, oldest first. A loan's balance is what is owed, entered positive.
    pub entries: Vec<ManualEntry>,
    /// Carry the latest known value forward with the S&P 500 (dividends reinvested), for an
    /// index fund without a public price, such as a plan's collective trust.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tracks_sp500: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ManualEntry {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub value: f64,
}

impl ManualAccount {
    /// Whether the balance counts against net worth.
    pub fn owed(&self) -> bool {
        self.kind == "loan"
    }

    /// The signed value on `date`: the latest entry on or before it, negative when owed.
    pub fn value_on(&self, date: &str) -> Option<f64> {
        let entry = self
            .entries
            .iter()
            .rev()
            .find(|entry| entry.date.as_str() <= date)?;
        Some(if self.owed() {
            -entry.value
        } else {
            entry.value
        })
    }

    fn normalized(mut self) -> Result<Self, String> {
        self.name = self.name.trim().chars().take(MAX_NAME_LENGTH).collect();
        if !self.id.starts_with("manual:")
            || self.id.len() > 64
            || self.id.chars().any(char::is_control)
            || self.name.is_empty()
            || self.name.chars().any(char::is_control)
            || !MANUAL_ACCOUNT_KINDS.contains(&self.kind.as_str())
            || self.entries.len() > MAX_MANUAL_ENTRIES
            || self.entries.iter().any(|entry| {
                NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d").is_err()
                    || !entry.value.is_finite()
                    || entry.value.abs() > MAX_MANUAL_VALUE
            })
        {
            return Err("Invalid manual account".into());
        }
        // One balance per day, oldest first; the later entry for a day wins.
        let mut by_date = BTreeMap::new();
        for entry in self.entries {
            by_date.insert(entry.date, crate::numeric::round2(entry.value));
        }
        self.entries = by_date
            .into_iter()
            .map(|(date, value)| ManualEntry { date, value })
            .collect();
        Ok(self)
    }
}

impl FinanceSettings {
    /// Every account the user spends from: the saved spending card, then the others.
    pub fn spending_account_ids(&self) -> impl Iterator<Item = &str> {
        self.spending_account_id
            .iter()
            .chain(&self.spending_cards)
            .map(String::as_str)
    }

    /// Trims and validates every value; the result is the only form ever persisted.
    pub fn normalized(self) -> Result<Self, String> {
        let invalid = |field: &str| format!("Invalid {field} setting");
        let valid_id = |id: &str| {
            !id.is_empty() && id.len() <= MAX_ID_LENGTH && !id.chars().any(char::is_control)
        };
        let spending_account_id = match self.spending_account_id {
            Some(id) if id.trim().is_empty() => None,
            Some(id) if valid_id(id.trim()) => Some(id.trim().to_owned()),
            Some(_) => return Err(invalid("spending account")),
            None => None,
        };
        if self.account_names.len() > MAX_ACCOUNTS
            || self.account_start_dates.len() > MAX_ACCOUNTS
            || self.account_details.len() > MAX_ACCOUNTS
            || self.spending_cards.len() > MAX_ACCOUNTS
        {
            return Err(invalid("account"));
        }
        let mut spending_cards = Vec::new();
        for id in self.spending_cards {
            let id = id.trim().to_owned();
            if !valid_id(&id) {
                return Err(invalid("spending cards"));
            }
            if spending_account_id.as_ref() != Some(&id) && !spending_cards.contains(&id) {
                spending_cards.push(id);
            }
        }
        let mut account_details = BTreeMap::new();
        for (id, details) in self.account_details {
            if !valid_id(&id) {
                return Err(invalid("account details"));
            }
            if let Some(details) = details.normalized()? {
                account_details.insert(id, details);
            }
        }
        let mut account_names = BTreeMap::new();
        for (id, name) in self.account_names {
            let name = name.trim();
            if !valid_id(&id) || name.chars().any(char::is_control) {
                return Err(invalid("account name"));
            }
            if !name.is_empty() {
                account_names.insert(id, name.chars().take(MAX_NAME_LENGTH).collect());
            }
        }
        for (id, date) in &self.account_start_dates {
            if !valid_id(id) || NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
                return Err(invalid("account start date"));
            }
        }
        if self.account_closed_dates.len() > MAX_ACCOUNTS {
            return Err(invalid("account"));
        }
        for (id, date) in &self.account_closed_dates {
            if !valid_id(id) || NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
                return Err(invalid("account closed date"));
            }
        }
        if self.manual_accounts.len() > MAX_MANUAL_ACCOUNTS {
            return Err(invalid("manual accounts"));
        }
        let manual_accounts = self
            .manual_accounts
            .into_iter()
            .map(ManualAccount::normalized)
            .collect::<Result<Vec<_>, _>>()?;
        let mut manual_ids = BTreeSet::new();
        if manual_accounts
            .iter()
            .any(|account| !manual_ids.insert(account.id.as_str()))
        {
            return Err(invalid("manual accounts"));
        }
        if self.hidden_benefit_ids.len() > MAX_HIDDEN_BENEFITS
            || self
                .hidden_benefit_ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 64 || !id.chars().all(benefit_id_char))
        {
            return Err(invalid("benefit visibility"));
        }
        if self.benefit_cards.len() > MAX_ACCOUNTS
            || self
                .benefit_cards
                .iter()
                .any(|(id, product)| !valid_id(id) || !valid_product(product))
        {
            return Err(invalid("card benefits"));
        }
        let mut products = BTreeSet::new();
        if self.card_rules.len() > MAX_CARD_PRODUCTS
            || self.card_rules.iter().any(|rules| {
                !valid_product(&rules.product)
                    || !products.insert(rules.product.as_str())
                    || rules.benefits.len() > MAX_CARD_BENEFITS
                    || rules
                        .benefits
                        .iter()
                        .filter_map(|benefit| benefit.logo_domain.as_deref())
                        .any(|domain| !valid_domain(domain))
                    || rules.benefits.iter().any(|benefit| {
                        benefit
                            .category
                            .as_deref()
                            .is_some_and(|category| !valid_category(category))
                    })
                    || serde_json::to_vec(rules)
                        .map_or(true, |json| json.len() > MAX_CARD_RULES_BYTES)
            })
        {
            return Err(invalid("card rules"));
        }
        crate::insights::benefits::Catalog::new(&self.card_rules)?;
        if self.card_choices.len() > MAX_ACCOUNTS
            || self.card_choices.iter().any(|choice| {
                !valid_id(&choice.account_id)
                    || choice.candidates.is_empty()
                    || choice.candidates.len() > MAX_CARD_PRODUCTS
                    || choice.candidates.iter().any(|option| {
                        option.id.is_empty()
                            || option.id.len() > 128
                            || option.name.len() > 200
                            || option.issuer.len() > 200
                    })
            })
        {
            return Err(invalid("card choices"));
        }
        if self.account_tax_treatments.len() > MAX_ACCOUNTS
            || self.account_tax_treatments.iter().any(|(id, treatment)| {
                !valid_id(id) || !TAX_TREATMENTS.contains(&treatment.as_str())
            })
        {
            return Err(invalid("account tax treatment"));
        }
        if let Some(profile) = &self.tax_profile {
            if !FILING_STATUSES.contains(&profile.filing_status.as_str())
                || !profile.taxable_income.is_finite()
                || !(0.0..=MAX_INCOME).contains(&profile.taxable_income)
                || profile.state_rate.is_some_and(|rate| {
                    !rate.is_finite() || !(0.0..=MAX_STATE_RATE).contains(&rate)
                })
            {
                return Err(invalid("tax profile"));
            }
        }
        if let Some(profile) = &self.retirement {
            let year = |year: i32| (1900..=2200).contains(&year);
            let amount =
                |value: f64| value.is_finite() && (0.0..=MAX_MANUAL_VALUE).contains(&value);
            if profile.birth_year.is_some_and(|value| !year(value))
                || profile.planned_monthly.is_some_and(|value| !amount(value))
                || profile.earlier_contributions.len() > MAX_ACCOUNTS
                || profile
                    .earlier_contributions
                    .iter()
                    .any(|(id, value)| !valid_id(id) || !amount(*value))
                || profile.contribution_years.len() > MAX_CONTRIBUTION_CHOICES
                || profile
                    .contribution_years
                    .iter()
                    .any(|(id, choice)| !valid_id(id) || choice.is_some_and(|value| !year(value)))
            {
                return Err(invalid("retirement"));
            }
        }
        let hidden_benefit_ids = self
            .hidden_benefit_ids
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if self.people.len() > MAX_PEOPLE {
            return Err(invalid("people"));
        }
        let people = self
            .people
            .into_iter()
            .map(Person::normalized)
            .collect::<Result<Vec<_>, _>>()?;
        let mut person_ids = BTreeSet::new();
        if people
            .iter()
            .any(|person| !person_ids.insert(person.id.as_str()))
        {
            return Err(invalid("people"));
        }
        if self.trip_companions.len() > MAX_TRIP_COMPANIONS {
            return Err(invalid("trip companions"));
        }
        let mut trip_companions = Vec::new();
        for entry in self.trip_companions {
            if entry.transaction_ids.is_empty()
                || entry.transaction_ids.len() > MAX_TRIP_TRANSACTIONS
                || entry.people.len() > MAX_COMPANIONS
                || entry.transaction_ids.iter().any(|id| !valid_id(id))
            {
                return Err(invalid("trip companions"));
            }
            // A person deleted since drops out; an entry left with nobody is dropped too.
            let mut companions = Vec::<String>::new();
            for id in entry.people {
                if person_ids.contains(id.as_str()) && !companions.contains(&id) {
                    companions.push(id);
                }
            }
            if !companions.is_empty() {
                trip_companions.push(TripCompanions {
                    transaction_ids: entry
                        .transaction_ids
                        .into_iter()
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                    people: companions,
                });
            }
        }
        let calendar_id = match self.calendar_id {
            Some(id) if id.trim().is_empty() => None,
            Some(id) if valid_id(id.trim()) => Some(id.trim().to_owned()),
            Some(_) => return Err(invalid("calendar")),
            None => None,
        };
        if self.category_reviews.len() > MAX_REVIEWS
            || self.category_reviews.iter().any(|(id, review)| {
                !valid_id(id) || review.category.trim().is_empty() || review.category.len() > 100
            })
        {
            return Err(invalid("category review"));
        }
        let data_folder = match self.data_folder {
            Some(folder)
                if folder.path.len() <= 1_024
                    && std::path::Path::new(&folder.path).is_absolute()
                    && !folder.path.chars().any(char::is_control) =>
            {
                Some(folder)
            }
            Some(_) => return Err(invalid("data folder")),
            None => None,
        };
        Ok(Self {
            spending_account_id,
            spending_cards,
            account_details,
            account_names,
            account_start_dates: self.account_start_dates,
            account_closed_dates: self.account_closed_dates,
            manual_accounts,
            hidden_benefit_ids,
            retired_category_targets: None,
            plaid_liabilities: self.plaid_liabilities,
            background_refresh: self.background_refresh,
            notifications: self.notifications,
            calendar_sync: self.calendar_sync,
            calendar_id,
            benefit_cards: self.benefit_cards,
            card_rules: self.card_rules,
            card_catalog_off: self.card_catalog_off,
            card_choices: self.card_choices,
            account_tax_treatments: self.account_tax_treatments,
            tax_profile: self.tax_profile.map(|profile| TaxProfile {
                taxable_income: crate::numeric::round2(profile.taxable_income),
                ..profile
            }),
            retirement: self.retirement.map(|profile| RetirementProfile {
                planned_monthly: profile.planned_monthly.map(crate::numeric::round2),
                earlier_contributions: profile
                    .earlier_contributions
                    .into_iter()
                    .map(|(id, value)| (id, crate::numeric::round2(value)))
                    .collect(),
                ..profile
            }),
            plaid_investment_transactions: self.plaid_investment_transactions,
            plaid_recurring: self.plaid_recurring,
            apple_wallet: self.apple_wallet,
            retired_template_wording: None,
            people,
            trip_companions,
            data_folder,
            retired_chapters: None,
            retired_planned_chapters: None,
            retired_explanation_reviews: None,
            retired_programs: None,
            category_reviews: self
                .category_reviews
                .into_iter()
                .map(|(id, review)| {
                    (
                        id,
                        CategoryReview {
                            category: review.category.trim().to_owned(),
                            ..review
                        },
                    )
                })
                .collect(),
            research: self.research,
            local_model: None,
            retired_merchant_sites: None,
            retired_mac_sources: None,
            retired_link_reviews: None,
            retired_person_handles: None,
        })
    }

    /// Imports values saved by older renderer builds without replacing choices made since.
    pub fn merge_missing(&self, imported: Self) -> Self {
        let mut next = self.clone();
        if next.spending_account_id.is_none() {
            next.spending_account_id = imported.spending_account_id;
        }
        for (id, name) in imported.account_names {
            next.account_names.entry(id).or_insert(name);
        }
        if next.spending_cards.is_empty() {
            next.spending_cards = imported.spending_cards;
        }
        for (id, details) in imported.account_details {
            next.account_details.entry(id).or_insert(details);
        }
        for (id, date) in imported.account_start_dates {
            next.account_start_dates.entry(id).or_insert(date);
        }
        for (id, date) in imported.account_closed_dates {
            next.account_closed_dates.entry(id).or_insert(date);
        }
        for account in imported.manual_accounts {
            if !next
                .manual_accounts
                .iter()
                .any(|known| known.id == account.id)
            {
                next.manual_accounts.push(account);
            }
        }
        if next.hidden_benefit_ids.is_empty() {
            next.hidden_benefit_ids = imported.hidden_benefit_ids;
        }
        next.plaid_liabilities |= imported.plaid_liabilities;
        next.background_refresh |= imported.background_refresh;
        next.notifications |= imported.notifications;
        next.card_catalog_off |= imported.card_catalog_off;
        for (id, product) in imported.benefit_cards {
            next.benefit_cards.entry(id).or_insert(product);
        }
        if next.tax_profile.is_none() {
            next.tax_profile = imported.tax_profile;
        }
        if next.retirement.is_none() {
            next.retirement = imported.retirement;
        }
        next.plaid_investment_transactions |= imported.plaid_investment_transactions;
        next.plaid_recurring |= imported.plaid_recurring;
        next.apple_wallet |= imported.apple_wallet;
        for (id, treatment) in imported.account_tax_treatments {
            next.account_tax_treatments.entry(id).or_insert(treatment);
        }
        for person in imported.people {
            if !next.people.iter().any(|known| known.id == person.id) {
                next.people.push(person);
            }
        }
        if next.trip_companions.is_empty() {
            next.trip_companions = imported.trip_companions;
        }
        // Reviews join by id.
        for (id, review) in imported.category_reviews {
            next.category_reviews.entry(id).or_insert(review);
        }
        next
    }
}

/// Settings with no more category reviews than `MAX_REVIEWS`, so answering a suggestion never
/// fails on the cap: reviews of transactions no longer saved go first, then the oldest.
pub fn within_review_limit(
    mut settings: FinanceSettings,
    transactions: &[crate::finance_contract::Transaction],
) -> FinanceSettings {
    let Some(excess) = settings.category_reviews.len().checked_sub(MAX_REVIEWS) else {
        return settings;
    };
    let dates = transactions
        .iter()
        .map(|transaction| (transaction.id.as_str(), transaction.posted_date()))
        .collect::<BTreeMap<_, _>>();
    let mut oldest = settings
        .category_reviews
        .keys()
        .map(|id| (dates.get(id.as_str()).copied(), id.clone()))
        .collect::<Vec<_>>();
    oldest.sort();
    for (_, id) in oldest.into_iter().take(excess) {
        settings.category_reviews.remove(&id);
    }
    settings
}

fn benefit_id_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '-'
}

/// A bare domain such as `example.com`: no scheme, path, or port.
fn valid_domain(domain: &str) -> bool {
    domain.len() <= 253
        && domain.contains('.')
        && crate::insights::forecast::website_domain(domain).as_deref() == Some(domain)
}

/// A benefit category: a short lowercase word, such as `dining`.
fn valid_category(category: &str) -> bool {
    !category.is_empty()
        && category.len() <= 32
        && category
            .chars()
            .all(|character| character.is_ascii_lowercase() || character == '-')
}

/// A card product id: a short lowercase slug, such as `example-card`.
fn valid_product(product: &str) -> bool {
    !product.is_empty()
        && product.len() <= 64
        && product.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `read` holds every value `saved` does; it may add defaults.
    fn keeps(saved: &serde_json::Value, read: &serde_json::Value) -> bool {
        use serde_json::Value;
        match (saved, read) {
            (Value::Object(saved), Value::Object(read)) => saved
                .iter()
                .all(|(key, value)| read.get(key).is_some_and(|read| keeps(value, read))),
            (Value::Array(saved), Value::Array(read)) => {
                saved.len() == read.len() && saved.iter().zip(read).all(|(s, r)| keeps(s, r))
            }
            (Value::Number(saved), Value::Number(read)) => saved.as_f64() == read.as_f64(),
            _ => saved == read,
        }
    }

    /// The renderer's test parses the same file, so a setting either side drops fails one.
    #[test]
    fn every_setting_survives_a_save_and_a_read() {
        let saved: serde_json::Value =
            serde_json::from_str(include_str!("../../src/data/fixtures/full-settings.json"))
                .unwrap();
        let settings = serde_json::from_value::<FinanceSettings>(saved.clone())
            .unwrap()
            .normalized()
            .unwrap();
        let read = serde_json::to_value(settings).unwrap();
        assert!(keeps(&saved, &read), "{read:#}");
    }

    #[test]
    fn a_saved_local_model_is_dropped_on_the_next_save() {
        let settings = FinanceSettings {
            local_model: Some("example-model".into()),
            ..Default::default()
        };
        assert_eq!(settings.normalized().unwrap().local_model, None);
    }

    #[test]
    fn normalizes_names_and_benefits_and_rejects_invalid_values() {
        let settings = FinanceSettings {
            spending_account_id: Some("  plaid:card  ".into()),
            account_names: BTreeMap::from([
                ("plaid:one".into(), "  Everyday  ".into()),
                ("plaid:two".into(), "   ".into()),
            ]),
            account_start_dates: BTreeMap::from([("plaid:one".into(), "2026-01-31".into())]),
            hidden_benefit_ids: vec!["travel".into(), "dining".into(), "travel".into()],
            plaid_liabilities: true,
            ..Default::default()
        }
        .normalized()
        .unwrap();
        assert_eq!(settings.spending_account_id.as_deref(), Some("plaid:card"));
        assert_eq!(
            settings.account_names,
            BTreeMap::from([("plaid:one".into(), "Everyday".into())])
        );
        assert_eq!(settings.hidden_benefit_ids, vec!["dining", "travel"]);
        assert!(settings.plaid_liabilities);

        for invalid in [
            FinanceSettings {
                account_start_dates: BTreeMap::from([("a".into(), "2026-02-30".into())]),
                ..Default::default()
            },
            FinanceSettings {
                hidden_benefit_ids: vec!["../x".into()],
                ..Default::default()
            },
        ] {
            assert!(invalid.normalized().is_err());
        }
    }

    #[test]
    fn importing_never_replaces_existing_choices() {
        let current = FinanceSettings {
            spending_account_id: Some("plaid:current".into()),
            account_names: BTreeMap::from([("plaid:one".into(), "Kept".into())]),
            ..Default::default()
        };
        let merged = current.merge_missing(FinanceSettings {
            spending_account_id: Some("plaid:legacy".into()),
            account_names: BTreeMap::from([
                ("plaid:one".into(), "Legacy".into()),
                ("plaid:two".into(), "Added".into()),
            ]),
            hidden_benefit_ids: vec!["dining".into()],
            ..Default::default()
        });
        assert_eq!(merged.spending_account_id.as_deref(), Some("plaid:current"));
        assert_eq!(merged.account_names["plaid:one"], "Kept");
        assert_eq!(merged.account_names["plaid:two"], "Added");
        assert_eq!(merged.hidden_benefit_ids, vec!["dining"]);
    }

    #[test]
    fn spending_cards_and_account_details_are_validated() {
        let settings = FinanceSettings {
            spending_account_id: Some("plaid:card".into()),
            spending_cards: vec![
                " plaid:other ".into(),
                "plaid:card".into(),
                "plaid:other".into(),
            ],
            account_details: BTreeMap::from([
                (
                    "plaid:savings".into(),
                    AccountDetails {
                        website: Some(" https://bank.example/savings ".into()),
                        apy: Some(4.12345),
                    },
                ),
                ("plaid:empty".into(), AccountDetails::default()),
            ]),
            ..Default::default()
        }
        .normalized()
        .unwrap();
        // The saved card is already a spending card, and repeats collapse.
        assert_eq!(settings.spending_cards, ["plaid:other"]);
        assert_eq!(
            settings.spending_account_ids().collect::<Vec<_>>(),
            ["plaid:card", "plaid:other"]
        );
        let savings = &settings.account_details["plaid:savings"];
        assert_eq!(
            savings.website.as_deref(),
            Some("https://bank.example/savings")
        );
        assert_eq!(savings.apy, Some(4.123));
        assert!(!settings.account_details.contains_key("plaid:empty"));

        for details in [
            AccountDetails {
                website: Some("http://bank.example".into()),
                apy: None,
            },
            AccountDetails {
                website: None,
                apy: Some(120.0),
            },
        ] {
            let invalid = FinanceSettings {
                account_details: BTreeMap::from([("plaid:savings".into(), details)]),
                ..Default::default()
            };
            assert!(invalid.normalized().is_err());
        }
    }

    #[test]
    fn manual_accounts_and_closed_dates_are_validated_and_ordered() {
        let account = |kind: &str, entries: Vec<(&str, f64)>| ManualAccount {
            id: "manual:one".into(),
            name: " Old 401(k) ".into(),
            kind: kind.into(),
            tracks_sp500: false,
            entries: entries
                .into_iter()
                .map(|(date, value)| ManualEntry {
                    date: date.into(),
                    value,
                })
                .collect(),
        };
        let settings = FinanceSettings {
            manual_accounts: vec![account(
                "retirement",
                vec![("2026-03-01", 12_000.0), ("2026-01-01", 10_000.004)],
            )],
            account_closed_dates: BTreeMap::from([("plaid:old".into(), "2026-05-31".into())]),
            ..Default::default()
        }
        .normalized()
        .unwrap();
        let saved = &settings.manual_accounts[0];
        assert_eq!(saved.name, "Old 401(k)");
        assert_eq!(
            saved.entries,
            vec![
                ManualEntry {
                    date: "2026-01-01".into(),
                    value: 10_000.0
                },
                ManualEntry {
                    date: "2026-03-01".into(),
                    value: 12_000.0
                },
            ]
        );
        assert_eq!(saved.value_on("2026-02-15"), Some(10_000.0));
        assert_eq!(saved.value_on("2025-12-31"), None);
        let loan = account("loan", vec![("2026-01-01", 5_000.0)])
            .normalized()
            .unwrap();
        assert_eq!(loan.value_on("2026-06-01"), Some(-5_000.0));
        // An imported statement can value an account entered without a balance.
        assert!(account("retirement", vec![]).normalized().is_ok());
        for invalid in [
            account("crypto", vec![("2026-01-01", 1.0)]),
            account("cash", vec![("2026-02-30", 1.0)]),
            account("cash", vec![("2026-01-01", f64::NAN)]),
        ] {
            assert!(invalid.normalized().is_err());
        }
        assert!(FinanceSettings {
            manual_accounts: vec![
                account("cash", vec![("2026-01-01", 1.0)]),
                account("cash", vec![("2026-01-01", 2.0)]),
            ],
            ..Default::default()
        }
        .normalized()
        .is_err());
        assert!(FinanceSettings {
            account_closed_dates: BTreeMap::from([("plaid:old".into(), "soon".into())]),
            ..Default::default()
        }
        .normalized()
        .is_err());
        // Neither is written while empty, so older settings files stay unchanged.
        let json = serde_json::to_string(&FinanceSettings::default()).unwrap();
        assert!(!json.contains("manualAccounts") && !json.contains("accountClosedDates"));
    }

    #[test]
    fn settings_saved_with_retired_targets_still_load_without_them() {
        let settings: FinanceSettings =
            serde_json::from_str(r#"{"categoryTargets":{"Dining":250.5},"plaidLiabilities":true}"#)
                .unwrap();
        assert!(settings.plaid_liabilities);
        let saved = serde_json::to_value(settings.normalized().unwrap()).unwrap();
        assert!(saved.get("categoryTargets").is_none());
    }

    #[test]
    fn settings_saved_with_artha_keys_still_load_without_them() {
        let settings: FinanceSettings = serde_json::from_str(
            r#"{"merchantSites":{"clear":"example.com"},"macSources":["mail"],
                "linkReviews":{"event:a|spent|transaction:b":{"confirmed":true,"keys":["event-name"]}},
                "personHandles":{"someone@example.com":"person:1"},"research":true}"#,
        )
        .unwrap();
        assert!(settings.research);
        let saved = serde_json::to_value(settings.normalized().unwrap()).unwrap();
        for key in [
            "merchantSites",
            "macSources",
            "linkReviews",
            "personHandles",
        ] {
            assert!(saved.get(key).is_none(), "{key}");
        }
    }

    #[test]
    fn card_benefit_assignments_name_a_product_and_imports_never_replace_them() {
        let assigned = |product: &str| FinanceSettings {
            benefit_cards: [("plaid:card".to_owned(), product.to_owned())].into(),
            ..Default::default()
        };
        // A product whose rules aren't saved tracks nothing, so removing rules never makes
        // settings invalid; only a malformed id is refused.
        assert!(assigned("example-card").normalized().is_ok());
        assert!(assigned("unsaved-card").normalized().is_ok());
        assert!(assigned("Not a product!").normalized().is_err());
        // Saved rules must compile, and each product is saved once.
        let rules = crate::insights::benefits::example_rules();
        let with_rules = |rules: Vec<crate::insights::benefits::CardRules>| FinanceSettings {
            card_rules: rules,
            ..Default::default()
        };
        assert!(with_rules(vec![rules.clone()]).normalized().is_ok());
        assert!(with_rules(vec![rules.clone(), rules.clone()])
            .normalized()
            .is_err());
        let mut broken = rules.clone();
        broken.benefits[0].matches = "(unclosed".into();
        assert!(with_rules(vec![broken]).normalized().is_err());
        let logo = |domain: &str| {
            let mut rules = rules.clone();
            rules.benefits[0].logo_domain = Some(domain.into());
            with_rules(vec![rules]).normalized()
        };
        assert!(logo("tv.example.com").is_ok());
        assert!(logo("https://tv.example.com/").is_err());
        assert!(logo("example").is_err());
        let category = |category: &str| {
            let mut rules = rules.clone();
            rules.benefits[0].category = Some(category.into());
            with_rules(vec![rules]).normalized()
        };
        assert!(category("digital-entertainment").is_ok());
        assert!(category("Dining").is_err());
        assert!(category("").is_err());
        let choice = |account: &str, candidates: usize| FinanceSettings {
            card_choices: vec![CardChoice {
                account_id: account.into(),
                candidates: (0..candidates)
                    .map(|index| CardChoiceOption {
                        id: format!("card-{index}"),
                        name: "Example Card".into(),
                        issuer: "Example Bank".into(),
                    })
                    .collect(),
            }],
            ..Default::default()
        };
        assert!(choice("plaid:card", 2).normalized().is_ok());
        assert!(choice("plaid:card", 0).normalized().is_err());
        assert!(choice("plaid:card", MAX_CARD_PRODUCTS + 1)
            .normalized()
            .is_err());
        assert!(choice("", 1).normalized().is_err());
        let treated = |treatment: &str| FinanceSettings {
            account_tax_treatments: [("plaid:hsa".to_owned(), treatment.to_owned())].into(),
            ..Default::default()
        };
        assert!(treated("taxFree").normalized().is_ok());
        assert!(treated("exempt").normalized().is_err());
        let profiled = |status: &str, income: f64, state: Option<f64>| FinanceSettings {
            tax_profile: Some(TaxProfile {
                filing_status: status.into(),
                taxable_income: income,
                state_rate: state,
            }),
            ..Default::default()
        };
        assert!(profiled("marriedJoint", 180_000.0, Some(5.0))
            .normalized()
            .is_ok());
        for invalid in [
            profiled("married", 1.0, None),
            profiled("single", -1.0, None),
            profiled("single", f64::NAN, None),
            profiled("single", 1.0, Some(40.0)),
        ] {
            assert!(invalid.normalized().is_err());
        }
        let merged = assigned("example-card").merge_missing(FinanceSettings {
            notifications: true,
            benefit_cards: [
                ("plaid:card".to_owned(), "other".to_owned()),
                ("plaid:new".to_owned(), "example-card".to_owned()),
            ]
            .into(),
            ..Default::default()
        });
        assert!(merged.notifications);
        assert_eq!(merged.benefit_cards["plaid:card"], "example-card");
        assert_eq!(merged.benefit_cards["plaid:new"], "example-card");
        // An empty map is not written, so settings saved before it existed are unchanged.
        assert!(!serde_json::to_string(&FinanceSettings::default())
            .unwrap()
            .contains("benefitCards"));
    }

    #[test]
    fn people_are_validated_and_trips_keep_only_people_who_exist() {
        let person = |id: &str, names: &[&str]| Person {
            id: id.into(),
            name: " Example Person ".into(),
            contact_id: Some(" ".into()),
            match_names: names.iter().map(|name| (*name).into()).collect(),
        };
        let settings = FinanceSettings {
            people: vec![person(
                "person:a",
                &["Example  Person", "example-person", ""],
            )],
            trip_companions: vec![
                TripCompanions {
                    transaction_ids: vec!["t2".into(), "t1".into(), "t1".into()],
                    people: vec!["person:a".into(), "person:gone".into(), "person:a".into()],
                },
                TripCompanions {
                    transaction_ids: vec!["t3".into()],
                    people: vec!["person:gone".into()],
                },
            ],
            ..Default::default()
        }
        .normalized()
        .unwrap();
        let saved = &settings.people[0];
        assert_eq!(saved.name, "Example Person");
        assert_eq!(saved.contact_id, None);
        // Match names compare as banks print them: one spelling each.
        assert_eq!(saved.match_names, ["EXAMPLE PERSON"]);
        assert_eq!(
            settings.trip_companions,
            [TripCompanions {
                transaction_ids: vec!["t1".into(), "t2".into()],
                people: vec!["person:a".into()],
            }]
        );
        for invalid in [
            vec![person("manual:a", &[])],
            vec![person("person:a", &[]), person("person:a", &[])],
            vec![person("person:a", &["A"; 9])],
            vec![Person {
                name: " ".into(),
                ..person("person:a", &[])
            }],
        ] {
            let settings = FinanceSettings {
                people: invalid,
                ..Default::default()
            };
            assert!(settings.normalized().is_err());
        }
        // Importing adds people not yet saved and never replaces one.
        let merged = FinanceSettings {
            people: vec![person("person:a", &[])],
            ..Default::default()
        }
        .merge_missing(FinanceSettings {
            people: vec![
                Person {
                    name: "Replaced".into(),
                    ..person("person:a", &[])
                },
                person("person:b", &[]),
            ],
            ..Default::default()
        });
        assert_eq!(merged.people.len(), 2);
        assert_eq!(merged.people[0].name, " Example Person ");
    }

    #[test]
    fn settings_saved_with_chapters_and_rules_still_load_without_them() {
        let settings: FinanceSettings = serde_json::from_str(
            r#"{"chapters":[{"id":"work:example","status":"confirmed"}],
                "plannedChapters":[{"id":"planned:example","title":"Example plan","start":"2026-11-01"}],
                "explanationReviews":{"rule:a":"dismissed"},
                "programs":[{"id":"example"}],
                "research":true}"#,
        )
        .unwrap();
        assert!(settings.research);
        let saved = serde_json::to_value(settings.normalized().unwrap()).unwrap();
        for key in [
            "chapters",
            "plannedChapters",
            "explanationReviews",
            "programs",
        ] {
            assert!(saved.get(key).is_none(), "{key} was kept");
        }
    }
}

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
    /// tracked without an entry.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub benefit_cards: BTreeMap<String, String>,
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
    /// Your word on detected chapters: confirmed or dismissed, with any title and dates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chapters: Vec<ChapterChoice>,
    /// Chapters you planned ahead, such as moving out, with their expected monthly costs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planned_chapters: Vec<PlannedChapter>,
    /// Your word on Suggested explanations, by explanation id. A dismissal is kept so Brief
    /// never asks twice.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub explanation_reviews: BTreeMap<String, ExplanationReview>,
    /// Program rules you approved from research, with their citations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub programs: Vec<crate::evidence::programs::Program>,
    /// Opt-in web research: generic queries from public names, each logged, only when on.
    pub research: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum ChapterStatus {
    Confirmed,
    Dismissed,
}

/// Your word on one detected chapter.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ChapterChoice {
    pub id: String,
    pub status: ChapterStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end: Option<NaiveDate>,
}

/// An expected monthly cost of a planned chapter, such as rent.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlannedCost {
    pub label: String,
    /// Positive: what it costs each month.
    pub monthly: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PlannedChapter {
    /// `planned:` followed by an identifier the renderer chose.
    pub id: String,
    pub title: String,
    pub start: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub costs: Vec<PlannedCost>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
pub enum ExplanationReview {
    Confirmed,
    Dismissed,
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

const MAX_CHAPTERS: usize = 200;
const MAX_PLANNED_COSTS: usize = 20;
const MAX_REVIEWS: usize = 2_000;
const MAX_PROGRAMS: usize = 100;
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
            || self.benefit_cards.iter().any(|(id, product)| {
                !valid_id(id) || crate::insights::benefits::product(product).is_none()
            })
        {
            return Err(invalid("card benefits"));
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
        if self.chapters.len() > MAX_CHAPTERS
            || self.planned_chapters.len() > MAX_CHAPTERS
            || self.explanation_reviews.len() > MAX_REVIEWS
            || self.programs.len() > MAX_PROGRAMS
        {
            return Err(invalid("evidence"));
        }
        let text_ok = |text: &str| !text.trim().is_empty() && text.len() <= MAX_NAME_LENGTH;
        let mut chapters = Vec::<ChapterChoice>::new();
        for choice in self.chapters {
            if !valid_id(&choice.id)
                || choice.title.as_deref().is_some_and(|title| !text_ok(title))
                || choice
                    .start
                    .zip(choice.end)
                    .is_some_and(|(start, end)| end < start)
            {
                return Err(invalid("chapter"));
            }
            chapters.retain(|known| known.id != choice.id);
            chapters.push(ChapterChoice {
                title: choice.title.map(|title| title.trim().to_owned()),
                ..choice
            });
        }
        let mut planned_chapters = Vec::<PlannedChapter>::new();
        for plan in self.planned_chapters {
            if !plan.id.starts_with("planned:")
                || !valid_id(&plan.id)
                || !text_ok(&plan.title)
                || plan.end.is_some_and(|end| end < plan.start)
                || plan.costs.len() > MAX_PLANNED_COSTS
                || plan.costs.iter().any(|cost| {
                    !text_ok(&cost.label)
                        || !cost.monthly.is_finite()
                        || cost.monthly < 0.0
                        || cost.monthly > MAX_MANUAL_VALUE
                })
            {
                return Err(invalid("planned chapter"));
            }
            planned_chapters.retain(|known| known.id != plan.id);
            planned_chapters.push(PlannedChapter {
                title: plan.title.trim().to_owned(),
                costs: plan
                    .costs
                    .into_iter()
                    .map(|cost| PlannedCost {
                        label: cost.label.trim().to_owned(),
                        monthly: crate::numeric::round2(cost.monthly),
                    })
                    .collect(),
                ..plan
            });
        }
        if self.explanation_reviews.keys().any(|id| !valid_id(id)) {
            return Err(invalid("explanation review"));
        }
        for program in &self.programs {
            program.validate()?;
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
            retired_template_wording: None,
            people,
            trip_companions,
            data_folder,
            chapters,
            planned_chapters,
            explanation_reviews: self.explanation_reviews,
            programs: self.programs,
            research: self.research,
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
        // Chapter choices, plans, reviews, and approved rules join by id; research stays as
        // set here, since turning it on is a choice about this Mac's queries.
        for choice in imported.chapters {
            if !next.chapters.iter().any(|known| known.id == choice.id) {
                next.chapters.push(choice);
            }
        }
        for plan in imported.planned_chapters {
            if !next
                .planned_chapters
                .iter()
                .any(|known| known.id == plan.id)
            {
                next.planned_chapters.push(plan);
            }
        }
        for (id, review) in imported.explanation_reviews {
            next.explanation_reviews.entry(id).or_insert(review);
        }
        for program in imported.programs {
            if !next.programs.iter().any(|known| known.id == program.id) {
                next.programs.push(program);
            }
        }
        next
    }
}

fn benefit_id_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_names_and_benefits_and_rejects_invalid_values() {
        let settings = FinanceSettings {
            spending_account_id: Some("  plaid:card  ".into()),
            account_names: BTreeMap::from([
                ("plaid:one".into(), "  Everyday  ".into()),
                ("plaid:two".into(), "   ".into()),
            ]),
            account_start_dates: BTreeMap::from([("plaid:one".into(), "2026-01-31".into())]),
            hidden_benefit_ids: vec!["uber-cash".into(), "resy".into(), "uber-cash".into()],
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
        assert_eq!(settings.hidden_benefit_ids, vec!["resy", "uber-cash"]);
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
            hidden_benefit_ids: vec!["resy".into()],
            ..Default::default()
        });
        assert_eq!(merged.spending_account_id.as_deref(), Some("plaid:current"));
        assert_eq!(merged.account_names["plaid:one"], "Kept");
        assert_eq!(merged.account_names["plaid:two"], "Added");
        assert_eq!(merged.hidden_benefit_ids, vec!["resy"]);
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
    fn card_benefit_assignments_name_a_known_product_and_imports_never_replace_them() {
        let assigned = |product: &str| FinanceSettings {
            benefit_cards: [("plaid:card".to_owned(), product.to_owned())].into(),
            ..Default::default()
        };
        assert!(assigned("amex-platinum").normalized().is_ok());
        assert!(assigned("made-up-card").normalized().is_err());
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
        let merged = assigned("amex-platinum").merge_missing(FinanceSettings {
            notifications: true,
            benefit_cards: [
                ("plaid:card".to_owned(), "other".to_owned()),
                ("plaid:new".to_owned(), "amex-platinum".to_owned()),
            ]
            .into(),
            ..Default::default()
        });
        assert!(merged.notifications);
        assert_eq!(merged.benefit_cards["plaid:card"], "amex-platinum");
        assert_eq!(merged.benefit_cards["plaid:new"], "amex-platinum");
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
    fn evidence_choices_are_checked_and_imports_join_them_by_id() {
        let day = |text: &str| NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap();
        let plan = |id: &str| PlannedChapter {
            id: id.into(),
            title: " Moving out ".into(),
            start: day("2026-11-01"),
            end: None,
            costs: vec![PlannedCost {
                label: " Rent ".into(),
                monthly: 1_800.004,
            }],
        };
        let settings = FinanceSettings {
            chapters: vec![
                ChapterChoice {
                    id: "work:example".into(),
                    status: ChapterStatus::Dismissed,
                    title: None,
                    start: None,
                    end: None,
                },
                // A later choice for the same chapter replaces the earlier one.
                ChapterChoice {
                    id: "work:example".into(),
                    status: ChapterStatus::Confirmed,
                    title: Some(" New job ".into()),
                    start: None,
                    end: None,
                },
            ],
            planned_chapters: vec![plan("planned:move")],
            explanation_reviews: BTreeMap::from([("rule:a".into(), ExplanationReview::Confirmed)]),
            ..Default::default()
        }
        .normalized()
        .unwrap();
        assert_eq!(settings.chapters.len(), 1);
        assert_eq!(settings.chapters[0].status, ChapterStatus::Confirmed);
        assert_eq!(settings.chapters[0].title.as_deref(), Some("New job"));
        assert_eq!(settings.planned_chapters[0].title, "Moving out");
        assert_eq!(settings.planned_chapters[0].costs[0].label, "Rent");
        assert_eq!(settings.planned_chapters[0].costs[0].monthly, 1_800.0);

        let backwards = ChapterChoice {
            id: "home:ca".into(),
            status: ChapterStatus::Confirmed,
            title: None,
            start: Some(day("2026-05-01")),
            end: Some(day("2026-04-01")),
        };
        let mut uncited = crate::evidence::programs::programs(&[])[0].clone();
        uncited.source.url = "http://example.com".into();
        for invalid in [
            FinanceSettings {
                chapters: vec![backwards],
                ..Default::default()
            },
            FinanceSettings {
                planned_chapters: vec![plan("move")],
                ..Default::default()
            },
            FinanceSettings {
                planned_chapters: vec![PlannedChapter {
                    costs: vec![PlannedCost {
                        label: "Rent".into(),
                        monthly: -1.0,
                    }],
                    ..plan("planned:negative")
                }],
                ..Default::default()
            },
            FinanceSettings {
                programs: vec![uncited],
                ..Default::default()
            },
        ] {
            assert!(invalid.normalized().is_err());
        }

        let merged = FinanceSettings {
            planned_chapters: vec![plan("planned:move")],
            ..Default::default()
        }
        .merge_missing(FinanceSettings {
            planned_chapters: vec![
                PlannedChapter {
                    title: "Replaced".into(),
                    ..plan("planned:move")
                },
                plan("planned:other"),
            ],
            explanation_reviews: BTreeMap::from([("rule:a".into(), ExplanationReview::Dismissed)]),
            research: true,
            ..Default::default()
        });
        assert_eq!(merged.planned_chapters.len(), 2);
        assert_eq!(merged.planned_chapters[0].title, " Moving out ");
        assert_eq!(merged.explanation_reviews.len(), 1);
        assert!(!merged.research);
    }
}

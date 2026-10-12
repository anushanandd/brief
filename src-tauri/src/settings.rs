//! Durable choices that change financial results. They live in the finance database so
//! they are backed up, restored, and erased with the data they describe.
use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::{de::IgnoredAny, Deserialize, Serialize};

const MAX_ACCOUNTS: usize = 500;
const MAX_ID_LENGTH: usize = 220;
const MAX_NAME_LENGTH: usize = 80;
const MAX_HIDDEN_BENEFITS: usize = 100;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct FinanceSettings {
    /// The saved credit account used by Spending and card benefits. Never inferred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spending_account_id: Option<String>,
    /// Display names keyed by account ID. Provider names remain the fallback.
    pub account_names: BTreeMap<String, String>,
    /// First day each account's history should be charted, as `YYYY-MM-DD`.
    pub account_start_dates: BTreeMap<String, String>,
    pub hidden_benefit_ids: Vec<String>,
    /// Monthly targets were retired; settings saved with them still load and drop the key.
    #[serde(rename = "categoryTargets", skip_serializing)]
    pub retired_category_targets: Option<IgnoredAny>,
    /// Request card statement details from Plaid Liabilities, a separately billed product.
    pub plaid_liabilities: bool,
    /// Keep running after the window closes and refresh saved data once a day.
    pub background_refresh: bool,
    /// Post a macOS notification when a background refresh finds something that needs action.
    pub notifications: bool,
    /// Card products tracked for benefits, keyed by credit account ID. The spending card is
    /// tracked without an entry.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub benefit_cards: BTreeMap<String, String>,
    /// How an account is taxed when its provider subtype says otherwise or nothing:
    /// `taxable`, `taxDeferred`, or `taxFree`, keyed by account ID.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub account_tax_treatments: BTreeMap<String, String>,
    /// What tax estimates stack on. Without it Brief reports amounts but no tax.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tax_profile: Option<TaxProfile>,
    /// Request Plaid Investments Transactions, a separately billed subscription per connection.
    pub plaid_investment_transactions: bool,
    /// Request Plaid Recurring Transactions, a separately billed subscription per connection.
    pub plaid_recurring: bool,
    /// The on-device summary wording choice was retired with the model's phrases; settings
    /// saved with it still load and drop the key.
    #[serde(rename = "templateWording", skip_serializing)]
    pub retired_template_wording: Option<IgnoredAny>,
}

pub const FILING_STATUSES: [&str; 4] = [
    "single",
    "marriedJoint",
    "marriedSeparate",
    "headOfHousehold",
];
const MAX_INCOME: f64 = 100_000_000.0;
const MAX_STATE_RATE: f64 = 20.0;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TaxProfile {
    pub filing_status: String,
    /// Taxable income for the year from everything Brief doesn't see (wages, business
    /// income), after deductions.
    pub taxable_income: f64,
    /// A flat state income tax rate in percent, when the user wants state tax included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_rate: Option<f64>,
}

pub const TAX_TREATMENTS: [&str; 3] = ["taxable", "taxDeferred", "taxFree"];

impl FinanceSettings {
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
        if self.account_names.len() > MAX_ACCOUNTS || self.account_start_dates.len() > MAX_ACCOUNTS
        {
            return Err(invalid("account"));
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
        let hidden_benefit_ids = self
            .hidden_benefit_ids
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(Self {
            spending_account_id,
            account_names,
            account_start_dates: self.account_start_dates,
            hidden_benefit_ids,
            retired_category_targets: None,
            plaid_liabilities: self.plaid_liabilities,
            background_refresh: self.background_refresh,
            notifications: self.notifications,
            benefit_cards: self.benefit_cards,
            account_tax_treatments: self.account_tax_treatments,
            tax_profile: self.tax_profile.map(|profile| TaxProfile {
                taxable_income: crate::numeric::round2(profile.taxable_income),
                ..profile
            }),
            plaid_investment_transactions: self.plaid_investment_transactions,
            plaid_recurring: self.plaid_recurring,
            retired_template_wording: None,
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
        for (id, date) in imported.account_start_dates {
            next.account_start_dates.entry(id).or_insert(date);
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
        next.plaid_investment_transactions |= imported.plaid_investment_transactions;
        next.plaid_recurring |= imported.plaid_recurring;
        for (id, treatment) in imported.account_tax_treatments {
            next.account_tax_treatments.entry(id).or_insert(treatment);
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
}

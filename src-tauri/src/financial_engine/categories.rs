//! Plaid personal finance categories, presented as a small set of top-level categories with
//! a concise label from Plaid's detailed code. The mapping is versioned data in
//! `categories.json`; the transaction kind it carries drives classification, so display
//! names never decide whether something is spending.
use std::{collections::BTreeMap, sync::LazyLock};

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CodeKind {
    Expense,
    Income,
    Interest,
    Dividend,
    Transfer,
    Payment,
    Fee,
    Tax,
}

#[derive(Debug, Deserialize)]
struct Detailed {
    category: String,
    label: Option<String>,
    kind: CodeKind,
}

#[derive(Debug, Deserialize)]
struct Primary {
    category: String,
    kind: CodeKind,
}

#[derive(Debug, Deserialize)]
struct Mapping {
    primary: BTreeMap<String, Primary>,
    detailed: BTreeMap<String, Detailed>,
}

static MAPPING: LazyLock<Mapping> = LazyLock::new(|| {
    serde_json::from_str(include_str!("categories.json")).expect("category mapping is valid")
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Categorized {
    pub category: String,
    /// The specific label, omitted when Plaid's code is generic or Plaid has no confidence.
    pub label: Option<String>,
}

/// The top-level category and label for a Plaid category. Unknown detailed codes fall back
/// to their primary code; unknown primary codes keep Plaid's own name, title-cased.
pub fn categorize(primary: &str, detailed: Option<&str>, confidence: Option<&str>) -> Categorized {
    // Plaid rates a third of real activity LOW while the code is usually right, and the
    // top-level category already relies on it; only an UNKNOWN rating hides the label.
    let low_confidence = confidence == Some("UNKNOWN");
    if let Some(entry) = detailed.and_then(|code| MAPPING.detailed.get(code)) {
        return Categorized {
            category: entry.category.clone(),
            label: entry.label.clone().filter(|_| !low_confidence),
        };
    }
    Categorized {
        category: MAPPING
            .primary
            .get(primary)
            .map(|entry| entry.category.clone())
            .unwrap_or_else(|| title_case(primary)),
        label: None,
    }
}

/// The transaction kind a Plaid detailed code implies, if it is a known code.
pub fn kind_for(detailed: &str) -> Option<CodeKind> {
    MAPPING
        .detailed
        .get(detailed)
        .map(|entry| entry.kind)
        .or_else(|| {
            MAPPING
                .primary
                .iter()
                .find(|(primary, _)| detailed.starts_with(&format!("{primary}_")))
                .map(|(_, entry)| entry.kind)
        })
}

pub fn title_case(value: &str) -> String {
    value
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_detailed_codes_to_categories_with_concise_labels() {
        let coffee = categorize(
            "FOOD_AND_DRINK",
            Some("FOOD_AND_DRINK_COFFEE"),
            Some("HIGH"),
        );
        assert_eq!(coffee.category, "Dining");
        assert_eq!(coffee.label.as_deref(), Some("Coffee"));
        let groceries = categorize("FOOD_AND_DRINK", Some("FOOD_AND_DRINK_GROCERIES"), None);
        assert_eq!(groceries.category, "Groceries");
        assert_eq!(
            groceries.label, None,
            "a category that is already specific needs no label"
        );
        let generic = categorize(
            "GENERAL_SERVICES",
            Some("GENERAL_SERVICES_OTHER_GENERAL_SERVICES"),
            None,
        );
        assert_eq!(
            (generic.category.as_str(), generic.label),
            ("Services", None)
        );
        let unsure = categorize("TRAVEL", Some("TRAVEL_FLIGHTS"), Some("UNKNOWN"));
        assert_eq!((unsure.category.as_str(), unsure.label), ("Travel", None));
        // v1 codes from older caches and codes Plaid adds later still land sensibly.
        assert_eq!(
            categorize("INCOME", Some("INCOME_WAGES"), None)
                .label
                .as_deref(),
            Some("Paycheck")
        );
        assert_eq!(
            categorize("TRAVEL", Some("TRAVEL_SPACE"), None).category,
            "Travel"
        );
        assert_eq!(categorize("NEW_THING", None, None).category, "New Thing");
    }

    #[test]
    fn every_code_has_a_kind_that_drives_classification() {
        assert_eq!(kind_for("INCOME_INTEREST_EARNED"), Some(CodeKind::Interest));
        assert_eq!(
            kind_for("LOAN_PAYMENTS_CREDIT_CARD_PAYMENT"),
            Some(CodeKind::Payment)
        );
        assert_eq!(
            kind_for("TRANSFER_OUT_TRANSFER_OUT_FROM_APPS"),
            Some(CodeKind::Expense)
        );
        assert_eq!(
            kind_for("GOVERNMENT_AND_NON_PROFIT_TAX_PAYMENT"),
            Some(CodeKind::Tax)
        );
        assert_eq!(
            kind_for("TRANSFER_IN_SOMETHING_NEW"),
            Some(CodeKind::Transfer)
        );
        assert_eq!(kind_for("UNKNOWN"), None);
        for (code, entry) in &MAPPING.detailed {
            let primary = MAPPING
                .primary
                .keys()
                .filter(|primary| code.starts_with(&format!("{primary}_")))
                .max_by_key(|primary| primary.len());
            assert!(primary.is_some(), "{code} has no primary fallback");
            assert!(!entry.category.is_empty());
        }
    }
}

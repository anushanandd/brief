//! Local category rules. A rule assigns a category before transaction classification;
//! an individual transaction annotation always takes precedence over every rule.
use serde::{Deserialize, Serialize};

const MAX_RULES: usize = 1_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleMatch {
    /// The normalized merchant equals the pattern.
    Exact,
    /// The normalized merchant or description contains the pattern as whole words.
    Contains,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CategoryRule {
    pub id: String,
    pub pattern: String,
    #[serde(rename = "match")]
    pub match_kind: RuleMatch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub category: String,
    pub created_at: String,
}

/// Lowercase alphanumeric words separated by single spaces; reference numbers and masked
/// card fragments are removed so recurring descriptors normalize to the same text.
pub fn normalize_merchant(value: &str) -> String {
    value
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| {
            !word.is_empty() && (word.len() < 5 || !word.chars().all(|c| c.is_ascii_digit()))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl CategoryRule {
    pub fn matches(
        &self,
        merchant: &str,
        description: Option<&str>,
        account_id: Option<&str>,
    ) -> bool {
        if self
            .account_id
            .as_deref()
            .is_some_and(|required| Some(required) != account_id)
        {
            return false;
        }
        let merchant = normalize_merchant(merchant);
        match self.match_kind {
            RuleMatch::Exact => merchant == self.pattern,
            RuleMatch::Contains => [Some(merchant), description.map(normalize_merchant)]
                .into_iter()
                .flatten()
                .any(|text| format!(" {text} ").contains(&format!(" {} ", self.pattern))),
        }
    }

    pub fn normalized(mut self) -> Result<Self, String> {
        self.pattern = normalize_merchant(&self.pattern);
        self.category = self.category.trim().to_owned();
        self.account_id = self.account_id.filter(|id| !id.trim().is_empty());
        if self.id.is_empty()
            || self.id.len() > 64
            || self.pattern.is_empty()
            || self.pattern.len() > 200
            || self.category.is_empty()
            || self.category.len() > 100
            || self.category.chars().any(char::is_control)
            || self.account_id.as_ref().is_some_and(|id| id.len() > 220)
            || chrono::DateTime::parse_from_rfc3339(&self.created_at).is_err()
        {
            return Err("Invalid category rule".into());
        }
        Ok(self)
    }
}

pub fn validate(rules: Vec<CategoryRule>) -> Result<Vec<CategoryRule>, String> {
    if rules.len() > MAX_RULES {
        return Err("Too many category rules".into());
    }
    let rules = rules
        .into_iter()
        .map(CategoryRule::normalized)
        .collect::<Result<Vec<_>, _>>()?;
    let mut ids = std::collections::BTreeSet::new();
    if rules.iter().any(|rule| !ids.insert(rule.id.as_str())) {
        return Err("Category rules must have unique IDs".into());
    }
    Ok(rules)
}

/// The most specific matching rule wins: an account-scoped exact rule, then any exact
/// rule, then the longest contained pattern. Ties keep the newest rule.
pub fn matching<'a>(
    rules: &'a [CategoryRule],
    merchant: &str,
    description: Option<&str>,
    account_id: Option<&str>,
) -> Option<&'a CategoryRule> {
    rules
        .iter()
        .filter(|rule| rule.matches(merchant, description, account_id))
        .max_by(|left, right| {
            let rank = |rule: &CategoryRule| {
                (
                    rule.match_kind == RuleMatch::Exact,
                    rule.account_id.is_some(),
                    rule.pattern.len(),
                )
            };
            rank(left)
                .cmp(&rank(right))
                .then_with(|| left.created_at.cmp(&right.created_at))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(
        id: &str,
        pattern: &str,
        kind: RuleMatch,
        account: Option<&str>,
        category: &str,
    ) -> CategoryRule {
        CategoryRule {
            id: id.into(),
            pattern: pattern.into(),
            match_kind: kind,
            account_id: account.map(str::to_owned),
            category: category.into(),
            created_at: "2026-09-01T00:00:00Z".into(),
        }
        .normalized()
        .unwrap()
    }

    #[test]
    fn normalizes_descriptors_and_matches_whole_words() {
        assert_eq!(
            normalize_merchant("  SQ *Cafe-Example 1234567 "),
            "sq cafe example"
        );
        let contains = rule("a", "Cafe", RuleMatch::Contains, None, "Dining");
        assert!(contains.matches("SQ *CAFE EXAMPLE", None, None));
        assert!(contains.matches("Square", Some("cafe purchase"), None));
        assert!(!contains.matches("Cafeteria", None, None));
        let exact = rule(
            "b",
            "Example Market",
            RuleMatch::Exact,
            Some("plaid:one"),
            "Groceries",
        );
        assert!(exact.matches("EXAMPLE MARKET", None, Some("plaid:one")));
        assert!(!exact.matches("EXAMPLE MARKET", None, Some("plaid:two")));
        assert!(!exact.matches("Example Market East", None, Some("plaid:one")));
    }

    #[test]
    fn most_specific_rule_wins_and_invalid_rules_are_rejected() {
        let rules = vec![
            rule("broad", "market", RuleMatch::Contains, None, "Shopping"),
            rule(
                "exact",
                "example market",
                RuleMatch::Exact,
                None,
                "Groceries",
            ),
            rule(
                "scoped",
                "example market",
                RuleMatch::Exact,
                Some("plaid:one"),
                "Household",
            ),
        ];
        let pick =
            |account| matching(&rules, "Example Market", None, account).map(|r| r.id.as_str());
        assert_eq!(pick(Some("plaid:one")), Some("scoped"));
        assert_eq!(pick(Some("plaid:two")), Some("exact"));
        assert_eq!(
            matching(&rules, "Corner Market", None, None).map(|r| r.id.as_str()),
            Some("broad")
        );
        assert!(validate(vec![rules[0].clone(), rules[0].clone()]).is_err());
        let mut invalid = rules[0].clone();
        invalid.pattern = "***".into();
        assert!(invalid.normalized().is_err());
    }
}

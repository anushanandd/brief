//! The brand a benefit names, which the renderer asks Logo.dev for by name: the literal words of
//! the rule's purchase pattern, then its name, without generic words such as "credit".
use super::CardRule;

const GENERIC: &[&str] = &[
    "a",
    "airline",
    "airlines",
    "an",
    "and",
    "annual",
    "at",
    "benefit",
    "bonus",
    "card",
    "cash",
    "credit",
    "credits",
    "delivery",
    "digital",
    "dining",
    "eligible",
    "entertainment",
    "fee",
    "fitness",
    "flight",
    "flights",
    "food",
    "for",
    "gas",
    "groceries",
    "grocery",
    "gym",
    "hotel",
    "hotels",
    "in",
    "lounge",
    "membership",
    "monthly",
    "of",
    "on",
    "online",
    "perk",
    "plus",
    "purchase",
    "purchases",
    "quarterly",
    "restaurant",
    "restaurants",
    "reward",
    "rewards",
    "ride",
    "rides",
    "rideshare",
    "ring",
    "security",
    "select",
    "semiannual",
    "shopping",
    "statement",
    "store",
    "stores",
    "streaming",
    "subscription",
    "subscriptions",
    "supermarket",
    "supermarkets",
    "the",
    "travel",
    "wellness",
    "with",
    "yearly",
];

/// Lowercase words without punctuation or generic words, or nothing when no word is left.
pub fn brand_key(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let words = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !GENERIC.contains(word))
        .collect::<Vec<_>>();
    words
        .iter()
        .any(|word| word.chars().any(char::is_alphabetic))
        .then(|| words.join(" "))
}

/// The rule's brand words in order: its pattern's literal alternatives, then its name.
pub(super) fn words(rule: &CardRule) -> Vec<String> {
    let mut words = Vec::new();
    for text in literal_alternatives(&rule.matches)
        .into_iter()
        .chain(std::iter::once(rule.name.clone()))
    {
        if let Some(key) = brand_key(&text) {
            if !words.contains(&key) {
                words.push(key);
            }
        }
    }
    words
}

pub(super) fn brand(rule: &CardRule) -> Option<String> {
    words(rule).into_iter().next()
}

/// The plain-text alternatives of a pattern such as `\b(?:tables|bistro)\b`; an alternative that
/// uses any other regex syntax is skipped.
fn literal_alternatives(pattern: &str) -> Vec<String> {
    let pattern = pattern.trim().trim_start_matches("(?i)");
    let pattern = pattern.replace(r"\b", "");
    split_top(unwrap_group(&pattern), '|')
        .into_iter()
        .filter_map(|alternative| literal(unwrap_group(alternative)))
        .collect()
}

/// The inside of a pattern that is one group, as many times as it is.
fn unwrap_group(mut pattern: &str) -> &str {
    loop {
        let inner = pattern
            .strip_prefix("(?:")
            .or_else(|| pattern.strip_prefix('('))
            .and_then(|rest| rest.strip_suffix(')'));
        match inner {
            Some(inner) if balanced(inner) => pattern = inner,
            _ => return pattern,
        }
    }
}

fn balanced(pattern: &str) -> bool {
    let mut depth = 0i32;
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Splits at `separator` outside groups and escapes.
fn split_top(pattern: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start, mut escaped) = (0i32, 0, false);
    for (index, c) in pattern.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == separator && depth == 0 {
            parts.push(&pattern[start..index]);
            start = index + c.len_utf8();
        } else if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
        }
    }
    parts.push(&pattern[start..]);
    parts
}

fn literal(alternative: &str) -> Option<String> {
    let alternative = alternative.trim_start_matches('^').trim_end_matches('$');
    let mut text = String::new();
    let mut chars = alternative.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                's' => {
                    chars.next_if(|next| matches!(next, '+' | '*' | '?'));
                    text.push(' ');
                }
                escaped if escaped.is_alphanumeric() => return None,
                escaped => text.push(escaped),
            },
            '(' | ')' | '[' | ']' | '{' | '}' | '?' | '*' | '+' | '|' | '.' | '^' | '$' => {
                return None
            }
            c => text.push(c),
        }
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(name: &str, matches: &str) -> CardRule {
        serde_json::from_value(serde_json::json!({
            "id": "perk", "name": name, "limit": "$10 / month", "cap": 10.0,
            "cadence": "monthly", "matches": matches, "instruction": "", "url": "https://example.com"
        }))
        .unwrap()
    }

    #[test]
    fn brand_words_come_from_the_pattern_then_the_name() {
        assert_eq!(
            words(&rule(
                "Examplebrand Cash",
                r"\b(?:examplebrand|other\ brand)\b"
            )),
            ["examplebrand", "other brand"]
        );
        assert_eq!(
            words(&rule(
                "Example Dining credit",
                r"examplebrand\s+eats|^tables$|bistro.*"
            )),
            ["examplebrand eats", "tables", "example"]
        );
        // A catalog benefit without keywords matches its escaped name.
        assert_eq!(
            words(&rule("Examplebrand+ credit", r"examplebrand\+ credit")),
            ["examplebrand"]
        );
        assert_eq!(
            words(&rule("Dining credit", r"dining|\d+")),
            Vec::<String>::new()
        );
        assert_eq!(
            words(&rule(
                "Example Thing",
                r"(?:a|b)(?:c|d)|(?i)\bexamplebrand\b"
            )),
            ["example thing"]
        );
        assert_eq!(
            brand(&rule("Examplebrand", "examplebrand|other brand")),
            Some("examplebrand".into())
        );
        assert_eq!(brand(&rule("Dining credit", "dining")), None);
    }
}

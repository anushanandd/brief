//! Checks that every figure an answer states came from the queries behind it. The model is told
//! to quote tool results exactly; this makes that a checked property instead of a request.
use std::{collections::BTreeSet, sync::LazyLock};

use regex::Regex;
use serde_json::Value;

use super::ToolCall;

/// The most figures an answer lists as unverified.
const MAX_UNVERIFIED: usize = 5;

/// A figure as written: an optional sign and dollar sign, digits with optional thousands
/// separators and decimals, and an optional percent sign or thousands suffix.
static FIGURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?P<sign>[-−])?(?P<dollar>\$)?(?P<number>\d{1,3}(?:,\d{3})+(?:\.\d+)?|\d+(?:\.\d+)?)(?P<suffix>%|k\b|m\b)?")
        .expect("valid figure pattern")
});

/// Dates the answer may write, whose numbers are days and years rather than amounts.
static DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b\d{4}-\d{2}-\d{2}\b|\b(?:jan|feb|mar|apr|may|jun|jul|aug|sep|sept|oct|nov|dec)[a-z]*\.?\s+\d{1,2}(?:st|nd|rd|th)?(?:,?\s+\d{4})?\b|\b(?:jan|feb|mar|apr|may|jun|jul|aug|sep|sept|oct|nov|dec)[a-z]*\.?\s+\d{4}\b|\b\d{1,2}/\d{1,2}(?:/\d{2,4})?\b",
    )
    .expect("valid date pattern")
});

/// The figures in `text` that no query returned, as written, at most five.
///
/// A figure matches a returned number exactly, as a whole-dollar rounding of it, or as a ratio
/// shown in percent. Plain whole numbers up to 31 are counts and days, which tool arguments and
/// list lengths already explain, so they are not checked; neither are years and the numbers
/// inside dates.
pub fn unverified(text: &str, evidence: &[ToolCall]) -> Vec<String> {
    let known = known_figures(evidence);
    let mut masked = text.to_owned();
    for date in DATE.find_iter(text) {
        masked.replace_range(date.range(), &" ".repeat(date.len()));
    }
    let mut missing = Vec::<String>::new();
    for capture in FIGURE.captures_iter(&masked) {
        let whole = capture.get(0).expect("match").as_str().trim();
        let dollar = capture.name("dollar").is_some();
        let suffix = capture
            .name("suffix")
            .map(|suffix| suffix.as_str().to_ascii_lowercase());
        let digits = capture["number"].replace(',', "");
        let Ok(value) = digits.parse::<f64>() else {
            continue;
        };
        let plain = !dollar && suffix.is_none() && !digits.contains('.');
        if plain && (value <= 31.0 || (1900.0..=2100.0).contains(&value)) {
            continue;
        }
        let value = match suffix.as_deref() {
            Some("k") => value * 1_000.0,
            Some("m") => value * 1_000_000.0,
            _ => value,
        };
        if known.contains(&cents(value)) {
            continue;
        }
        let shown = whole.to_owned();
        if !missing.contains(&shown) {
            missing.push(shown);
        }
        if missing.len() == MAX_UNVERIFIED {
            break;
        }
    }
    missing
}

/// Every figure the queries returned or were asked with, in whole cents of its absolute value,
/// with the whole-dollar rounding of amounts and the percent form of ratios.
fn known_figures(evidence: &[ToolCall]) -> BTreeSet<i64> {
    let mut known = BTreeSet::new();
    for call in evidence {
        collect(&call.arguments, &mut known);
        collect(&call.result, &mut known);
    }
    known
}

fn collect(value: &Value, known: &mut BTreeSet<i64>) {
    match value {
        Value::Number(number) => {
            if let Some(number) = number.as_f64() {
                add(number, known);
            }
        }
        Value::String(text) => {
            for capture in FIGURE.captures_iter(text) {
                if let Ok(number) = capture["number"].replace(',', "").parse::<f64>() {
                    add(number, known);
                }
            }
        }
        Value::Array(rows) => {
            known.insert(cents(rows.len() as f64));
            for row in rows {
                collect(row, known);
            }
        }
        Value::Object(fields) => {
            for field in fields.values() {
                collect(field, known);
            }
        }
        Value::Bool(_) | Value::Null => {}
    }
}

fn add(number: f64, known: &mut BTreeSet<i64>) {
    known.insert(cents(number));
    known.insert(cents(number.round()));
    known.insert(cents(number * 100.0));
    known.insert(cents((number * 1_000.0).round() / 10.0));
}

/// The absolute value in whole cents.
fn cents(value: f64) -> i64 {
    (value.abs() * 100.0).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(result: Value) -> ToolCall {
        ToolCall {
            name: "cashFlow".into(),
            arguments: json!({ "start": "2026-08-01", "end": "2026-08-31" }),
            result,
        }
    }

    #[test]
    fn quoted_figures_pass_and_invented_ones_are_listed() {
        let evidence = [call(json!({
            "spending": -1234.56,
            "income": 4200.0,
            "savingsRate": 0.123,
            "categories": [{ "name": "Groceries", "total": "$310.40" }],
        }))];
        assert!(unverified(
            "You spent $1,234.56 and earned $4,200 in August 2026, saving 12.3%. \
             Groceries were $310.40, about $1,235 in all.",
            &evidence,
        )
        .is_empty());
        assert_eq!(
            unverified("You spent $1,300 and saved $2.1k.", &evidence),
            vec!["$1,300", "$2.1k"]
        );
    }

    #[test]
    fn dates_counts_and_years_are_not_amounts() {
        let evidence = [call(json!({ "total": 50.0 }))];
        assert!(unverified(
            "Between 2026-08-01 and Aug 31, 2026 you had 3 charges totaling $50 on 8/14.",
            &evidence,
        )
        .is_empty());
        assert_eq!(unverified("That was 412 charges.", &evidence), vec!["412"]);
    }

    #[test]
    fn an_answer_without_queries_has_every_amount_unverified() {
        assert_eq!(unverified("Rent is $2,400.", &[]), vec!["$2,400"]);
        assert!(unverified("No saved data covers that.", &[]).is_empty());
    }
}

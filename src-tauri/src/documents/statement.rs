//! Bank statements issued as PDFs. A statement lists its period, its beginning and ending
//! balances, and each transaction on a line that starts with its day and ends with its amount
//! and the running balance. The rows become a CSV the statement importer already reads, and are
//! offered only when beginning balance plus every row equals the ending balance.
use std::sync::LazyLock;

use chrono::{Datelike, NaiveDate};
use regex::Regex;

use super::read::{amounts, row_text, rows, Reading};
use super::{Check, Field, Kind, Read};

static PERIOD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(january|february|march|april|may|june|july|august|september|october|november|december)\s+(\d{1,2}),\s*(\d{4})\s*(?:through|to|-|–)\s*(january|february|march|april|may|june|july|august|september|october|november|december)\s+(\d{1,2}),\s*(\d{4})").expect("valid")
});
static ACCOUNT: LazyLock<Regex> = LazyLock::new(|| {
    // On the label's own line: a label that ends its line can sit above a mailing barcode.
    Regex::new(r"(?i)account[ \t]+(?:number|no\.?)[ \t]*:?[ \t]*([0-9xX*\-][0-9xX*\- ]{3,})")
        .expect("valid")
});
static ROW: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{2})/(\d{2})\s+(.*)$").expect("valid"));
static DAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{2}/\d{2}$").expect("valid"));
/// The detail's heading, alone on its line ("TRANSACTION DETAIL (continued)"); a statement may
/// also carry the words in hidden markers ("*end*transaction detail").
static DETAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^transaction detail\b").expect("valid"));
/// A description wraps onto the line directly below its row: no further than this many of its
/// line heights below the row's bottom, and printed at least this share of the row's height
/// (a statement's hidden markers are a sliver of its text's height).
const WRAP_LINES: f64 = 1.5;
const WRAP_HEIGHT: f64 = 0.5;

fn month(name: &str) -> Option<u32> {
    [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ]
    .iter()
    .position(|candidate| candidate.eq_ignore_ascii_case(name))
    .and_then(|index| u32::try_from(index + 1).ok())
}

fn day(found: &regex::Captures<'_>, offset: usize) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(
        found[offset + 2].parse().ok()?,
        month(&found[offset])?,
        found[offset + 1].parse().ok()?,
    )
}

fn balance(text: &str, label: &str) -> Option<f64> {
    text.lines()
        .find(|line| line.to_lowercase().contains(label))
        .and_then(|line| amounts(line).into_iter().last())
}

fn csv_field(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}

pub(super) fn read(reading: &Reading) -> Option<Read> {
    let text = reading.text();
    let lower = text.to_lowercase();
    if !(lower.contains("beginning balance") && lower.contains("ending balance")) {
        return None;
    }
    let period = PERIOD.captures(&text)?;
    let (start, end) = (day(&period, 1)?, day(&period, 4)?);
    let opening = balance(&text, "beginning balance");
    let closing = balance(&text, "ending balance");
    let mask = ACCOUNT.captures(&text).and_then(|found| {
        let digits = found[1]
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>();
        (digits.len() >= 4).then(|| digits[digits.len() - 4..].to_owned())
    });
    let issuer = text
        .lines()
        .map(str::trim)
        .find(|line| {
            let lower = line.to_lowercase();
            lower.contains("bank") && lower.len() < 60 && !lower.contains("balance")
        })
        .map(str::to_owned);

    // Rows from the transaction detail: a day, a description that may wrap to the next line,
    // and the amount (with the balance after it, when the statement prints one).
    let mut entries: Vec<(NaiveDate, String, f64)> = Vec::new();
    let mut detail = false;
    // The page, bottom edge, and height of the last line a row's description reached, which
    // only a line of its size directly below it can continue.
    let mut wraps: Option<(usize, f64, f64)> = None;
    for (page_index, page) in reading.pages.iter().enumerate() {
        for row in rows(&page.lines) {
            let top = row.iter().map(|line| line.y).fold(f64::INFINITY, f64::min);
            let bottom = row
                .iter()
                .map(|line| line.y + line.h)
                .fold(f64::NEG_INFINITY, f64::max);
            let height = bottom - top;
            let below = wraps.take().filter(|(page, edge, above)| {
                *page == page_index
                    && top - edge <= above * WRAP_LINES
                    && height >= above * WRAP_HEIGHT
            });
            // A description that wraps can start left of its day; the day leads the row.
            let mut words = row_text(&row)
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if let Some(index) = words.iter().position(|word| DAY.is_match(word)) {
                let day = words.remove(index);
                words.insert(0, day);
            }
            let line = words.join(" ");
            let lowered = line.to_lowercase();
            if DETAIL.is_match(&line) {
                detail = true;
                continue;
            }
            if !detail || lowered.contains("beginning balance") {
                continue;
            }
            if lowered.contains("ending balance") {
                detail = false;
                continue;
            }
            match ROW.captures(&line) {
                Some(found) => {
                    let figures = amounts(&found[3]);
                    let amount = match figures.len() {
                        0 => continue,
                        1 => figures[0],
                        _ => figures[figures.len() - 2],
                    };
                    let month: u32 = found[1].parse().unwrap_or(0);
                    let year = if month > end.month() {
                        start.year()
                    } else {
                        end.year()
                    };
                    let Some(date) =
                        NaiveDate::from_ymd_opt(year, month, found[2].parse().unwrap_or(0))
                    else {
                        continue;
                    };
                    let description = found[3]
                        .split_whitespace()
                        .filter(|word| super::read::money(word).is_none())
                        .collect::<Vec<_>>()
                        .join(" ");
                    entries.push((date, description, amount));
                    wraps = Some((page_index, bottom, height));
                }
                None if below.is_some() && amounts(&line).is_empty() && line.len() < 120 => {
                    if let Some(last) = entries.last_mut() {
                        last.1 = format!("{} {}", last.1, line);
                        wraps = Some((page_index, bottom, height));
                    }
                }
                None => {}
            }
        }
    }
    let mut checks = Vec::new();
    let balanced = match (opening, closing) {
        (Some(opening), Some(closing)) => {
            let total = entries
                .iter()
                .map(|(_, _, amount)| crate::numeric::minor_units(*amount))
                .sum::<i64>();
            let passed = crate::numeric::minor_units(opening) + total
                == crate::numeric::minor_units(closing);
            checks.push(Check {
                label: "Beginning balance plus every row equals the ending balance".into(),
                passed,
            });
            passed
        }
        _ => false,
    };
    let mut fields = Vec::new();
    for (key, label, value) in [
        ("beginningBalance", "Beginning balance", opening),
        ("endingBalance", "Ending balance", closing),
    ] {
        if let Some(value) = value {
            fields.push(Field {
                key: key.into(),
                label: label.into(),
                amount: Some(value),
                ..Field::default()
            });
        }
    }
    let statement_csv = (balanced && !entries.is_empty()).then(|| {
        let mut csv = String::from("Date,Description,Amount\n");
        for (date, description, amount) in &entries {
            csv.push_str(&format!(
                "{},{},{amount:.2}\n",
                date.format("%m/%d/%Y"),
                csv_field(description)
            ));
        }
        (csv, entries.len())
    });
    Some(Read {
        kind: Some(Kind::Statement),
        form: Some("Statement".into()),
        issuer,
        tax_year: Some(end.year()),
        date: Some(end.to_string()),
        period_start: Some(start.to_string()),
        period_end: Some(end.to_string()),
        mask,
        fields,
        checks,
        statement_csv,
        ..Read::default()
    })
}

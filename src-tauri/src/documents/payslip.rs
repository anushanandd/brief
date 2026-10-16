//! Pay statements. Each line is a label with this period's amount and the year's, told apart by
//! the columns the statement heads them with; a section heading such as "Tax Deductions:
//! California" says whose withholding a line is. Taxes and deductions are kept as what was taken
//! out, so a refund reads negative; other lines keep the statement's sign. The reading checks
//! itself: gross less what was taken, plus the other lines, must equal net pay.
use std::{collections::BTreeMap, sync::LazyLock};

use regex::Regex;
use serde::Deserialize;

use super::read::{date, masked, money, rows, words, Line, Reading};
use super::{Check, Field, Kind, PayDeposit, Read};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    key: String,
    label: String,
    kind: String,
    #[serde(rename = "match")]
    names: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vocabulary {
    titles: Vec<String>,
    federal_sections: Vec<String>,
    state_sections: Vec<String>,
    deduction_sections: Vec<String>,
    earnings_sections: Vec<String>,
    current_columns: Vec<String>,
    ytd_columns: Vec<String>,
    lines: Vec<Entry>,
}

static VOCABULARY: LazyLock<Vocabulary> = LazyLock::new(|| {
    serde_json::from_str(include_str!("payslip.json")).expect("pay statement vocabulary is valid")
});

pub(super) fn is_payslip(text: &str) -> bool {
    VOCABULARY
        .titles
        .iter()
        .any(|title| text.contains(title.as_str()))
        || (text.contains("net pay") && text.contains("gross"))
}

/// Where a column's name ends in the header row, the way amounts align under it.
fn column(row: &[&Line], names: &[String]) -> Option<f64> {
    row.iter().find_map(|line| {
        let lower = line.text.to_lowercase();
        names.iter().find_map(|name| {
            let start = lower.find(name.as_str())?;
            let end = lower[..start + name.len()].chars().count() as f64;
            Some(line.x + line.w * end / lower.chars().count().max(1) as f64)
        })
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Section {
    None,
    Earnings,
    Federal,
    State,
    Deductions,
}

static COMPANY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(inc|llc|l\.l\.c|corp|corporation|company|university|n\.a)\.?$")
        .expect("valid")
});

fn entry(label: &str) -> Option<&'static Entry> {
    // An adjustment to net pay is its own line, not net pay.
    if label.contains("adj") {
        return None;
    }
    VOCABULARY
        .lines
        .iter()
        .find(|entry| entry.names.iter().any(|name| label.contains(name.as_str())))
}

#[derive(Default)]
struct Amounts {
    label: String,
    kind: String,
    current: Option<f64>,
    ytd: Option<f64>,
    uncertain: bool,
}

pub(super) fn read(reading: &Reading) -> Read {
    let mut read = Read {
        kind: Some(Kind::Payslip),
        form: Some("Pay statement".into()),
        ..Read::default()
    };
    let mut lines = BTreeMap::<String, Amounts>::new();
    let mut seen = Vec::<(String, Option<i64>, Option<i64>)>::new();
    let mut signed = false;
    for page in &reading.pages {
        let rows = rows(&page.lines);
        let header = rows
            .iter()
            .find(|row| column(row, &VOCABULARY.ytd_columns).is_some());
        let ytd_x = header.and_then(|row| column(row, &VOCABULARY.ytd_columns));
        let current_x = header.and_then(|row| {
            row.iter()
                .filter(|line| line.x < ytd_x.unwrap_or(f64::MAX))
                .filter_map(|line| column(&[*line], &VOCABULARY.current_columns))
                .rfind(|edge| *edge < ytd_x.unwrap_or(f64::MAX) - 0.01)
        });
        let ignored = header
            .map(|row| {
                ["rate", "hours", "units"]
                    .iter()
                    .filter_map(|name| column(row, &[(*name).to_owned()]))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut section = Section::None;
        let mut depositing = false;
        for row in &rows {
            let joined = row
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let lower = joined.to_lowercase();
            let mut label = Vec::new();
            let mut figures = Vec::new();
            for line in row {
                for (word, right) in words(line) {
                    match money(word) {
                        Some(value) => figures.push((value, right)),
                        // The label is what precedes the figures; words after them belong to
                        // another panel of the statement.
                        None if figures.is_empty() => label.push(word),
                        None => {}
                    }
                }
            }
            if lower.contains("check date") || lower.contains("pay date") {
                read.date = read
                    .date
                    .take()
                    .or(date(&joined).map(|day| day.to_string()));
            }
            if lower.contains("period beginning") || lower.contains("period start") {
                read.period_start = read
                    .period_start
                    .take()
                    .or(date(&joined).map(|day| day.to_string()));
            }
            if lower.contains("period ending") || lower.contains("period end") {
                read.period_end = read
                    .period_end
                    .take()
                    .or(date(&joined).map(|day| day.to_string()));
            }
            if read.issuer.is_none() {
                read.issuer = row
                    .iter()
                    .map(|line| line.text.trim())
                    .find(|text| COMPANY.is_match(text))
                    .map(|text| text.trim_end_matches('.').to_owned());
            }
            if lower.contains("deposited to") || lower.contains("direct deposit") {
                depositing = true;
                continue;
            }
            if depositing {
                if let (Some(mask), Some((amount, _))) = (masked(&joined), figures.last()) {
                    let deposit = PayDeposit {
                        mask: Some(mask),
                        amount: *amount,
                    };
                    if !read.deposits.contains(&deposit) {
                        read.deposits.push(deposit);
                    }
                }
                continue;
            }
            if figures.is_empty() {
                if VOCABULARY
                    .state_sections
                    .iter()
                    .any(|name| lower.contains(name.as_str()))
                {
                    section = Section::State;
                } else if VOCABULARY
                    .federal_sections
                    .iter()
                    .any(|name| lower.contains(name.as_str()))
                {
                    section = Section::Federal;
                } else if VOCABULARY
                    .deduction_sections
                    .iter()
                    .any(|name| lower.contains(name.as_str()))
                {
                    section = Section::Deductions;
                } else if VOCABULARY
                    .earnings_sections
                    .iter()
                    .any(|name| lower.contains(name.as_str()))
                {
                    section = Section::Earnings;
                }
                continue;
            }
            let label = label
                .join(" ")
                .trim_matches(|c: char| c == '*' || c == '-' || c.is_whitespace())
                .to_owned();
            if label.is_empty() || label.chars().filter(|c| c.is_alphabetic()).count() < 3 {
                continue;
            }
            // Each figure to the column it aligns under; one far from every column (another
            // panel's amount) is left out.
            let mut current = None;
            let mut ytd = None;
            let mut uncertain = false;
            for (value, right) in &figures {
                let mut columns = vec![];
                if let Some(x) = current_x {
                    columns.push((x, 1));
                }
                if let Some(x) = ytd_x {
                    columns.push((x, 2));
                }
                columns.extend(ignored.iter().map(|x| (*x, 0)));
                match columns.iter().min_by(|left, right_| {
                    (left.0 - right).abs().total_cmp(&(right_.0 - right).abs())
                }) {
                    Some((x, column)) if (x - right).abs() <= 0.08 => match column {
                        1 => current = Some(*value),
                        2 => ytd = Some(*value),
                        _ => {}
                    },
                    Some(_) => {}
                    None => match figures.len() {
                        1 => {
                            current = Some(*value);
                            uncertain = true;
                        }
                        _ if current.is_none() => current = Some(*value),
                        _ => ytd = Some(*value),
                    },
                }
            }
            if current.is_none() && ytd.is_none() {
                continue;
            }
            let lowered = label.to_lowercase();
            let (key, name, kind) = match entry(&lowered) {
                Some(entry) if entry.key == "withholding" => match section {
                    Section::State => (
                        "stateWithholding".to_owned(),
                        "State income tax".to_owned(),
                        "tax",
                    ),
                    Section::Federal => (
                        "federalWithholding".to_owned(),
                        "Federal income tax".to_owned(),
                        "tax",
                    ),
                    _ => ("withholding".to_owned(), label.clone(), "tax"),
                },
                Some(entry) => (entry.key.clone(), entry.label.clone(), entry.kind.as_str()),
                None if section == Section::Earnings => {
                    (format!("earnings:{label}"), label.clone(), "earnings")
                }
                None => (format!("other:{label}"), label.clone(), "other"),
            };
            let fingerprint = (
                key.clone(),
                current.map(crate::numeric::minor_units),
                ytd.map(crate::numeric::minor_units),
            );
            // A second page that repeats the first adds nothing.
            if seen.contains(&fingerprint) {
                continue;
            }
            seen.push(fingerprint);
            if matches!(kind, "tax" | "deduction")
                && [current, ytd]
                    .into_iter()
                    .flatten()
                    .any(|value| value < 0.0)
            {
                signed = true;
            }
            let amounts = lines.entry(key).or_insert_with(|| Amounts {
                label: name,
                kind: kind.to_owned(),
                ..Amounts::default()
            });
            let add = |total: Option<f64>, value: Option<f64>| match (total, value) {
                (Some(total), Some(value)) => Some(crate::numeric::round2(total + value)),
                (total, value) => total.or(value),
            };
            amounts.current = add(amounts.current, current);
            amounts.ytd = add(amounts.ytd, ytd);
            amounts.uncertain |= uncertain || row.iter().any(|line| line.confidence < 0.9);
        }
    }
    // Taxes and deductions as what was taken out. A statement that prints them negative marks
    // a refund by leaving the minus off; one that prints them all positive took every one.
    let taken = |value: f64| if signed { -value } else { value.abs() };
    for (key, amounts) in &lines {
        let deducted = matches!(amounts.kind.as_str(), "tax" | "deduction");
        read.fields.push(Field {
            key: key.clone(),
            label: amounts.label.clone(),
            amount: amounts
                .current
                .map(|value| if deducted { taken(value) } else { value }),
            ytd: amounts
                .ytd
                .map(|value| if deducted { taken(value) } else { value }),
            uncertain: amounts.uncertain,
            ..Field::default()
        });
    }
    read.tax_year = read
        .date
        .as_deref()
        .and_then(|day| day.get(..4))
        .and_then(|year| year.parse().ok());
    read.mask = read
        .deposits
        .first()
        .and_then(|deposit| deposit.mask.clone());
    for (column, label) in [
        (
            false,
            "This period: gross less taxes and deductions equals net pay",
        ),
        (
            true,
            "Year to date: gross less taxes and deductions equals net pay",
        ),
    ] {
        if let Some(passed) = balances(&read.fields, &lines, column) {
            read.checks.push(Check {
                label: label.into(),
                passed,
            });
        }
    }
    read
}

/// Whether gross less what was taken, plus every other line, equals net pay.
fn balances(fields: &[Field], lines: &BTreeMap<String, Amounts>, ytd: bool) -> Option<bool> {
    let value = |field: &Field| if ytd { field.ytd } else { field.amount };
    let find = |key: &str| fields.iter().find(|field| field.key == key).and_then(value);
    let (gross, net) = (find("gross")?, find("net")?);
    let mut total = crate::numeric::minor_units(gross);
    for field in fields {
        let kind = lines.get(&field.key).map(|amounts| amounts.kind.as_str());
        let Some(amount) = value(field) else { continue };
        match kind {
            Some("tax" | "deduction") => total -= crate::numeric::minor_units(amount),
            Some("other") => total += crate::numeric::minor_units(amount),
            _ => {}
        }
    }
    Some(total == crate::numeric::minor_units(net))
}

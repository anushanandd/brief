//! Information returns, read box by box. Each box's figure sits beside or just below its label,
//! so the reader finds the label and takes the nearest amount there. A printout often carries
//! several copies of the form; a box is certain only when every copy reads alike.
use std::{collections::BTreeMap, sync::LazyLock};

use regex::Regex;
use serde::Deserialize;

use super::read::{money, printed_year, row_text, rows, words, Line, Reading};
use super::{Field, Kind, Read};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Box {
    key: String,
    label: String,
    patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FormSpec {
    form: String,
    titles: Vec<String>,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    corrected: Vec<String>,
    #[serde(default)]
    issuer_labels: Vec<String>,
    boxes: Vec<Box>,
}

#[derive(Debug, Deserialize)]
struct Forms {
    forms: Vec<FormSpec>,
    /// A brokerage's combined statement names several of these.
    composite: Vec<String>,
}

static FORMS: LazyLock<Forms> = LazyLock::new(|| {
    serde_json::from_str(include_str!("forms.json")).expect("form definitions are valid")
});

/// Which form `text` (lower case) is: a composite 1099 first, since it names several.
pub(super) fn identify(text: &str) -> Option<Vec<&'static FormSpec>> {
    let named = FORMS
        .composite
        .iter()
        .filter(|form| text.contains(form.as_str()))
        .count();
    if named >= 2 {
        return Some(
            FORMS
                .forms
                .iter()
                .filter(|spec| spec.form.starts_with("1099-") && spec.form != "1099-NEC")
                .collect(),
        );
    }
    FORMS
        .forms
        .iter()
        .find(|spec| {
            spec.titles
                .iter()
                .any(|title| text.contains(title.as_str()))
                && spec
                    .requires
                    .iter()
                    .all(|word| text.contains(word.as_str()))
        })
        .map(|spec| vec![spec])
}

fn lowered(line: &Line) -> String {
    line.text.to_lowercase().replace(['’', '`'], "'")
}

/// Where each box's label sits. A reading may merge neighbouring labels into one line ("1 Wages,
/// tips, other compensation 2 Federal income tax withheld"), so each label is cut out of its
/// line at its own words, which also gives the start of the next box.
fn labels<'a>(lines: &[Line], specs: &[&'a FormSpec]) -> Vec<(&'a str, Line)> {
    let mut found = Vec::new();
    for line in lines {
        let text = lowered(line);
        // The copy's own heading ("City or Local Income Tax Return") names boxes it isn't.
        if text.contains("tax return") || text.contains("furnished") {
            continue;
        }
        let characters = text.chars().count().max(1) as f64;
        let mut starts = specs
            .iter()
            .flat_map(|spec| &spec.boxes)
            .filter_map(|item| {
                item.patterns
                    .iter()
                    .filter_map(|pattern| text.find(pattern.as_str()))
                    .min()
                    .map(|start| (start, item.key.as_str()))
            })
            .collect::<Vec<_>>();
        starts.sort();
        starts.dedup_by_key(|(start, _)| *start);
        for (index, (start, key)) in starts.iter().enumerate() {
            // Back up over the box number printed before the label.
            let number = text[..*start]
                .trim_end()
                .rfind(' ')
                .map_or(0, |space| space + 1);
            let begin = if text[number..*start]
                .trim()
                .chars()
                .all(|c| c.is_ascii_alphanumeric())
            {
                number
            } else {
                *start
            };
            let end = starts
                .get(index + 1)
                .map_or(characters, |(next, _)| text[..*next].chars().count() as f64);
            let from = text[..begin].chars().count() as f64;
            found.push((
                *key,
                Line {
                    text: line
                        .text
                        .chars()
                        .skip(from as usize)
                        .take((end - from) as usize)
                        .collect(),
                    x: line.x + line.w * from / characters,
                    w: line.w * (end - from) / characters,
                    ..line.clone()
                },
            ));
        }
    }
    found
}

/// An amount printed on the page and where it sits.
struct Figure {
    value: f64,
    right: f64,
    middle: f64,
}

/// Every amount on the page, word by word, so a line holding a label and its figure, or two
/// boxes' figures, gives each its own place.
fn figures(lines: &[Line]) -> Vec<Figure> {
    lines
        .iter()
        .flat_map(|line| {
            words(line).into_iter().filter_map(|(word, right)| {
                money(word).map(|value| Figure {
                    value,
                    right,
                    middle: line.middle(),
                })
            })
        })
        .collect()
}

/// Which figure each label's box holds, as `labels[i] -> figures[j]`. A box's sides are its
/// neighbours: the next label on its row, and the labels above and below it in its column.
/// Most forms print a box's figure under its label, some above it (a W-2's state row often
/// does), so figures go first to the label they sit under, nearest first, and only then to one
/// they sit over; each figure belongs to one box. A label alone on its row, as in a summary
/// table, takes the figure at the end of the row.
fn assign(lines: &[Line], labels: &[(&str, Line)]) -> Vec<Option<f64>> {
    let figures = figures(lines);
    struct Bounds {
        right: f64,
        top: f64,
        bottom: f64,
        alone: bool,
    }
    let bounds = labels
        .iter()
        .map(|(_, label)| {
            let tolerance = label.h.max(0.006);
            let depth = (label.h * 2.6).max(0.012);
            let column = |other: &&Line| other.x < label.x + 0.05 && other.right() > label.x - 0.02;
            let others = labels.iter().map(|(_, other)| other);
            let beside = others
                .clone()
                .filter(|other| {
                    other.x > label.x + 0.01 && (other.middle() - label.middle()).abs() < tolerance
                })
                .map(|other| other.x)
                .reduce(f64::min);
            Bounds {
                right: beside.unwrap_or(label.x + label.w.max(0.12) + 0.05),
                bottom: others
                    .clone()
                    .filter(|other| other.middle() > label.middle() + tolerance)
                    .filter(column)
                    .map(|other| other.y)
                    .fold(label.y + depth, f64::min),
                top: others
                    .filter(|other| other.middle() < label.middle() - tolerance)
                    .filter(column)
                    .map(|other| other.y + other.h)
                    .fold(label.y - depth, f64::max),
                alone: beside.is_none(),
            }
        })
        .collect::<Vec<_>>();
    let within = |label: &Line, bounds: &Bounds, figure: &Figure| {
        figure.right > label.x + 0.005 && figure.right <= bounds.right + 0.01
    };
    let mut taken = vec![false; figures.len()];
    let mut found = vec![None; labels.len()];
    // Under the label (or in its own line, beside its words), then over it.
    for above in [false, true] {
        let mut pairs = Vec::new();
        for (index, ((_, label), bounds)) in labels.iter().zip(&bounds).enumerate() {
            if found[index].is_some() {
                continue;
            }
            for (figure_index, figure) in figures.iter().enumerate() {
                let placed = if above {
                    figure.middle < label.middle() && figure.middle > bounds.top - 0.002
                } else {
                    figure.middle >= label.middle() - 0.002 && figure.middle < bounds.bottom + 0.002
                };
                if placed && within(label, bounds, figure) {
                    let distance = (figure.middle - label.middle()).abs()
                        + (figure.right - label.right()).abs() * 0.1;
                    pairs.push((distance, index, figure_index));
                }
            }
        }
        pairs.sort_by(|left, right| left.0.total_cmp(&right.0));
        for (_, index, figure_index) in pairs {
            if found[index].is_none() && !taken[figure_index] {
                found[index] = Some(figures[figure_index].value);
                taken[figure_index] = true;
            }
        }
    }
    for (index, ((_, label), bounds)) in labels.iter().zip(&bounds).enumerate() {
        if found[index].is_some() || !bounds.alone {
            continue;
        }
        found[index] = figures
            .iter()
            .enumerate()
            .filter(|(figure_index, figure)| {
                !taken[*figure_index]
                    && figure.right > label.right()
                    && (figure.middle - label.middle()).abs() < label.h.max(0.006)
            })
            .min_by(|(_, left), (_, right)| left.right.total_cmp(&right.right))
            .map(|(_, figure)| figure.value);
    }
    found
}

static OTHER_ITEM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:([A-Za-z][A-Za-z .&/-]{1,24}?)\s+\$?(\d[\d,]*\.\d{2})|\$?(\d[\d,]*\.\d{2})\s+([A-Za-z][A-Za-z .&/-]{1,24}))$")
        .expect("valid")
});

/// W-2 box 14: what the employer reports beside the boxes, each a short name and an amount,
/// such as "171.78 CA SDI" or "FAMLI 4.76", in the lines under its label.
fn box14(lines: &[Line]) -> Vec<(String, f64)> {
    let mut found = Vec::new();
    // The label may share its line with box 13's ("13 Statutory employee 14 Other").
    for (label, start) in lines.iter().filter_map(|line| {
        let text = lowered(line);
        let at = text.find("14 other")?;
        let characters = text.chars().count().max(1) as f64;
        Some((
            line,
            line.x + line.w * text[..at].chars().count() as f64 / characters,
        ))
    }) {
        let region = lines
            .iter()
            .filter(|line| {
                line.middle() > label.middle()
                    && line.y - label.y <= 0.06
                    && line.x >= start - 0.02
                    && line.x <= start + 0.2
            })
            .cloned()
            .collect::<Vec<_>>();
        // A name and its amount can come back as two pieces of one row.
        for row in rows(&region) {
            if let Some(found_item) = OTHER_ITEM.captures(row_text(&row).trim()) {
                let (name, amount) = match (found_item.get(1), found_item.get(2)) {
                    (Some(name), Some(amount)) => (name.as_str(), amount.as_str()),
                    _ => (&found_item[4], &found_item[3]),
                };
                if let Some(amount) = money(amount) {
                    found.push((name.trim().to_uppercase(), amount));
                }
            }
        }
    }
    found
}

/// What most copies of a box agree on, counting a copy whose box is empty as a vote for empty,
/// and whether the box is in doubt: copies disagree, or too few copies show it at all.
fn consensus(votes: &[Option<f64>], copies: usize) -> Option<(f64, bool)> {
    let mut counts = BTreeMap::<i64, usize>::new();
    for value in votes.iter().flatten() {
        *counts
            .entry(crate::numeric::minor_units(*value))
            .or_default() += 1;
    }
    let empty = votes.iter().filter(|vote| vote.is_none()).count();
    // A copy that lost a digit ("056.00" for "1056.00") reads the same figure, so it supports
    // the fuller reading rather than disputing it.
    let lost_digit = |short: i64, full: i64| {
        let (short, full) = (short.abs().to_string(), full.abs().to_string());
        short.len() < full.len() && (full.ends_with(&short) || full.starts_with(&short))
    };
    let support = |cents: i64| {
        counts
            .iter()
            .filter(|(other, _)| **other == cents || lost_digit(**other, cents))
            .map(|(_, count)| count)
            .sum::<usize>()
    };
    let cents = *counts
        .keys()
        .max_by_key(|cents| (support(**cents), cents.abs()))?;
    let agreeing = support(cents);
    if empty > agreeing {
        return None;
    }
    // A copy that reads differently disputes the figure, and so does a printout where only one
    // copy shows the box at all.
    let disputed = counts
        .keys()
        .any(|other| *other != cents && !lost_digit(*other, cents));
    let doubtful = disputed || (copies >= 2 && agreeing < 2);
    Some((crate::numeric::from_minor(cents), doubtful))
}

static BOX12: LazyLock<Regex> = LazyLock::new(|| {
    // The divider printed after the code often reads as `|`, `I`, or `l`: "CI 1.06" is C.
    Regex::new(r"\b(AA|BB|DD|EE|FF|GG|HH|II|[A-HJ-NP-TV-Z])[|Il]?\s*\|?\s*\$?\s*(\d[\d,]*\.\d{2})")
        .expect("valid")
});

/// W-2 box 12: a code and an amount below each of its four labels.
fn box12(lines: &[Line]) -> Vec<(String, f64)> {
    let mut found = Vec::new();
    for label in lines
        .iter()
        .filter(|line| lowered(line).trim_start().starts_with("12"))
    {
        let mut region = lines
            .iter()
            .filter(|line| {
                line.y >= label.y - 0.002
                    && line.y - label.y <= 0.035
                    && line.x >= label.x - 0.02
                    && line.x <= label.x + 0.2
            })
            .collect::<Vec<_>>();
        region.sort_by(|left, right| left.y.total_cmp(&right.y).then(left.x.total_cmp(&right.x)));
        let text = region
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        for capture in BOX12.captures_iter(&text) {
            if let Some(amount) = money(&capture[2]) {
                found.push((capture[1].to_owned(), amount));
            }
        }
    }
    found
}

/// The issuer's name: the first line of words below its label.
fn issuer(lines: &[Line], labels: &[String]) -> Option<String> {
    let mut names = Vec::new();
    for label in lines.iter().filter(|line| {
        labels
            .iter()
            .any(|name| lowered(line).contains(name.as_str()))
    }) {
        let mut below = lines
            .iter()
            .filter(|line| {
                line.y > label.y + 0.002
                    && line.y - label.y < 0.04
                    && line.x >= label.x - 0.03
                    && line.x <= label.x + 0.3
            })
            .collect::<Vec<_>>();
        below.sort_by(|left, right| left.y.total_cmp(&right.y).then(left.x.total_cmp(&right.x)));
        let Some(first) = below.first() else { continue };
        let name = below
            .iter()
            .filter(|line| (line.middle() - first.middle()).abs() < first.h.max(0.006))
            .map(|line| line.text.trim())
            .collect::<Vec<_>>()
            .join(" ");
        let letters = name.chars().filter(|c| c.is_alphabetic()).count();
        if letters >= 4 && letters * 2 > name.len() {
            names.push(name);
        }
    }
    names
        .into_iter()
        .max_by_key(|name| name.split_whitespace().count())
}

static STATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(A[KLRZ]|C[AOT]|D[CE]|FL|GA|HI|I[ADLN]|K[SY]|LA|M[ADEINOST]|N[CDEHJMVY]|O[HKR]|PA|RI|S[CD]|T[NX]|UT|V[AT]|W[AIVY])\b").expect("valid")
});

pub(super) fn read(reading: &Reading, specs: Vec<&'static FormSpec>) -> Read {
    let text = reading.text().to_lowercase();
    let mut fields = Vec::new();
    let mut issuer_name = None;
    let placed = reading
        .pages
        .iter()
        .map(|page| labels(&page.lines, &specs))
        .collect::<Vec<_>>();
    let assigned = reading
        .pages
        .iter()
        .zip(&placed)
        .map(|(page, labels)| assign(&page.lines, labels))
        .collect::<Vec<_>>();
    // How many copies the printout carries: the most labels any one box shows.
    let copies = placed
        .iter()
        .flatten()
        .fold(BTreeMap::<&str, usize>::new(), |mut counts, (key, _)| {
            *counts.entry(key).or_default() += 1;
            counts
        })
        .into_values()
        .max()
        .unwrap_or(1);
    for spec in &specs {
        for item in &spec.boxes {
            let mut votes = Vec::new();
            let mut doubtful = false;
            for ((labels, values), _) in placed.iter().zip(&assigned).zip(&reading.pages) {
                for ((key, label), value) in labels.iter().zip(values) {
                    if *key == item.key {
                        votes.push(*value);
                        doubtful |= label.confidence < 0.9;
                    }
                }
            }
            if let Some((amount, disagreed)) = consensus(&votes, copies) {
                let key = if specs.len() > 1 {
                    format!("{}:{}", spec.form, item.key)
                } else {
                    item.key.clone()
                };
                fields.push(Field {
                    key,
                    label: item.label.clone(),
                    amount: Some(amount),
                    uncertain: disagreed || doubtful,
                    ..Field::default()
                });
            }
        }
        issuer_name = issuer_name.or_else(|| {
            reading
                .pages
                .iter()
                .find_map(|page| issuer(&page.lines, &spec.issuer_labels))
        });
    }
    let form = if specs.len() > 1 {
        "1099".to_owned()
    } else {
        specs[0].form.clone()
    };
    let corrected = specs.iter().any(|spec| {
        spec.corrected
            .iter()
            .any(|word| text.contains(word.as_str()))
    });
    if form == "W-2" {
        let mut codes = BTreeMap::<String, Vec<f64>>::new();
        for page in &reading.pages {
            for (code, amount) in box12(&page.lines) {
                codes.entry(code).or_default().push(amount);
            }
        }
        for (code, values) in codes {
            let votes = values.into_iter().map(Some).collect::<Vec<_>>();
            if let Some((amount, disagreed)) = consensus(&votes, 1) {
                fields.push(Field {
                    key: format!("box12:{code}"),
                    label: format!("Box 12, code {code}"),
                    amount: Some(amount),
                    uncertain: disagreed,
                    ..Field::default()
                });
            }
        }
        let mut others = BTreeMap::<String, Vec<f64>>::new();
        for page in &reading.pages {
            for (name, amount) in box14(&page.lines) {
                // "SDI" and "CA SDI" are one item, printed differently on different copies.
                let key = others
                    .keys()
                    .find(|known| known.ends_with(&name) || name.ends_with(known.as_str()))
                    .cloned()
                    .unwrap_or(name);
                others.entry(key).or_default().push(amount);
            }
        }
        for (name, values) in others {
            let votes = values.into_iter().map(Some).collect::<Vec<_>>();
            if let Some((amount, disagreed)) = consensus(&votes, 1) {
                fields.push(Field {
                    key: format!("box14:{name}"),
                    label: format!("Box 14, {name}"),
                    amount: Some(amount),
                    uncertain: disagreed,
                    ..Field::default()
                });
            }
        }
        let state = reading
            .pages
            .iter()
            .flat_map(|page| &page.lines)
            .find_map(|line| {
                STATE
                    .captures(line.text.trim())
                    .map(|found| found[1].to_owned())
            });
        if let Some(state) = state {
            fields.push(Field {
                key: "state".into(),
                label: "State".into(),
                text: Some(state),
                ..Field::default()
            });
        }
    }
    let checks = if form == "W-2" {
        w2_checks(&mut fields)
    } else {
        Vec::new()
    };
    Read {
        kind: Some(Kind::TaxForm),
        form: Some(if corrected && form == "W-2" {
            "W-2c".into()
        } else {
            form
        }),
        corrected,
        issuer: issuer_name,
        tax_year: printed_year(reading),
        fields,
        checks,
        ..Read::default()
    }
}

/// Social Security tax is 6.2% of Social Security wages and Medicare tax 1.45% of Medicare
/// wages, to the cent the way payroll rounds them.
///
/// Two figures read apart that agree to the rate confirm each other, so neither stays in doubt.
fn w2_checks(fields: &mut [Field]) -> Vec<super::Check> {
    let amount = |fields: &[Field], key: &str| {
        fields
            .iter()
            .find(|field| field.key == key)
            .and_then(|field| field.amount)
    };
    let mut checks = Vec::new();
    for (wages, tax, rate, label) in [
        (
            "box3",
            "box4",
            0.062,
            "Social Security tax is 6.2% of its wages",
        ),
        ("box5", "box6", 0.0145, "Medicare tax is 1.45% of its wages"),
    ] {
        if let (Some(paid), Some(withheld)) = (amount(fields, wages), amount(fields, tax)) {
            // Payroll rounds each paycheck, so a year can drift a few cents from the rate.
            let passed = (paid * rate - withheld).abs() <= 0.5;
            if passed {
                for field in fields
                    .iter_mut()
                    .filter(|field| field.key == wages || field.key == tax)
                {
                    field.uncertain = false;
                }
            }
            checks.push(super::Check {
                label: label.into(),
                passed,
            });
        }
    }
    checks
}

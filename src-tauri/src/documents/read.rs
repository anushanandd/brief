//! A document's text as lines with positions, from the native bridge, and the small readers
//! every form shares: amounts, dates, rows, and what a file's name suggests.
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;
use serde::Deserialize;

/// One line of text and its box, in fractions of the page from the top left.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Line {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(default = "certain")]
    pub confidence: f64,
}

fn certain() -> f64 {
    1.0
}

impl Line {
    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn middle(&self) -> f64 {
        self.y + self.h / 2.0
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Page {
    /// `text` for a PDF's own text layer, `vision` for recognized text.
    pub source: String,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Reading {
    pub pages: Vec<Page>,
    /// A password-protected PDF, which Brief can't read.
    #[serde(default)]
    pub encrypted: bool,
}

impl Reading {
    pub fn text(&self) -> String {
        self.pages
            .iter()
            .flat_map(|page| &page.lines)
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.pages.iter().all(|page| page.lines.is_empty())
    }
}

/// Reads the file at `path` on this Mac. `recognize` asks Vision to read each PDF page as an
/// image, which keeps columns apart; images are always recognized.
#[cfg(target_os = "macos")]
pub fn read_file(path: &std::path::Path, recognize: bool) -> Result<Reading, String> {
    use std::ffi::{c_char, CStr, CString};

    unsafe extern "C" {
        fn brief_document_read(path: *const c_char, recognize: i32) -> *mut c_char;
        fn brief_document_free(value: *mut c_char);
    }
    let path = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| "Invalid file path")?;
    let output = unsafe { brief_document_read(path.as_ptr(), i32::from(recognize)) };
    if output.is_null() {
        return Err("Brief can't read this file".into());
    }
    let text = unsafe { CStr::from_ptr(output) }
        .to_string_lossy()
        .into_owned();
    unsafe { brief_document_free(output) };
    serde_json::from_str(&text).map_err(|_| "Brief can't read this file".into())
}

#[cfg(not(target_os = "macos"))]
pub fn read_file(_path: &std::path::Path, _recognize: bool) -> Result<Reading, String> {
    Err("Reading documents needs macOS".into())
}

static MONEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\(?-?\$?\s*(\d{1,3}(?:,\d{3})+|\d+)(\.\d{2})\)?(-)?$").expect("valid")
});

/// An amount as forms print it: `4,807.69`, `$1,056.00`, `4,342.05-`, `(12.00)`, `-40.00`.
/// Whole numbers are left out, since box numbers and hours look the same.
pub fn money(text: &str) -> Option<f64> {
    let cleaned = text.trim().replace(' ', "");
    let found = MONEY.captures(&cleaned)?;
    let value: f64 = format!("{}{}", found[1].replace(',', ""), &found[2])
        .parse()
        .ok()?;
    let negative = cleaned.starts_with('-') || cleaned.starts_with('(') || found.get(3).is_some();
    Some(if negative { -value } else { value })
}

/// Every amount in `text`, split at spaces: a line may hold a label and its figures. A minus
/// printed apart from its figure ("- 27.91") still makes it negative.
pub fn amounts(text: &str) -> Vec<f64> {
    let mut found = Vec::new();
    let mut negate = false;
    for word in text.split_whitespace() {
        match money(word) {
            Some(value) => found.push(if negate { -value.abs() } else { value }),
            None => {
                negate = matches!(word, "-" | "–" | "−");
                continue;
            }
        }
        negate = false;
    }
    found
}

static DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{1,2})/(\d{1,2})/(\d{4})\b").expect("valid"));

/// The first `MM/DD/YYYY` date in `text`.
pub fn date(text: &str) -> Option<NaiveDate> {
    let found = DATE.captures(text)?;
    NaiveDate::from_ymd_opt(
        found[3].parse().ok()?,
        found[1].parse().ok()?,
        found[2].parse().ok()?,
    )
}

/// A line's words with the right edge of each, estimated from where it falls in the line.
pub fn words(line: &Line) -> Vec<(&str, f64)> {
    let characters = line.text.chars().count().max(1) as f64;
    let mut found = Vec::new();
    let mut offset = 0;
    for word in line.text.split(' ') {
        let end = offset + word.chars().count();
        if !word.is_empty() {
            found.push((word, line.x + line.w * end as f64 / characters));
        }
        offset = end + 1;
    }
    found
}

/// A row's words in the order they sit across the page, whichever line each came from.
pub fn row_text(row: &[&Line]) -> String {
    let mut placed = row.iter().flat_map(|line| words(line)).collect::<Vec<_>>();
    placed.sort_by(|left, right| left.1.total_cmp(&right.1));
    placed
        .into_iter()
        .map(|(word, _)| word)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The page's lines grouped into rows by their middles, each left to right.
pub fn rows(lines: &[Line]) -> Vec<Vec<&Line>> {
    let mut sorted = lines.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.middle().total_cmp(&right.middle()));
    let mut rows: Vec<Vec<&Line>> = Vec::new();
    for line in sorted {
        let tolerance = line.h.max(0.004) * 0.6;
        match rows.last_mut() {
            Some(row) if (row[0].middle() - line.middle()).abs() <= tolerance => row.push(line),
            _ => rows.push(vec![line]),
        }
    }
    for row in &mut rows {
        row.sort_by(|left, right| left.x.total_cmp(&right.x));
    }
    rows
}

/// What a file's own name says, as people and institutions name them: `W-2 2025 Forge
/// Global.jpeg`, or `20250430-statements-9967-.pdf` with the account's last four digits.
#[derive(Debug, Default, PartialEq)]
pub struct FileHints {
    pub year: Option<i32>,
    pub issuer: Option<String>,
    pub mask: Option<String>,
}

pub fn file_hints(file_name: &str) -> FileHints {
    const STOP: [&str; 20] = [
        "w",
        "2",
        "w2",
        "w2c",
        "c",
        "1099",
        "nec",
        "int",
        "div",
        "misc",
        "5498",
        "3922",
        "statement",
        "statements",
        "payslip",
        "paystub",
        "pay",
        "stub",
        "form",
        "copy",
    ];
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    let mut hints = FileHints::default();
    let mut words = Vec::new();
    for word in stem.split(|c: char| !(c.is_alphanumeric() || c == '\'' || c == '&')) {
        let lower = word.to_lowercase();
        if word.is_empty() || STOP.contains(&lower.as_str()) {
            continue;
        }
        if word.chars().all(|c| c.is_ascii_digit()) {
            match (word.len(), word.parse::<i32>()) {
                (4, Ok(year)) if (1990..=2100).contains(&year) => hints.year = Some(year),
                (4, _) => hints.mask = Some(word.to_owned()),
                _ => {}
            }
        } else if word.chars().any(char::is_alphabetic) {
            words.push(word.to_owned());
        }
    }
    hints.issuer = (!words.is_empty()).then(|| words.join(" "));
    hints
}

/// The year printed most often on its own, as forms print their tax year.
pub fn printed_year(reading: &Reading) -> Option<i32> {
    let mut counts = std::collections::BTreeMap::<i32, usize>::new();
    for line in reading.pages.iter().flat_map(|page| &page.lines) {
        for word in line.text.split_whitespace() {
            if let Ok(year) = word
                .trim_matches(|c: char| !c.is_ascii_digit())
                .parse::<i32>()
            {
                if word.trim_matches(|c: char| !c.is_ascii_digit()).len() == 4
                    && (2000..=2100).contains(&year)
                {
                    *counts.entry(year).or_default() += 1;
                }
            }
        }
    }
    counts
        .into_iter()
        .max_by_key(|(year, count)| (*count, *year))
        .map(|(year, _)| year)
}

static MASK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:x{2,}|\*{2,}|\.{3})\s*-?(\d{3,4})\b").expect("valid"));

/// An account number printed masked, such as `XXXXX8549` or `...6744`: its last digits.
pub fn masked(text: &str) -> Option<String> {
    MASK.captures(text).map(|found| found[1].to_owned())
}

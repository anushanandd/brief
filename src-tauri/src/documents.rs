//! Documents: the user's own copies of pay statements, tax forms, and statements. Brief keeps
//! each original encrypted (`crypto`, `files`) and what it read from it here, in the finance
//! database. Reading happens on this Mac (`read`): a PDF's text layer, and Vision's recognition
//! where columns matter or there is no text. Every figure is read, never inferred; the user
//! confirms or corrects it, and a confirmed value is never overwritten by a later reading.
use serde::{Deserialize, Serialize};

pub mod crypto;
pub mod files;
mod forms;
mod payslip;
pub mod read;
mod statement;

pub use read::Reading;

/// Raised whenever a reader reads documents better, so those read before can be read again.
pub const READER_VERSION: u32 = 4;

/// Imports refuse anything larger; a phone photo of a form is a few megabytes.
pub const MAX_DOCUMENT_BYTES: usize = 40 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Payslip,
    /// A W-2, 1099, 5498, 3922, or another information return; `form` says which.
    TaxForm,
    /// A filed return, such as a Form 1040.
    TaxReturn,
    /// A bank, card, or brokerage statement as the institution issued it.
    Statement,
    /// A CSV or XLSX activity export.
    Export,
    Other,
}

/// One figure a document states, as read and as the user confirmed or corrected it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    /// Stable within a form: `box1`, `box12:D`, `gross`, `pretax401k`.
    pub key: String,
    pub label: String,
    /// The period's amount, or the box's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    /// A pay statement's year-to-date amount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ytd: Option<f64>,
    /// A code, date, or name rather than an amount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The reading disagreed with itself (copies of a form, or recognition below certainty).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub uncertain: bool,
    /// The user typed this value; a later reading never replaces it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub edited: bool,
}

/// Money a pay statement says it deposited, and where.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PayDeposit {
    /// The account number's last characters as printed, such as `8549`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<String>,
    pub amount: f64,
}

/// A self-check the reading passed or failed, such as gross less deductions equalling net.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub label: String,
    pub passed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// SHA-256 of the original bytes, so a file imported twice is one document.
    pub id: String,
    pub file_name: String,
    pub media_type: String,
    pub size: u64,
    pub imported_at: String,
    pub kind: Kind,
    /// The form's name, such as `W-2` or `1099-NEC`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<String>,
    /// A corrected form (W-2c, or a 1099 marked CORRECTED) replaces the original's figures.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub corrected: bool,
    /// The employer, payer, or institution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tax_year: Option<i32>,
    /// A pay statement's check date, or a statement's last day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period_end: Option<String>,
    /// The account it belongs to: a statement's, or the one a statement's mask names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// The account number's last characters as the document prints them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deposits: Vec<PayDeposit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Check>,
    /// The statement history Brief read from it, kept as an import the user adds by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_rows: Option<usize>,
    /// A PDF statement's rows as CSV, for the statement importer. An export is its own CSV.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_csv: Option<String>,
    /// The imported statement file made from this document, once the user added it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_file_id: Option<String>,
    /// The user looked over every figure.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reviewed: bool,
    /// Whose document it is when it isn't the user's, by person id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    /// Why nothing could be read, such as a password-protected PDF.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<String>,
    /// The `READER_VERSION` that read it; zero for documents read before versions.
    #[serde(default)]
    pub reader: u32,
    /// A few sentences on what it is, written on this Mac by Apple Intelligence from the
    /// document's text. Kept only when every figure it states is printed in the document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl Document {
    pub fn field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.key == key)
    }

    pub fn amount(&self, key: &str) -> Option<f64> {
        self.field(key).and_then(|field| field.amount)
    }

    pub fn ytd(&self, key: &str) -> Option<f64> {
        self.field(key).and_then(|field| field.ytd)
    }

    /// Checks the saved form before it is persisted.
    pub fn validate(&self) -> Result<(), String> {
        let short =
            |text: &str, limit: usize| text.len() <= limit && !text.chars().any(char::is_control);
        let optional = |text: &Option<String>, limit: usize| {
            text.as_deref().is_none_or(|text| short(text, limit))
        };
        if self.id.len() != 64
            || !self
                .id
                .chars()
                .all(|character| character.is_ascii_hexdigit())
            || !short(&self.file_name, 255)
            || !short(&self.media_type, 80)
            || !optional(&self.form, 20)
            || !optional(&self.issuer, 120)
            || !optional(&self.account_id, 220)
            || !optional(&self.mask, 8)
            || !optional(&self.person, 64)
            || self.summary.as_deref().is_some_and(|summary| {
                summary.len() > 1_500
                    || summary
                        .chars()
                        .any(|character| character.is_control() && character != '\n')
            })
            || self.fields.len() > 120
            || self.deposits.len() > 12
            || self.fields.iter().any(|field| {
                !short(&field.key, 60)
                    || !short(&field.label, 120)
                    || !optional(&field.text, 120)
                    || [field.amount, field.ytd]
                        .into_iter()
                        .flatten()
                        .any(|value| !value.is_finite() || value.abs() > 1e12)
            })
        {
            return Err("Invalid document".into());
        }
        Ok(())
    }
}

/// What a reading produced, before it becomes a document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Read {
    pub kind: Option<Kind>,
    pub form: Option<String>,
    pub corrected: bool,
    pub issuer: Option<String>,
    pub tax_year: Option<i32>,
    pub date: Option<String>,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub mask: Option<String>,
    pub fields: Vec<Field>,
    pub deposits: Vec<PayDeposit>,
    pub checks: Vec<Check>,
    /// A statement's rows as CSV, ready for the statement importer.
    pub statement_csv: Option<(String, usize)>,
}

/// What the document is, from its text: the form's own title first, then what a pay or account
/// statement always prints. `file_name` breaks ties a reading can't, like an issuer's name.
pub fn understand(reading: &Reading, file_name: &str) -> Read {
    let text = reading.text().to_lowercase();
    let name = read::file_hints(file_name);
    let mut read = if let Some(form) = forms::identify(&text) {
        forms::read(reading, form)
    } else if payslip::is_payslip(&text) {
        payslip::read(reading)
    } else if let Some(statement) = statement::read(reading) {
        statement
    } else if text.contains("form 1040") && text.contains("individual income tax return") {
        Read {
            kind: Some(Kind::TaxReturn),
            form: Some("1040".into()),
            ..Read::default()
        }
    } else {
        Read::default()
    };
    // A name the user gave the file is cleaner than one read from a photo, when they agree.
    let shares_word = |reading: &str, named: &str| {
        let named = named.to_lowercase();
        reading
            .to_lowercase()
            .split_whitespace()
            .any(|word| word.len() >= 4 && named.contains(word))
    };
    match (&read.issuer, name.issuer) {
        (None, named) => read.issuer = named,
        (Some(reading), Some(named)) if shares_word(reading, &named) => read.issuer = Some(named),
        _ => {}
    }
    if read.mask.is_none() {
        read.mask = name.mask;
    }
    if read.tax_year.is_none() && read.kind == Some(Kind::TaxForm) {
        read.tax_year = name.year;
    }
    read
}

/// What a file's first bytes say it is; Brief keeps PDFs, photos, and activity exports.
pub fn media_type(file_name: &str, bytes: &[u8]) -> Option<&'static str> {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    if bytes.starts_with(b"%PDF") {
        Some("application/pdf")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.get(4..12).is_some_and(|brand| {
        [b"ftypheic", b"ftypheix", b"ftypmif1", b"ftyphevc"]
            .iter()
            .any(|known| brand == *known)
    }) {
        Some("image/heic")
    } else if bytes.starts_with(b"PK\x03\x04") && extension == "xlsx" {
        Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
    } else if extension == "csv" && std::str::from_utf8(bytes).is_ok() {
        Some("text/csv")
    } else {
        None
    }
}

/// An account a document can belong to, matched by the last digits its documents print.
pub struct AccountMatch<'a> {
    pub id: &'a str,
    pub mask: Option<&'a str>,
}

fn account_for(accounts: &[AccountMatch<'_>], mask: Option<&str>) -> Option<String> {
    let mask = mask?;
    let mut found = accounts.iter().filter(|account| {
        account
            .mask
            .is_some_and(|known| known.ends_with(mask) || mask.ends_with(known))
    });
    let first = found.next()?;
    // Two accounts ending alike can't be told apart; the user picks.
    found.next().is_none().then(|| first.id.to_owned())
}

/// Reads a file into a document: an export through the statement reader, anything else on
/// this Mac with its text layer first and Vision where columns matter or there is no text.
pub fn ingest(
    path: &std::path::Path,
    file_name: &str,
    bytes: &[u8],
    accounts: &[AccountMatch<'_>],
    now: &str,
) -> Result<Document, String> {
    let media_type =
        media_type(file_name, bytes).ok_or("Brief keeps PDFs, photos, and CSV or Excel exports")?;
    let mut document = Document {
        id: crate::statements::content_id(bytes),
        file_name: file_name.chars().take(255).collect(),
        media_type: media_type.into(),
        size: bytes.len() as u64,
        imported_at: now.into(),
        kind: Kind::Other,
        form: None,
        corrected: false,
        issuer: None,
        tax_year: None,
        date: None,
        period_start: None,
        period_end: None,
        account_id: None,
        mask: None,
        fields: Vec::new(),
        deposits: Vec::new(),
        checks: Vec::new(),
        statement_rows: None,
        statement_csv: None,
        statement_file_id: None,
        reviewed: false,
        person: None,
        unreadable: None,
        reader: READER_VERSION,
        summary: None,
    };
    if media_type.starts_with("text/") || media_type.contains("spreadsheet") {
        match crate::statements::parse(bytes) {
            Ok(parsed) => {
                let first = parsed.rows.iter().map(|row| row.posted_on).min();
                let last = parsed.rows.iter().map(|row| row.posted_on).max();
                document.kind = Kind::Export;
                document.form = Some("Activity export".into());
                document.statement_rows = Some(parsed.rows.len());
                document.issuer = parsed.hint.label.clone();
                document.mask = parsed
                    .hint
                    .last_four
                    .clone()
                    .or(read::file_hints(file_name).mask);
                document.period_start = first.map(|day| day.to_string());
                document.period_end = last.map(|day| day.to_string());
                document.date = document.period_end.clone();
                document.tax_year = last.map(|day| chrono::Datelike::year(&day));
            }
            Err(error) => document.unreadable = Some(error),
        }
        document.account_id = account_for(accounts, document.mask.as_deref());
        return Ok(document);
    }
    let quick = read::read_file(path, false)?;
    if quick.encrypted {
        document.unreadable = Some("This PDF is password-protected".into());
        return Ok(document);
    }
    let mut read = understand(&quick, file_name);
    if quick.pages.iter().any(|page| page.source == "text")
        && (matches!(read.kind, Some(Kind::Payslip | Kind::TaxForm)) || quick.is_empty())
    {
        read = understand(&read::read_file(path, true)?, file_name);
        confirm_against_text(&mut read.fields, &quick);
    }
    if read.kind.is_none() && quick.is_empty() {
        document.unreadable = Some("Brief found no text in this file".into());
    }
    let deposit_mask = read
        .deposits
        .first()
        .and_then(|deposit| deposit.mask.clone());
    document.kind = read.kind.unwrap_or(Kind::Other);
    document.form = read.form;
    document.corrected = read.corrected;
    document.issuer = read.issuer;
    document.tax_year = read.tax_year;
    document.date = read.date;
    document.period_start = read.period_start;
    document.period_end = read.period_end;
    document.mask = read.mask;
    document.fields = read.fields;
    document.deposits = read.deposits;
    document.checks = read.checks;
    if let Some((csv, rows)) = read.statement_csv {
        document.statement_rows = Some(rows);
        document.statement_csv = Some(csv);
    }
    document.account_id = account_for(
        accounts,
        if document.kind == Kind::Payslip {
            deposit_mask.as_deref()
        } else {
            document.mask.as_deref()
        },
    );
    Ok(document)
}

/// Vision keeps a statement's columns apart but can misread a digit; the PDF's own text layer
/// prints each figure exactly. A figure in doubt is confirmed when every amount Vision read for
/// it appears, to the cent, in the text layer.
fn confirm_against_text(fields: &mut [Field], text: &Reading) {
    let printed = text
        .pages
        .iter()
        .filter(|page| page.source == "text")
        .flat_map(|page| &page.lines)
        .flat_map(|line| read::amounts(&line.text))
        .map(|value| crate::numeric::minor_units(value.abs()))
        .collect::<std::collections::BTreeSet<_>>();
    if printed.is_empty() {
        return;
    }
    for field in fields.iter_mut().filter(|field| field.uncertain) {
        let read = [field.amount, field.ytd]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if !read.is_empty()
            && read
                .iter()
                .all(|value| printed.contains(&crate::numeric::minor_units(value.abs())))
        {
            field.uncertain = false;
        }
    }
}

/// Keeps what the user typed, and takes everything else from a new reading.
pub fn merge_fields(previous: &[Field], read: Vec<Field>) -> Vec<Field> {
    let mut fields = read;
    for edited in previous.iter().filter(|field| field.edited) {
        match fields.iter_mut().find(|field| field.key == edited.key) {
            Some(field) => *field = edited.clone(),
            None => fields.push(edited.clone()),
        }
    }
    fields
}

#[cfg(test)]
mod tests;

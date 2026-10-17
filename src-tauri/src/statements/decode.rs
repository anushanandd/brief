//! Statement bytes to a grid of text cells. XLSX is read by content, not file name; anything
//! else is read as delimited text.
use std::io::Cursor;

use calamine::{Data, Reader, Xlsx};

pub(super) fn grid(contents: &[u8]) -> Result<Vec<Vec<String>>, String> {
    if contents.starts_with(b"PK\x03\x04") {
        return workbook(contents);
    }
    delimited(&text(contents))
}

/// UTF-8 (with or without a byte-order mark), else Latin-1, which every byte decodes as.
fn text(contents: &[u8]) -> String {
    let contents = contents.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(contents);
    match std::str::from_utf8(contents) {
        Ok(text) => text.to_owned(),
        Err(_) => contents.iter().map(|&byte| char::from(byte)).collect(),
    }
}

fn delimited(text: &str) -> Result<Vec<Vec<String>>, String> {
    // The delimiter that splits the most lines into the same, largest number of fields.
    let lines = text.lines().filter(|line| !line.trim().is_empty()).take(60);
    let sample = lines.collect::<Vec<_>>();
    let delimiter = [b',', b';', b'\t']
        .into_iter()
        .max_by_key(|delimiter| {
            let mut counts = std::collections::BTreeMap::<usize, usize>::new();
            for line in &sample {
                let fields = line.bytes().filter(|byte| byte == delimiter).count();
                if fields > 0 {
                    *counts.entry(fields).or_default() += 1;
                }
            }
            counts
                .into_iter()
                .max_by_key(|(fields, lines)| (*lines, *fields))
                .map_or(0, |(fields, lines)| lines * 1000 + fields)
        })
        .unwrap_or(b',');
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|_| "This file isn't a readable CSV statement")?;
        rows.push(record.iter().map(|cell| cell.trim().to_owned()).collect());
    }
    Ok(rows)
}

fn workbook(contents: &[u8]) -> Result<Vec<Vec<String>>, String> {
    let unreadable = || "This file isn't a readable Excel statement".to_string();
    let mut workbook: Xlsx<_> =
        calamine::open_workbook_from_rs(Cursor::new(contents)).map_err(|_| unreadable())?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(unreadable)?
        .map_err(|_| unreadable())?;
    let (top, left) = range.start().unwrap_or((0, 0));
    let mut rows = vec![Vec::new(); top as usize];
    for row in range.rows() {
        let mut cells = vec![String::new(); left as usize];
        cells.extend(row.iter().map(cell));
        rows.push(cells);
    }
    Ok(rows)
}

fn cell(value: &Data) -> String {
    match value {
        Data::Empty | Data::Error(_) => String::new(),
        Data::String(text) => text.trim().to_owned(),
        Data::Int(value) => value.to_string(),
        // Rust prints the shortest text that reads back as the same float, which is the
        // decimal the spreadsheet stored for any amount with up to 15 significant digits.
        Data::Float(value) => value.to_string(),
        Data::Bool(value) => value.to_string(),
        // Date cells become ISO dates, which read the same whatever the file's date order.
        Data::DateTime(value) => {
            let days = value.as_f64().floor() as i64;
            chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
                .and_then(|epoch| epoch.checked_add_signed(chrono::Duration::days(days)))
                .map(|date| date.to_string())
                .unwrap_or_default()
        }
        Data::DateTimeIso(text) | Data::DurationIso(text) => text.clone(),
    }
}

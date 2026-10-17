//! What a card issuer's export says about the merchant beyond its statement text: the store's
//! address, the issuer's own category, and the fuller name and phone number its details carry.
//! Only these are read; the details' other lines (items bought, travel and passenger records)
//! are never kept.
use serde::{Deserialize, Serialize};

use super::table::{blank, collapsed};

/// The columns an export names for these, if any.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Columns {
    pub details: Option<usize>,
    pub address: Option<usize>,
    pub locality: Option<usize>,
    pub postal_code: Option<usize>,
    pub country: Option<usize>,
    pub category: Option<usize>,
}

/// The merchant as the card issuer records it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IssuerDetail {
    /// The fuller name in the issuer's details, which statement text truncates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The street address, as printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// The US state, or the region printed abroad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,
    /// ISO 3166 code, when the printed country is recognized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// The issuer's own category, such as "Restaurant-Bar & Café".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

fn country_code(text: &str) -> Option<String> {
    let text = text.trim().to_ascii_uppercase();
    match text.as_str() {
        "UNITED STATES" | "UNITED STATES OF AMERICA" | "USA" => Some("US".into()),
        _ if text.len() == 2 && text.bytes().all(|byte| byte.is_ascii_alphabetic()) => Some(text),
        _ => None,
    }
}

/// "SAN JOSE CA" reads as a city and a US state; anything else is the city as printed.
fn locality(text: Option<String>, country: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(text) = text else {
        return (None, None);
    };
    if matches!(country, None | Some("US")) {
        if let Some((city, state)) = text.rsplit_once(' ') {
            if crate::places::us_state(&state.to_ascii_uppercase()).is_some() {
                return (
                    Some(city.trim().to_owned()),
                    Some(state.to_ascii_uppercase()),
                );
            }
        }
    }
    (Some(text), None)
}

fn key(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_uppercase())
        .collect()
}

/// The details line that starts like the statement text and says more, without its trailing
/// reference numbers. Issuers truncate statement text to a fixed width; the details don't.
fn fuller_name(details: &str, description: &str) -> Option<String> {
    let base = key(description);
    let prefix = base.chars().take(8).collect::<String>();
    if prefix.len() < 4 {
        return None;
    }
    details
        .lines()
        .map(collapsed)
        .filter_map(|line| {
            let mut words = line.split(' ').collect::<Vec<_>>();
            while words
                .last()
                .is_some_and(|word| word.bytes().filter(u8::is_ascii_digit).count() >= 5)
            {
                words.pop();
            }
            let name = words.join(" ");
            let name = name.trim_end_matches([' ', '-', '#', '*']).to_owned();
            let line_key = key(&name);
            (line_key.starts_with(&prefix) && line_key != base && !base.starts_with(&line_key))
                .then_some(name)
        })
        .max_by_key(String::len)
        .map(|name| name.chars().take(80).collect())
}

/// A ten-digit phone number in the details, written as 408-555-0100.
fn phone(details: &str) -> Option<String> {
    details.split_whitespace().find_map(|token| {
        let digits = token
            .trim_start_matches('+')
            .chars()
            .filter(|character| *character != '-')
            .collect::<String>();
        let digits = match digits.len() {
            11 if digits.starts_with('1') => &digits[1..],
            10 => digits.as_str(),
            _ => return None,
        };
        let shaped = token
            .trim_start_matches('+')
            .bytes()
            .all(|byte| byte.is_ascii_digit())
            || token.split('-').map(str::len).eq([3, 3, 4]);
        let real = digits.bytes().all(|byte| byte.is_ascii_digit())
            && !digits.starts_with(['0', '1'])
            && digits.bytes().any(|byte| byte != digits.as_bytes()[0]);
        (shaped && real).then(|| format!("{}-{}-{}", &digits[..3], &digits[3..6], &digits[6..]))
    })
}

pub(super) fn detail(row: &[String], columns: &Columns, description: &str) -> Option<IssuerDetail> {
    let text = |column: Option<usize>| {
        column
            .and_then(|column| row.get(column))
            .map(|cell| collapsed(cell))
            .filter(|value| !blank(value))
            .map(|value| value.chars().take(120).collect::<String>())
    };
    let printed_country = text(columns.country);
    let country = printed_country.as_deref().and_then(country_code);
    let (city, region) = locality(text(columns.locality), country.as_deref());
    let details = columns
        .details
        .and_then(|column| row.get(column))
        .map(String::as_str)
        .unwrap_or("");
    let detail = IssuerDetail {
        name: fuller_name(details, description),
        address: text(columns.address),
        country: country
            .or_else(|| (printed_country.is_none() && region.is_some()).then(|| "US".to_owned())),
        city,
        region,
        postal_code: text(columns.postal_code),
        category: text(columns.category),
        phone: phone(details),
    };
    (detail != IssuerDetail::default()).then_some(detail)
}

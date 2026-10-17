//! Finds the activity table in a statement grid, maps its columns, and reads its rows.
//! Column names come from `vocabulary.json`; when a name is unfamiliar, what a column
//! contains (dates, amounts, text) fills the date, amount, and description roles.
use std::str::FromStr;
use std::sync::LazyLock;

use chrono::{Datelike, NaiveDate};
use regex::Regex;
use rust_decimal::Decimal;
use serde::Deserialize;

use super::{
    deposit_activity, issuer, AccountHint, Activity, ContributionMemo, StatementRow, VOCABULARY,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Columns {
    posted: Vec<String>,
    occurred: Vec<String>,
    settled: Vec<String>,
    amount: Vec<String>,
    debit: Vec<String>,
    credit: Vec<String>,
    balance: Vec<String>,
    description: Vec<String>,
    activity: Vec<String>,
    symbol: Vec<String>,
    quantity: Vec<String>,
    price: Vec<String>,
    fee: Vec<String>,
    pending: Vec<String>,
    details: Vec<String>,
    address: Vec<String>,
    locality: Vec<String>,
    postal_code: Vec<String>,
    country: Vec<String>,
    issuer_category: Vec<String>,
}

pub(super) struct Table {
    pub rows: Vec<StatementRow>,
    pub memos: Vec<ContributionMemo>,
    pub hint: AccountHint,
    pub total: Option<Decimal>,
    pub dates_assumed: bool,
}

#[derive(Debug, Default, PartialEq)]
struct Mapping {
    posted: usize,
    occurred: Option<usize>,
    settled: Option<usize>,
    amount: Option<usize>,
    debit: Option<usize>,
    credit: Option<usize>,
    balance: Option<usize>,
    description: usize,
    activity: Option<usize>,
    symbol: Option<usize>,
    quantity: Option<usize>,
    price: Option<usize>,
    fee: Option<usize>,
    pending: Option<usize>,
    issuer: issuer::Columns,
}

/// Lowercase words, without currency and count marks: "Amount ($)" and "Amount $" both read
/// "amount", and "Running Bal." reads "running bal".
pub(super) fn normalized(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '$' | '#' | '(' | ')' | '*' | ':' => ' ',
            _ => character.to_ascii_lowercase(),
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches('.')
        .to_owned()
}

pub(super) fn collapsed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn blank(text: &str) -> bool {
    let text = text.trim();
    text.is_empty() || text.chars().all(|character| character == '-')
}

/// An amount as statements print it: `$1,234.56`, `-1234.56`, `(1,234.56)`, `1234.56-`, or
/// with a CR or DR suffix. Blank cells and dashes are no amount.
pub(super) fn money(text: &str) -> Option<Result<Decimal, ()>> {
    if blank(text) {
        return None;
    }
    let mut text = text.trim().to_ascii_uppercase();
    let mut negative = false;
    if text.starts_with('(') && text.ends_with(')') {
        negative = true;
        text = text[1..text.len() - 1].to_owned();
    }
    for (suffix, sign) in [("CR", false), ("DR", true)] {
        if let Some(stripped) = text.strip_suffix(suffix) {
            negative ^= sign;
            text = stripped.to_owned();
        }
    }
    if let Some(stripped) = text.trim().strip_suffix('-') {
        negative = !negative;
        text = stripped.to_owned();
    }
    let digits = text
        .chars()
        .filter(|character| !matches!(character, '$' | ',' | ' ' | '+'))
        .collect::<String>()
        .trim_start_matches("USD")
        .to_owned();
    let value = Decimal::from_str(&digits).or_else(|_| Decimal::from_scientific(&digits));
    Some(
        value
            .map(|value| if negative { -value } else { value })
            .map_err(|_| ()),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Order {
    MonthFirst,
    DayFirst,
}

/// The three numeric parts of a date cell, or `None` if the cell isn't a date. A time after
/// the date is ignored.
fn date_parts(text: &str) -> Option<(u32, u32, i32, bool)> {
    let text = text.split_whitespace().next()?;
    let parts = text.split(['/', '-', '.']).collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let numbers = parts
        .iter()
        .map(|part| part.parse::<i32>().ok())
        .collect::<Option<Vec<_>>>()?;
    if parts[0].len() == 4 {
        // ISO order: always year, month, day.
        return Some((numbers[1] as u32, numbers[2] as u32, numbers[0], true));
    }
    let year = match parts[2].len() {
        2 => 2000 + numbers[2],
        4 => numbers[2],
        _ => return None,
    };
    Some((numbers[0] as u32, numbers[1] as u32, year, false))
}

fn date(text: &str, order: Order) -> Option<NaiveDate> {
    let (first, second, year, iso) = date_parts(text)?;
    let (month, day) = if iso || order == Order::MonthFirst {
        (first, second)
    } else {
        (second, first)
    };
    NaiveDate::from_ymd_opt(year, month, day)
}

fn is_date(text: &str) -> bool {
    date(text, Order::MonthFirst).is_some() || date(text, Order::DayFirst).is_some()
}

/// The header is the row that names the most known columns, including a date and an amount.
fn header(grid: &[Vec<String>]) -> Option<(usize, Mapping)> {
    let columns = &VOCABULARY.columns;
    grid.iter()
        .enumerate()
        .take(80)
        .filter_map(|(index, row)| {
            let names = row.iter().map(|cell| normalized(cell)).collect::<Vec<_>>();
            if names.iter().any(|name| is_date(name)) {
                return None;
            }
            let known = [
                &columns.posted,
                &columns.occurred,
                &columns.settled,
                &columns.amount,
                &columns.debit,
                &columns.credit,
                &columns.balance,
                &columns.description,
                &columns.activity,
                &columns.symbol,
                &columns.quantity,
                &columns.price,
                &columns.fee,
            ]
            .iter()
            .map(|synonyms| names.iter().filter(|name| synonyms.contains(name)).count())
            .sum::<usize>();
            let body = &grid[index + 1..];
            let mapping = mapping(&names, body)?;
            Some((known, index, mapping))
        })
        .max_by_key(|(known, index, _)| (*known, std::cmp::Reverse(*index)))
        .filter(|(known, _, _)| *known >= 2)
        .map(|(_, index, mapping)| (index, mapping))
}

/// The first column, by synonym priority, whose name is one of `synonyms`.
fn named(names: &[String], synonyms: &[String], taken: &[usize]) -> Option<usize> {
    synonyms.iter().find_map(|synonym| {
        names
            .iter()
            .enumerate()
            .find(|(index, name)| *name == synonym && !taken.contains(index))
            .map(|(index, _)| index)
    })
}

fn mapping(names: &[String], body: &[Vec<String>]) -> Option<Mapping> {
    let columns = &VOCABULARY.columns;
    let sample = body
        .iter()
        .take_while(|row| row.len() >= names.len().min(3) && row.iter().any(|cell| is_date(cell)))
        .take(200)
        .collect::<Vec<_>>();
    if sample.is_empty() {
        return None;
    }
    let values = |column: usize| {
        sample
            .iter()
            .filter_map(move |row| row.get(column))
            .filter(|cell| !blank(cell))
    };
    let all_dates =
        |column: usize| values(column).count() > 0 && values(column).all(|cell| is_date(cell));
    let all_money = |column: usize| {
        values(column).count() > 0 && values(column).all(|cell| matches!(money(cell), Some(Ok(_))))
    };

    let mut taken = Vec::new();
    let take = |index: Option<usize>, taken: &mut Vec<usize>| {
        if let Some(index) = index {
            taken.push(index);
        }
        index
    };
    let mut occurred = take(named(names, &columns.occurred, &taken), &mut taken)
        .filter(|&column| all_dates(column));
    let posted = take(named(names, &columns.posted, &taken), &mut taken)
        .filter(|&column| all_dates(column))
        .or_else(|| occurred.take())
        .or_else(|| {
            (0..names.len()).find(|&column| !taken.contains(&column) && all_dates(column))
        })?;
    taken.push(posted);
    let settled = take(named(names, &columns.settled, &taken), &mut taken)
        .filter(|&column| all_dates(column));
    // A named amount column needs mostly amounts; a stray value then fails its row by number.
    let mostly_money = |column: usize| {
        let readable = values(column)
            .filter(|cell| matches!(money(cell), Some(Ok(_))))
            .count();
        readable > 0 && readable * 2 >= values(column).count()
    };
    let money_column = |column: Option<usize>| column.filter(|&column| mostly_money(column));
    let balance = money_column(take(named(names, &columns.balance, &taken), &mut taken));
    let quantity = money_column(take(named(names, &columns.quantity, &taken), &mut taken));
    let price = money_column(take(named(names, &columns.price, &taken), &mut taken));
    let fee = money_column(take(named(names, &columns.fee, &taken), &mut taken));
    let mut amount = money_column(take(named(names, &columns.amount, &taken), &mut taken));
    let mut debit = money_column(take(named(names, &columns.debit, &taken), &mut taken));
    let mut credit = money_column(take(named(names, &columns.credit, &taken), &mut taken));
    if amount.is_none() && (debit.is_none() || credit.is_none()) {
        // An unnamed amount: the one remaining all-numeric column filled on every row.
        let candidates = (0..names.len())
            .filter(|column| !taken.contains(column) && all_money(*column))
            .filter(|&column| values(column).count() == sample.len())
            .collect::<Vec<_>>();
        if let [only] = candidates[..] {
            amount = Some(only);
            debit = None;
            credit = None;
        }
    }
    if amount.is_none() && (debit.is_none() || credit.is_none()) {
        return None;
    }
    let symbol = take(named(names, &columns.symbol, &taken), &mut taken);
    let activity = take(named(names, &columns.activity, &taken), &mut taken);
    let pending = take(named(names, &columns.pending, &taken), &mut taken);
    let mut issuer_column = |synonyms: &[String]| take(named(names, synonyms, &taken), &mut taken);
    let issuer = issuer::Columns {
        details: issuer_column(&columns.details),
        address: issuer_column(&columns.address),
        locality: issuer_column(&columns.locality),
        postal_code: issuer_column(&columns.postal_code),
        country: issuer_column(&columns.country),
        category: issuer_column(&columns.issuer_category),
    };
    let description = named(names, &columns.description, &taken).or_else(|| {
        // The free-text column with the longest values.
        (0..names.len())
            .filter(|column| !taken.contains(column) && !all_money(*column) && !all_dates(*column))
            .max_by_key(|&column| {
                let lengths = values(column).map(String::len).collect::<Vec<_>>();
                lengths.iter().sum::<usize>() / lengths.len().max(1)
            })
    })?;
    Some(Mapping {
        posted,
        occurred,
        settled,
        amount,
        debit,
        credit,
        balance,
        description,
        activity,
        symbol,
        quantity,
        price,
        fee,
        pending,
        issuer,
    })
}

/// Month-first unless a date in the file can only be day-first. A file that never shows a day
/// above 12 is read month-first and says so.
fn order(rows: &[&Vec<String>], columns: &[usize]) -> Result<(Order, bool), String> {
    let mut month_first = false;
    let mut day_first = false;
    for row in rows {
        for &column in columns {
            if let Some((first, second, _, false)) =
                row.get(column).and_then(|cell| date_parts(cell))
            {
                month_first |= second > 12;
                day_first |= first > 12;
            }
        }
    }
    match (month_first, day_first) {
        (true, true) => Err("This statement mixes date formats".into()),
        (false, true) => Ok((Order::DayFirst, false)),
        (true, false) => Ok((Order::MonthFirst, false)),
        (false, false) => Ok((Order::MonthFirst, true)),
    }
}

fn hint(rows: &[Vec<String>]) -> AccountHint {
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            let name = normalized(cell);
            let Some(prefix) = VOCABULARY
                .account_labels
                .iter()
                .find(|prefix| name == **prefix || name.starts_with(&format!("{prefix} ")))
            else {
                continue;
            };
            let rest = collapsed(cell.trim())
                .chars()
                .skip(prefix.chars().count())
                .collect::<String>();
            let rest = rest.trim_start_matches([':', ' ']).trim();
            let label = if rest.is_empty() {
                row.iter()
                    .skip(index + 1)
                    .find(|cell| !blank(cell))
                    .map(|cell| collapsed(cell))
            } else {
                // "… from Current Year" describes the export window, not the account.
                Some(
                    rest.split(" from ")
                        .next()
                        .unwrap_or(rest)
                        .trim()
                        .to_owned(),
                )
            };
            let Some(label) = label.filter(|label| !label.is_empty()) else {
                continue;
            };
            let digits = label
                .trim_end()
                .chars()
                .rev()
                .take_while(|character| character.is_ascii_digit())
                .collect::<Vec<_>>();
            let last_four =
                (digits.len() >= 4).then(|| digits[..4].iter().rev().collect::<String>());
            return AccountHint {
                label: Some(label.chars().take(120).collect()),
                last_four,
            };
        }
    }
    AccountHint::default()
}

/// The statement's own total of its amounts. Above the table it is the first amount after
/// the label; below it, only the amount column counts, since a split debit and credit table
/// totals each side separately.
fn total(
    grid: &[Vec<String>],
    header: usize,
    amount: Option<usize>,
) -> Result<Option<Decimal>, String> {
    for (index, row) in grid.iter().enumerate() {
        let Some(first) = row.iter().position(|cell| !blank(cell)) else {
            continue;
        };
        if !VOCABULARY.total_labels.contains(&normalized(&row[first])) {
            continue;
        }
        let value = if index < header {
            row.iter().skip(first + 1).find_map(|cell| money(cell))
        } else {
            amount
                .and_then(|column| row.get(column))
                .and_then(|cell| money(cell))
        };
        return match value {
            Some(Ok(value)) => Ok(Some(value)),
            Some(Err(())) => Err("This statement's total can't be read".into()),
            None => Ok(None),
        };
    }
    Ok(None)
}

/// A brokerage's note of which tax year an IRA contribution is for, such as E*Trade's
/// "TY 2026 $ 7500 INDIVID CONTRIB - CURR YR".
fn contribution_memo(posted_on: NaiveDate, description: &str) -> Option<ContributionMemo> {
    static MEMO: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bTY\s*(\d{4})\s*\$\s*([\d,]+(?:\.\d{1,2})?)\b.*\bCONTRIB").expect("valid")
    });
    let found = MEMO.captures(description)?;
    let tax_year = found[1].parse().ok()?;
    let amount = found[2].replace(',', "").parse::<Decimal>().ok()?;
    ((posted_on.year() - 1..=posted_on.year()).contains(&tax_year) && !amount.is_zero()).then_some(
        ContributionMemo {
            posted_on,
            tax_year,
            amount,
        },
    )
}

pub(super) fn read(grid: &[Vec<String>]) -> Result<Table, String> {
    let (header, mapping) =
        header(grid).ok_or("Brief couldn't find the date and amount columns in this file")?;
    let body = grid[header + 1..]
        .iter()
        .take_while(|row| row.get(mapping.posted).is_some_and(|cell| is_date(cell)))
        .collect::<Vec<_>>();
    let date_columns = [Some(mapping.posted), mapping.occurred, mapping.settled]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let (order, dates_assumed) = order(&body, &date_columns)?;
    fn cell(row: &[String], column: Option<usize>) -> &str {
        column
            .and_then(|column| row.get(column))
            .map(String::as_str)
            .unwrap_or("")
    }
    let pending_values = &VOCABULARY.pending_values;
    let mut rows = Vec::new();
    let mut memos = Vec::new();
    for (offset, row) in body.iter().enumerate() {
        let line = header + offset + 2;
        let unreadable = |what: &str| format!("Row {line} has an unreadable {what}");
        let number = |column: Option<usize>, what: &str| match money(cell(row, column)) {
            None => Ok(None),
            Some(Ok(value)) => Ok(Some(value)),
            Some(Err(())) => Err(unreadable(what)),
        };
        let posted_on =
            date(cell(row, Some(mapping.posted)), order).ok_or_else(|| unreadable("date"))?;
        let optional_date = |column: Option<usize>| {
            let text = cell(row, column);
            if blank(text) {
                Ok(None)
            } else {
                date(text, order)
                    .map(Some)
                    .ok_or_else(|| unreadable("date"))
            }
        };
        let occurred_on = optional_date(mapping.occurred)?.filter(|date| *date != posted_on);
        let settled_on = optional_date(mapping.settled)?;
        let amount = match mapping.amount {
            Some(column) => number(Some(column), "amount")?,
            None => {
                let credit = number(mapping.credit, "amount")?.map(|value| value.abs());
                let debit = number(mapping.debit, "amount")?.map(|value| value.abs());
                (credit.is_some() || debit.is_some())
                    .then(|| credit.unwrap_or_default() - debit.unwrap_or_default())
            }
        };
        let quantity = number(mapping.quantity, "quantity")?.filter(|value| !value.is_zero());
        let Some(amount) = amount.filter(|value| !value.is_zero() || quantity.is_some()) else {
            // A row with no money and no shares records nothing, unless it states the tax
            // year a contribution counts toward.
            memos.extend(contribution_memo(
                posted_on,
                cell(row, Some(mapping.description)),
            ));
            continue;
        };
        let activity_label = collapsed(cell(row, mapping.activity));
        let activity = deposit_activity(
            &activity_label,
            cell(row, Some(mapping.description)),
            amount.is_sign_positive() && !amount.is_zero(),
        );
        // A ticker; or, for a plan contribution buying units of a fund without one, the fund's
        // name as the statement prints it.
        let named_fund = activity == Activity::Contribution && quantity.is_some();
        let symbol = Some(cell(row, mapping.symbol).trim())
            .filter(|symbol| !blank(symbol))
            .and_then(|symbol| match symbol.len() {
                ..=12 if !named_fund || !symbol.contains(' ') => Some(symbol.to_ascii_uppercase()),
                ..=80 if named_fund => Some(collapsed(symbol)),
                _ => None,
            });
        rows.push(StatementRow {
            posted_on,
            occurred_on,
            settled_on,
            activity,
            activity_label: activity_label.chars().take(80).collect(),
            description: collapsed(cell(row, Some(mapping.description)))
                .chars()
                .take(300)
                .collect(),
            amount,
            balance: number(mapping.balance, "balance")?,
            symbol: if activity == Activity::Other && quantity.is_none() {
                None
            } else {
                symbol
            },
            quantity,
            price: number(mapping.price, "price")?.filter(|value| !value.is_zero()),
            fee: number(mapping.fee, "commission")?.filter(|value| !value.is_zero()),
            pending: pending_values.contains(&normalized(cell(row, mapping.pending))),
            issuer: issuer::detail(row, &mapping.issuer, cell(row, Some(mapping.description))),
        });
    }
    if rows.is_empty() {
        return Err("This statement has no activity".into());
    }
    if rows
        .iter()
        .any(|row| row.activity.is_trade() && (row.symbol.is_none() || row.quantity.is_none()))
    {
        return Err("A trade in this statement has no symbol or quantity".into());
    }
    Ok(Table {
        rows,
        memos,
        hint: hint(&grid[..header]),
        total: total(grid, header, mapping.amount)?,
        dates_assumed,
    })
}

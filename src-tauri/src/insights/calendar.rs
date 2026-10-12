//! Calendar rules shared by forecasts, statements, and benefit windows. Dates are plain
//! calendar days; an instant becomes a day only through an explicit `Zone`.
use std::sync::LazyLock;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc, Weekday};
use regex::Regex;

use crate::finance_contract::Transaction;

/// Converts an instant to the calendar day observed in one place.
pub trait Zone {
    fn date(&self, instant: DateTime<Utc>) -> NaiveDate;
}

/// The Mac's configured time zone.
pub struct LocalZone;

impl Zone for LocalZone {
    fn date(&self, instant: DateTime<Utc>) -> NaiveDate {
        instant.with_timezone(&chrono::Local).date_naive()
    }
}

/// The few named zones that benefit terms use, with U.S. daylight-saving rules since 2007.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamedZone {
    Utc,
    Honolulu,
    Chicago,
    LosAngeles,
    NewYork,
}

impl NamedZone {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "UTC" | "Etc/UTC" => Some(Self::Utc),
            "Pacific/Honolulu" => Some(Self::Honolulu),
            "America/Chicago" => Some(Self::Chicago),
            "America/Los_Angeles" => Some(Self::LosAngeles),
            "America/New_York" => Some(Self::NewYork),
            _ => None,
        }
    }

    fn standard_offset_hours(self) -> i64 {
        match self {
            Self::Utc => 0,
            Self::Honolulu => -10,
            Self::Chicago => -6,
            Self::LosAngeles => -8,
            Self::NewYork => -5,
        }
    }

    /// Hours from UTC at `instant`, daylight time included.
    pub fn offset_hours(self, instant: DateTime<Utc>) -> i64 {
        let standard = self.standard_offset_hours();
        let observes_dst = matches!(self, Self::Chicago | Self::LosAngeles | Self::NewYork);
        if observes_dst && in_us_daylight_time(instant, standard) {
            standard + 1
        } else {
            standard
        }
    }
}

impl Zone for NamedZone {
    fn date(&self, instant: DateTime<Utc>) -> NaiveDate {
        (instant + Duration::hours(self.offset_hours(instant))).date_naive()
    }
}

/// Daylight time runs from 2:00 standard time on the second Sunday of March until 2:00
/// daylight time (1:00 standard) on the first Sunday of November.
fn in_us_daylight_time(instant: DateTime<Utc>, standard_offset: i64) -> bool {
    let year = (instant + Duration::hours(standard_offset)).year();
    let at = |date: NaiveDate, hour: u32| {
        date.and_hms_opt(hour, 0, 0).expect("valid hour").and_utc()
            - Duration::hours(standard_offset)
    };
    let start = at(nth_weekday(year, 3, Weekday::Sun, 2), 2);
    let end = at(nth_weekday(year, 11, Weekday::Sun, 1), 1);
    instant >= start && instant < end
}

pub fn nth_weekday(year: i32, month: u32, weekday: Weekday, occurrence: u32) -> NaiveDate {
    NaiveDate::from_weekday_of_month_opt(year, month, weekday, occurrence as u8)
        .expect("every month has the first four weekdays")
}

pub fn last_weekday(year: i32, month: u32, weekday: Weekday) -> NaiveDate {
    let mut date = month_end(year, month);
    while date.weekday() != weekday {
        date = date.pred_opt().expect("date in range");
    }
    date
}

pub fn month_end(year: i32, month: u32) -> NaiveDate {
    first_of_month(year, month as i64 + 1)
        .pred_opt()
        .expect("date in range")
}

/// The first day of a month; `month` may overflow or underflow the year.
pub fn first_of_month(year: i32, month: i64) -> NaiveDate {
    let index = year as i64 * 12 + month - 1;
    NaiveDate::from_ymd_opt(
        index.div_euclid(12) as i32,
        index.rem_euclid(12) as u32 + 1,
        1,
    )
    .expect("date in range")
}

pub fn add_months_first(date: NaiveDate, months: i64) -> NaiveDate {
    first_of_month(date.year(), date.month() as i64 + months)
}

pub fn is_month_end(date: NaiveDate) -> bool {
    date.succ_opt()
        .is_some_and(|next| next.month() != date.month())
}

pub fn month_index(date: NaiveDate) -> i64 {
    date.year() as i64 * 12 + date.month0() as i64
}

pub fn days_between(left: NaiveDate, right: NaiveDate) -> i64 {
    (right - left).num_days()
}

fn observed(year: i32, month: u32, day: u32) -> NaiveDate {
    let date = NaiveDate::from_ymd_opt(year, month, day).expect("fixed holiday");
    match date.weekday() {
        Weekday::Sat => date.pred_opt().expect("date in range"),
        Weekday::Sun => date.succ_opt().expect("date in range"),
        _ => date,
    }
}

/// U.S. federal holidays as observed by banks scheduling ACH settlement.
pub fn is_federal_holiday(date: NaiveDate) -> bool {
    let year = date.year();
    let mut holidays = vec![
        observed(year, 1, 1),
        observed(year + 1, 1, 1),
        nth_weekday(year, 1, Weekday::Mon, 3),
        nth_weekday(year, 2, Weekday::Mon, 3),
        last_weekday(year, 5, Weekday::Mon),
        observed(year, 7, 4),
        nth_weekday(year, 9, Weekday::Mon, 1),
        nth_weekday(year, 10, Weekday::Mon, 2),
        observed(year, 11, 11),
        nth_weekday(year, 11, Weekday::Thu, 4),
        observed(year, 12, 25),
    ];
    if year >= 2021 {
        holidays.push(observed(year, 6, 19));
    }
    holidays.contains(&date)
}

pub fn is_business_day(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) && !is_federal_holiday(date)
}

/// Business days after `left` through `right`, counting no further than six.
pub fn business_days_after(left: NaiveDate, right: NaiveDate) -> u32 {
    let mut days = 0;
    let mut date = left;
    while days <= 5 {
        date = date.succ_opt().expect("date in range");
        if date > right {
            break;
        }
        if is_business_day(date) {
            days += 1;
        }
    }
    days
}

pub fn shift_to_business_day(mut date: NaiveDate, direction: i64) -> NaiveDate {
    while !is_business_day(date) {
        date += Duration::days(direction);
    }
    date
}

pub fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// `Sep 5`
pub fn short_label(date: NaiveDate) -> String {
    date.format("%b %-d").to_string()
}

/// `Sep 5, 2026`
pub fn long_label(date: NaiveDate) -> String {
    date.format("%b %-d, %Y").to_string()
}

static ISO_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}").expect("valid pattern"));
static SHORT_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^([a-z]{3,9})\s+(\d{1,2})$").expect("valid pattern"));

fn month_number(name: &str) -> Option<u32> {
    let prefix = name.get(..3)?.to_ascii_lowercase();
    [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ]
    .iter()
    .position(|month| *month == prefix)
    .map(|index| index as u32 + 1)
}

/// A month and day with JavaScript `Date.UTC` overflow, so `Feb 30` becomes `Mar 2`.
fn overflowing_date(year: i32, month: u32, day: u32) -> NaiveDate {
    first_of_month(year, month as i64) + Duration::days(day as i64 - 1)
}

/// An ISO instant or date. A bare month and day is accepted only for compatibility with
/// legacy saved values and, like the original renderer, resolves to the year 2001.
pub fn parse_instant(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(instant) = DateTime::parse_from_rfc3339(value) {
        return Some(instant.with_timezone(&Utc));
    }
    if value.len() == 10 {
        if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
            return Some(date.and_hms_opt(0, 0, 0)?.and_utc());
        }
    }
    let captures = SHORT_DATE.captures(value.trim())?;
    let date = overflowing_date(2001, month_number(&captures[1])?, captures[2].parse().ok()?);
    // Noon UTC is that calendar day in every zone Brief uses.
    Some(date.and_hms_opt(12, 0, 0)?.and_utc())
}

/// The calendar day of an ISO reference in a zone.
pub fn calendar_date(reference: &str, zone: &dyn Zone) -> Option<NaiveDate> {
    parse_instant(reference).map(|instant| zone.date(instant))
}

/// A transaction date key: ISO dates are kept as written; a legacy `Mon D` value resolves to
/// the most recent such day on or before the reference's month.
pub fn transaction_date(value: &str, reference: &str, zone: &dyn Zone) -> Option<NaiveDate> {
    if ISO_PREFIX.is_match(value) {
        return NaiveDate::parse_from_str(&value[..10], "%Y-%m-%d").ok();
    }
    if let Some(captures) = SHORT_DATE.captures(value.trim()) {
        if let (Some(month), Some(reference)) =
            (month_number(&captures[1]), calendar_date(reference, zone))
        {
            let year = reference.year() - i32::from(month > reference.month());
            return Some(overflowing_date(year, month, captures[2].parse().ok()?));
        }
    }
    DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|instant| instant.with_timezone(&Utc).date_naive())
}

/// Posted transactions dated through `today`, oldest first, and by id within a day.
pub fn posted_through<'a>(
    transactions: &'a [Transaction],
    reference: &str,
    today: NaiveDate,
    zone: &dyn Zone,
) -> Vec<(NaiveDate, &'a Transaction)> {
    let mut posted = transactions
        .iter()
        .filter(|transaction| !transaction.pending)
        .filter_map(|transaction| {
            let date = transaction_date(transaction.posted_date(), reference, zone)?;
            (date <= today).then_some((date, transaction))
        })
        .collect::<Vec<_>>();
    posted.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.id.cmp(&right.1.id))
    });
    posted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn named_zones_follow_us_daylight_saving_boundaries() {
        let instant = |value: &str| DateTime::parse_from_rfc3339(value).unwrap().to_utc();
        let chicago = NamedZone::Chicago;
        // 05:30 UTC is 23:30 CST (Jan) but 00:30 CDT (Jul).
        assert_eq!(
            chicago.date(instant("2026-01-02T05:30:00Z")),
            date("2026-01-01")
        );
        assert_eq!(
            chicago.date(instant("2026-07-02T05:30:00Z")),
            date("2026-07-02")
        );
        // Daylight time begins at 08:00 UTC on 2026-03-08 in Chicago.
        assert_eq!(
            chicago.date(instant("2026-03-09T05:30:00Z")),
            date("2026-03-09")
        );
        assert_eq!(
            NamedZone::Honolulu.date(instant("2026-01-01T09:59:00Z")),
            date("2025-12-31")
        );
        assert_eq!(
            NamedZone::LosAngeles.date(instant("2026-11-01T07:30:00Z")),
            date("2026-11-01")
        );
    }

    #[test]
    fn holidays_and_business_days_match_bank_schedules() {
        assert!(is_federal_holiday(date("2026-07-03")));
        assert!(is_federal_holiday(date("2027-12-31")));
        assert!(is_federal_holiday(date("2026-11-26")));
        assert!(!is_business_day(date("2026-09-07")));
        assert_eq!(
            shift_to_business_day(date("2026-09-05"), 1),
            date("2026-09-08")
        );
        assert_eq!(
            shift_to_business_day(date("2026-09-07"), -1),
            date("2026-09-04")
        );
        assert_eq!(
            business_days_after(date("2026-09-04"), date("2026-09-08")),
            1
        );
    }

    #[test]
    fn resolves_legacy_short_dates_against_the_reference() {
        let zone = NamedZone::Utc;
        assert_eq!(
            transaction_date("Dec 31", "2026-01-03T12:00:00Z", &zone),
            Some(date("2025-12-31"))
        );
        assert_eq!(
            transaction_date("2026-09-01T10:00:00Z", "", &zone),
            Some(date("2026-09-01"))
        );
        assert_eq!(
            transaction_date("unknown", "2026-01-03T12:00:00Z", &zone),
            None
        );
        assert_eq!(
            transaction_date("Aug 21", "Aug 21", &zone),
            Some(date("2001-08-21"))
        );
    }
}

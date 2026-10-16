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

/// A named IANA time zone, with its daylight-saving rules from the time zone database. Benefit
/// terms name the zone a credit resets in; the market's day is New York's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamedZone(chrono_tz::Tz);

#[allow(non_upper_case_globals)]
impl NamedZone {
    pub const NewYork: Self = Self(chrono_tz::America::New_York);
    #[cfg(test)]
    pub const Utc: Self = Self(chrono_tz::UTC);
    #[cfg(test)]
    pub const Honolulu: Self = Self(chrono_tz::Pacific::Honolulu);
    #[cfg(test)]
    pub const Chicago: Self = Self(chrono_tz::America::Chicago);
    #[cfg(test)]
    pub const LosAngeles: Self = Self(chrono_tz::America::Los_Angeles);

    /// Any IANA zone name, such as `America/Phoenix`.
    pub fn parse(name: &str) -> Option<Self> {
        name.parse::<chrono_tz::Tz>().ok().map(Self)
    }

    /// The wall-clock time in this zone at `instant`.
    pub fn local(self, instant: DateTime<Utc>) -> chrono::NaiveDateTime {
        instant.with_timezone(&self.0).naive_local()
    }

    /// The instant this zone's clock reads `local`. A time skipped when clocks spring forward
    /// reads as the hour after; a repeated one, as its first occurrence.
    pub fn instant(self, local: chrono::NaiveDateTime) -> DateTime<Utc> {
        use chrono::{LocalResult, TimeZone};
        match self.0.from_local_datetime(&local) {
            LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => at.to_utc(),
            LocalResult::None => self.instant(local + Duration::hours(1)),
        }
    }
}

impl Zone for NamedZone {
    fn date(&self, instant: DateTime<Utc>) -> NaiveDate {
        self.local(instant).date()
    }
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

/// An ISO instant or date; a date alone is its UTC midnight.
pub fn parse_instant(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(instant) = DateTime::parse_from_rfc3339(value) {
        return Some(instant.with_timezone(&Utc));
    }
    if value.len() == 10 {
        if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
            return Some(date.and_hms_opt(0, 0, 0)?.and_utc());
        }
    }
    None
}

/// The calendar day of an ISO reference in a zone.
pub fn calendar_date(reference: &str, zone: &dyn Zone) -> Option<NaiveDate> {
    parse_instant(reference).map(|instant| zone.date(instant))
}

/// A transaction's calendar day: an ISO date as written, or an RFC 2822 instant's UTC day.
pub fn transaction_date(value: &str) -> Option<NaiveDate> {
    if ISO_PREFIX.is_match(value) {
        return NaiveDate::parse_from_str(&value[..10], "%Y-%m-%d").ok();
    }
    DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|instant| instant.with_timezone(&Utc).date_naive())
}

/// Posted transactions dated through `today`, oldest first, and by id within a day.
pub fn posted_through(
    transactions: &[Transaction],
    today: NaiveDate,
) -> Vec<(NaiveDate, &Transaction)> {
    let mut posted = transactions
        .iter()
        .filter(|transaction| !transaction.pending)
        .filter_map(|transaction| {
            let date = transaction_date(transaction.posted_date())?;
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
        // Any IANA zone parses, including Arizona's, which keeps standard time all year.
        let phoenix = NamedZone::parse("America/Phoenix").unwrap();
        assert_eq!(
            phoenix.date(instant("2026-07-02T06:30:00Z")),
            date("2026-07-01")
        );
        assert!(NamedZone::parse("Etc/UTC").is_some());
        assert!(NamedZone::parse("Mars/Olympus_Mons").is_none());
    }

    #[test]
    fn local_times_skipped_or_repeated_by_daylight_saving_still_name_an_instant() {
        let new_york = NamedZone::NewYork;
        let local =
            |value: &str| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M").unwrap();
        let instant = |value: &str| DateTime::parse_from_rfc3339(value).unwrap().to_utc();
        // 2:30 a.m. on 2026-03-08 never happens in New York; it reads as 3:30 a.m. EDT.
        assert_eq!(
            new_york.instant(local("2026-03-08 02:30")),
            instant("2026-03-08T07:30:00Z")
        );
        // 1:30 a.m. on 2026-11-01 happens twice; the first is still daylight time.
        assert_eq!(
            new_york.instant(local("2026-11-01 01:30")),
            instant("2026-11-01T05:30:00Z")
        );
        assert_eq!(
            new_york.local(instant("2026-11-01T06:30:00Z")),
            local("2026-11-01 01:30")
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
    fn transaction_dates_are_iso_or_rfc_2822_days_only() {
        assert_eq!(
            transaction_date("2026-09-01T10:00:00Z"),
            Some(date("2026-09-01"))
        );
        assert_eq!(
            transaction_date("Tue, 1 Sep 2026 23:30:00 -0700"),
            Some(date("2026-09-02"))
        );
        // A month and day without a year names no particular day.
        assert_eq!(transaction_date("Dec 31"), None);
        assert_eq!(parse_instant("Aug 21"), None);
        assert_eq!(transaction_date("unknown"), None);
    }
}

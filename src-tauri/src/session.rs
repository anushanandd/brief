//! One definition of a day for everything about the market: the New York trading session.
//! Instants become session dates in New York time, never UTC or the Mac's zone, so a story, a
//! bar, a trade, and the summary's period all agree on which day they belong to. Pre-market and
//! after-hours are explicit states of the clock; their prices never stand in for a session's
//! close.
//!
//! Holidays and early closes follow the NYSE's published rules, computed rather than fetched.
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc, Weekday};
use serde::Serialize;

use crate::insights::calendar::{last_weekday, nth_weekday, NamedZone};

const NEW_YORK: NamedZone = NamedZone::NewYork;

/// The wall-clock time in New York at `instant`.
pub fn new_york(instant: DateTime<Utc>) -> NaiveDateTime {
    instant.naive_utc() + Duration::hours(NEW_YORK.offset_hours(instant))
}

pub fn new_york_date(instant: DateTime<Utc>) -> NaiveDate {
    new_york(instant).date()
}

/// The instant New York's clock reads `time` on `date`.
pub fn at_new_york(date: NaiveDate, time: NaiveTime) -> DateTime<Utc> {
    let local = date.and_time(time);
    // Offsets differ only across the 2 a.m. changes, when no session is open.
    let guess = local.and_utc() + Duration::hours(5);
    (local - Duration::hours(NEW_YORK.offset_hours(guess))).and_utc()
}

/// An RFC 3339 instant, or `None`.
pub fn instant(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

/// The New York date of an instant, or a bare `YYYY-MM-DD` date as written (trades and
/// dividends are posted on a day, not at a time).
pub fn date_of(value: &str) -> Option<NaiveDate> {
    match instant(value) {
        Some(at) => Some(new_york_date(at)),
        None => value
            .get(..10)
            .filter(|_| value.len() == 10)
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()),
    }
}

fn hm(hour: u32, minute: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(hour, minute, 0).expect("valid time")
}

/// A fixed-date holiday moved to the nearest weekday; New Year's Day on a Saturday is not moved
/// back into the old year.
fn observed(date: NaiveDate) -> Option<NaiveDate> {
    match date.weekday() {
        Weekday::Sat if date.month() == 1 && date.day() == 1 => None,
        Weekday::Sat => date.pred_opt(),
        Weekday::Sun => date.succ_opt(),
        _ => Some(date),
    }
}

/// Easter Sunday in the Gregorian calendar (the anonymous algorithm).
fn easter(year: i32) -> NaiveDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(year, month as u32, day as u32).expect("Easter is a valid date")
}

fn ymd(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("fixed date")
}

/// NYSE full-day closures.
pub fn is_market_holiday(date: NaiveDate) -> bool {
    let year = date.year();
    let mut holidays = vec![
        nth_weekday(year, 1, Weekday::Mon, 3),
        nth_weekday(year, 2, Weekday::Mon, 3),
        easter(year) - Duration::days(2),
        last_weekday(year, 5, Weekday::Mon),
        nth_weekday(year, 9, Weekday::Mon, 1),
        nth_weekday(year, 11, Weekday::Thu, 4),
    ];
    holidays.extend(
        [ymd(year, 1, 1), ymd(year, 7, 4), ymd(year, 12, 25)]
            .into_iter()
            .filter_map(observed),
    );
    if year >= 2022 {
        holidays.extend(observed(ymd(year, 6, 19)));
    }
    holidays.contains(&date)
}

pub fn is_trading_day(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun) && !is_market_holiday(date)
}

/// The NYSE closes at 1 p.m. on July 3, the day after Thanksgiving, and Christmas Eve, when
/// each is a session (not a weekend, or the observed holiday itself).
pub fn closes_early(date: NaiveDate) -> bool {
    let year = date.year();
    let thanksgiving = nth_weekday(year, 11, Weekday::Thu, 4);
    is_trading_day(date)
        && (date == ymd(year, 7, 3)
            || date == thanksgiving + Duration::days(1)
            || date == ymd(year, 12, 24))
}

/// One regular New York trading session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Session {
    pub date: NaiveDate,
}

/// Where the clock stands relative to the regular session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MarketState {
    /// 4:00 to 9:30 on a trading day: trades print, but the session hasn't opened.
    PreMarket,
    Open,
    /// From the close to 8:00 p.m.: the session's close is final.
    AfterHours,
    Closed,
}

impl Session {
    /// The session on `date`, when the market trades that day.
    pub fn on(date: NaiveDate) -> Option<Self> {
        is_trading_day(date).then_some(Self { date })
    }

    /// The latest session on or before `date`.
    pub fn on_or_before(mut date: NaiveDate) -> Self {
        while !is_trading_day(date) {
            date = date.pred_opt().expect("date in range");
        }
        Self { date }
    }

    /// The first session on or after `date`.
    pub fn on_or_after(mut date: NaiveDate) -> Self {
        while !is_trading_day(date) {
            date = date.succ_opt().expect("date in range");
        }
        Self { date }
    }

    pub fn previous(self) -> Self {
        Self::on_or_before(self.date.pred_opt().expect("date in range"))
    }

    pub fn next(self) -> Self {
        Self::on_or_after(self.date.succ_opt().expect("date in range"))
    }

    pub fn open(self) -> DateTime<Utc> {
        at_new_york(self.date, hm(9, 30))
    }

    pub fn close(self) -> DateTime<Utc> {
        at_new_york(
            self.date,
            if closes_early(self.date) {
                hm(13, 0)
            } else {
                hm(16, 0)
            },
        )
    }

    /// When extended-hours trading begins: 4:00 a.m. New York time.
    pub fn pre_market(self) -> DateTime<Utc> {
        at_new_york(self.date, hm(4, 0))
    }

    /// The latest session that has opened by `now`: today's once the bell rings, otherwise the
    /// one before. Pre-market prices never make today's session.
    pub fn latest(now: DateTime<Utc>) -> Self {
        let today = Self::on_or_before(new_york_date(now));
        if today.date == new_york_date(now) && now < today.open() {
            today.previous()
        } else {
            today
        }
    }

    /// The latest session whose close was at least `settle` before `now`: the one a close
    /// job can record once its prices have settled.
    pub fn latest_settled(now: DateTime<Utc>, settle: Duration) -> Self {
        let latest = Self::latest(now);
        if now >= latest.close() + settle {
            latest
        } else {
            latest.previous()
        }
    }

    /// The first session whose close comes after `at`: the one whose prices can react to news
    /// published then. A story after the close is priced the next session.
    pub fn reacting_to(at: DateTime<Utc>) -> Self {
        let session = Self::on_or_after(new_york_date(at));
        if at >= session.close() {
            session.next()
        } else {
            session
        }
    }

    /// Where `now` stands relative to this session's regular hours.
    pub fn state(now: DateTime<Utc>) -> MarketState {
        let date = new_york_date(now);
        let Some(today) = Self::on(date) else {
            return MarketState::Closed;
        };
        let late = at_new_york(date, hm(20, 0));
        if now < today.pre_market() {
            MarketState::Closed
        } else if now < today.open() {
            MarketState::PreMarket
        } else if now < today.close() {
            MarketState::Open
        } else if now < late {
            MarketState::AfterHours
        } else {
            MarketState::Closed
        }
    }
}

/// The clock a summary was made at: the session it describes and where the market stands.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionClock {
    /// The latest session that has opened.
    pub date: NaiveDate,
    /// The New York date at the time, so "today" means the session is today's.
    pub today: NaiveDate,
    pub state: MarketState,
    /// The session's close, final once it has passed.
    pub closes_at: String,
}

impl SessionClock {
    pub fn at(now: DateTime<Utc>) -> Self {
        let session = Session::latest(now);
        Self {
            date: session.date,
            today: new_york_date(now),
            state: Session::state(now),
            closes_at: session.close().to_rfc3339(),
        }
    }
}

/// Sessions after `start`, through `end`: a returns window in trading days. `end` is the
/// latest session that has opened; evidence from after it (weekend news) still belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Period {
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// The New York date when the period was asked for.
    pub through: NaiveDate,
}

impl Period {
    /// From `start`, written `YYYY-MM-DD`, through the latest session at `now`. A start on or
    /// after that session (a Monday morning's week that began Monday) begins the session
    /// before, so a period always holds a session.
    pub fn through_latest(start: &str, now: DateTime<Utc>) -> Result<Self, String> {
        let start = NaiveDate::parse_from_str(start, "%Y-%m-%d")
            .map_err(|_| "Choose a valid start date".to_string())?;
        let end = Session::latest(now);
        Ok(Self {
            start: start.min(end.previous().date),
            end: end.date,
            through: new_york_date(now).max(end.date),
        })
    }

    /// Whether an instant or posted date falls after the start's session, through the period.
    pub fn covers(&self, value: &str) -> bool {
        date_of(value).is_some_and(|day| day > self.start && day <= self.through)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str) -> DateTime<Utc> {
        instant(value).unwrap()
    }

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn nyse_holidays_and_early_closes_follow_the_published_rules() {
        // 2026: Good Friday April 3, Juneteenth Friday, July 4 on a Saturday closes Friday.
        for holiday in [
            "2026-01-01",
            "2026-01-19",
            "2026-02-16",
            "2026-04-03",
            "2026-05-25",
            "2026-06-19",
            "2026-07-03",
            "2026-09-07",
            "2026-11-26",
            "2026-12-25",
        ] {
            assert!(is_market_holiday(day(holiday)), "{holiday}");
        }
        // Columbus Day and Veterans Day are bank holidays, not market ones.
        assert!(is_trading_day(day("2026-10-12")));
        assert!(is_trading_day(day("2026-11-11")));
        // New Year's Day 2022 fell on a Saturday; Friday December 31 still traded.
        assert!(is_trading_day(day("2021-12-31")));
        assert!(closes_early(day("2026-11-27")));
        assert!(closes_early(day("2026-12-24")));
        // July 4, 2026 is a Saturday: Friday is closed, not short.
        assert!(!closes_early(day("2026-07-02")));
        assert!(closes_early(day("2025-07-03")));
        assert_eq!(
            Session::on(day("2026-11-27")).unwrap().close(),
            at("2026-11-27T18:00:00Z")
        );
    }

    #[test]
    fn a_pacific_evening_and_a_monday_morning_read_the_same_session() {
        // 9:30 p.m. Monday in Los Angeles is 12:30 a.m. Tuesday in New York (daylight time).
        let evening = at("2026-10-06T04:30:00Z");
        let pacific = NamedZone::LosAngeles;
        use crate::insights::calendar::Zone;
        assert_eq!(pacific.date(evening), day("2026-10-05"));
        assert_eq!(new_york_date(evening), day("2026-10-06"));
        assert_eq!(Session::latest(evening).date, day("2026-10-05"));
        assert_eq!(Session::state(evening), MarketState::Closed);

        // 8 a.m. Monday in New York: pre-market, and the latest session is Friday's.
        let morning = at("2026-10-05T12:00:00Z");
        assert_eq!(Session::latest(morning).date, day("2026-10-02"));
        assert_eq!(Session::state(morning), MarketState::PreMarket);
        // After the bell, Monday's.
        let open = at("2026-10-05T13:31:00Z");
        assert_eq!(Session::latest(open).date, day("2026-10-05"));
        assert_eq!(Session::state(open), MarketState::Open);
        assert_eq!(
            Session::state(at("2026-10-05T20:30:00Z")),
            MarketState::AfterHours
        );
        // Standard time: 9:30 a.m. is 14:30 UTC.
        assert_eq!(
            Session::on(day("2026-12-07")).unwrap().open(),
            at("2026-12-07T14:30:00Z")
        );
    }

    #[test]
    fn a_session_settles_half_an_hour_after_its_close() {
        let settle = Duration::minutes(30);
        // 4:15 p.m. Monday: Friday's session is the latest settled one; 4:31 p.m., Monday's.
        assert_eq!(
            Session::latest_settled(at("2026-10-05T20:15:00Z"), settle).date,
            day("2026-10-02")
        );
        assert_eq!(
            Session::latest_settled(at("2026-10-05T20:31:00Z"), settle).date,
            day("2026-10-05")
        );
        // A launch on Saturday records Friday's.
        assert_eq!(
            Session::latest_settled(at("2026-10-03T15:00:00Z"), settle).date,
            day("2026-10-02")
        );
    }

    #[test]
    fn news_is_priced_by_the_first_session_that_closes_after_it() {
        // A story at 5 p.m. Monday, which UTC dates Monday, is priced Tuesday.
        assert_eq!(
            Session::reacting_to(at("2026-10-05T21:00:00Z")).date,
            day("2026-10-06")
        );
        // A story at 9 p.m. Monday, which UTC dates Tuesday, is priced Tuesday too.
        assert_eq!(
            Session::reacting_to(at("2026-10-06T01:00:00Z")).date,
            day("2026-10-06")
        );
        // Midday news is priced that day; Saturday's on Monday.
        assert_eq!(
            Session::reacting_to(at("2026-10-05T16:00:00Z")).date,
            day("2026-10-05")
        );
        assert_eq!(
            Session::reacting_to(at("2026-10-03T16:00:00Z")).date,
            day("2026-10-05")
        );
        // An instant takes its New York date; a posted date stays as written.
        assert_eq!(date_of("2026-10-06T01:00:00Z"), Some(day("2026-10-05")));
        assert_eq!(date_of("2026-10-06"), Some(day("2026-10-06")));
        assert_eq!(date_of("October"), None);
    }

    #[test]
    fn a_period_runs_after_its_start_through_the_latest_session() {
        let monday_morning = at("2026-10-05T12:00:00Z");
        let period = Period::through_latest("2026-09-28", monday_morning).unwrap();
        assert_eq!(period.end, day("2026-10-02"));
        assert!(!period.covers("2026-09-28T15:00:00Z"));
        assert!(period.covers("2026-09-29"));
        // Weekend news belongs to the period though no session has priced it yet.
        assert!(period.covers("2026-10-04T15:00:00Z"));
        assert!(!period.covers("2026-10-06T15:00:00Z"));
        // A week that starts on the latest session still holds a session.
        let same = Period::through_latest("2026-10-02", monday_morning).unwrap();
        assert_eq!(same.start, day("2026-10-01"));
        assert_eq!(
            Period::through_latest("September", monday_morning).unwrap_err(),
            "Choose a valid start date"
        );
    }
}

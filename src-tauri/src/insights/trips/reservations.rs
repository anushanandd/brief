//! Flights and stays in the user's own calendar, as booking emails put them there: "Flight to
//! Chicago (UA 1811)" from the origin airport, "Stay at Example Hotel" at its address. Titles
//! that name only airport codes are skipped. Cities come from the bundled gazetteer.
use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;

use crate::event_calendar::Event;
use crate::places::{self, City};

#[derive(Clone, Debug, PartialEq)]
pub enum Reservation {
    /// Days are the Mac's local days, so a long flight can land a day early.
    Flight {
        departs: NaiveDate,
        arrives: NaiveDate,
        from: Option<&'static City>,
        to: &'static City,
    },
    /// The nights from check-in up to check-out. The address's town can name several cities;
    /// the one nearest the other evidence of those days is chosen later.
    Stay {
        from: NaiveDate,
        until: NaiveDate,
        cities: Vec<&'static City>,
    },
}

static FLIGHT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^Flight to (?P<city>[^(]+?)\s*\([A-Z0-9]{2}\s?\d{1,4}\)")
        .expect("the flight pattern is valid")
});
static STAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(Stay at |Stay: |Hotel: |[^:]+ Hotel: )").expect("the stay pattern is valid")
});
/// A trailing airport code or terminal in a flight's origin: "Oakland OAK".
static AIRPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s*(\([^)]*\)|\b[A-Z]{3})\s*$").expect("the airport pattern is valid")
});

/// Whether a calendar event can be a reservation, before its place is looked up.
pub fn named_like_one(title: &str) -> bool {
    FLIGHT.is_match(title) || STAY.is_match(title)
}

fn day(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.get(..10)?, "%Y-%m-%d").ok()
}

/// The most populous city of a name, unless the name is an airport code.
fn city(name: &str) -> Option<&'static City> {
    let name = name
        .split('/')
        .next()?
        .trim_matches(|character: char| character.is_whitespace() || character == '\u{200b}');
    let code = name.len() <= 3 && name.bytes().all(|byte| byte.is_ascii_uppercase());
    (!code)
        .then(|| places::cities_named(name).first().copied())
        .flatten()
}

pub fn read(events: &[Event]) -> Vec<Reservation> {
    let mut reservations = events
        .iter()
        .filter_map(|event| {
            let (start, end) = (day(&event.start)?, day(&event.end)?);
            if let Some(found) = FLIGHT.captures(&event.title) {
                let origin = event
                    .location
                    .as_deref()
                    .map(|location| AIRPORT.replace(location, "").into_owned());
                return Some(Reservation::Flight {
                    departs: start,
                    arrives: end.max(start),
                    from: origin.as_deref().and_then(city),
                    to: city(&found["city"])?,
                });
            }
            if !STAY.is_match(&event.title) {
                return None;
            }
            // The first part of an address is the street; a town follows, then perhaps a
            // region and a postal code with the country.
            let cities = event
                .location
                .as_deref()?
                .split(',')
                .skip(1)
                .flat_map(|part| {
                    let town = part
                        .chars()
                        .filter(|character| !character.is_ascii_digit())
                        .collect::<String>();
                    // A region code ("WA", "LU") also names some small town somewhere.
                    let town = town.trim();
                    if town.chars().count() < 4 {
                        return Vec::new();
                    }
                    places::cities_named(town)
                })
                .collect::<Vec<_>>();
            (!cities.is_empty()).then(|| Reservation::Stay {
                from: start,
                until: end.max(start),
                cities,
            })
        })
        .collect::<Vec<_>>();
    reservations.dedup();
    reservations
}

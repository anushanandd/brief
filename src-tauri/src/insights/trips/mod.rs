//! Trips: runs of days away from home.
//!
//! `days` works out where the user was each day, from in-person purchases, Wallet's record of
//! where the phone was, and the flights and stays in the user's own calendar, and which places
//! were bases (places lived for a stretch) at each time. A trip is a run of days away from
//! every base active then. It needs the phone or a reservation to place it, or purchases from
//! two merchants, so one merchant's printed billing city never makes a trip. A day at a base
//! ends a trip; silence ends one only after `GAP_DAYS`.
//!
//! A trip claims its away purchases, the travel and transport charges without a place on its
//! days (rideshares, hotels billed online), and the flights and lodging booked ahead of it.
//! Amounts are summed in whole cents.
mod days;
pub mod reservations;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use ts_rs::TS;

use self::days::{Evidence, Named, Source};
use self::reservations::Reservation;
use super::calendar;
use crate::finance_contract::Transaction;
use crate::merchants;
use crate::numeric::{from_minor, minor_units, round2};
use crate::transaction_policy::Kind;

const GAP_DAYS: i64 = 7;
/// Farther than this from home, a trip was most likely flown.
const FLIGHT_KM: f64 = 800.0;
const TOP_CATEGORIES: usize = 3;
/// Flights and lodging bought this long before a trip were booked for it.
const BOOKED_DAYS: i64 = 60;
/// A place a reservation names within this of a purchase's is the same stop.
const SAME_STOP_KM: f64 = 50.0;

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct TripCategory {
    pub name: String,
    /// Negative for spending.
    pub amount: f64,
    /// Its share of the trip's spending, in percent.
    pub share: f64,
}

/// A year's trips at a glance, by the year each began.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct TripYear {
    pub year: i32,
    pub count: usize,
    /// Calendar days away, counting both ends of each trip.
    pub days: i64,
    /// What the trips spent, positive.
    pub spent: f64,
    pub abroad: usize,
    pub flown: usize,
    /// The longest trip's ID; the newest when tied.
    pub longest: String,
}

/// One calendar day of a trip, including the days without a purchase.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct TripDay {
    pub date: NaiveDate,
    /// Net spending that day, negative; zero on a day without a purchase.
    pub spent: f64,
    pub purchases: u32,
}

/// A flight or lodging charge made before the trip, for it.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct TripBooking {
    pub transaction_id: String,
    pub date: NaiveDate,
    /// How likely it was for this trip, from 0 to 1: higher booked closer to it, and when the
    /// calendar holds the trip's flights or stays.
    pub confidence: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Trip {
    pub id: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// Every day from `start` to `end`, in order, with what was spent on it there.
    pub days: Vec<TripDay>,
    /// `total` over the trip's days, negative, to the cent.
    pub per_day: f64,
    /// The most frequent city and region of the away purchases, or of the reservations when
    /// nothing was bought; the city and country abroad.
    pub destination: String,
    pub abroad: bool,
    /// Net spending, negative, with the bookings made ahead.
    pub total: f64,
    /// `total` split: flights, lodging, and everything else, each negative.
    pub getting_there: f64,
    pub staying: f64,
    pub on_the_ground: f64,
    /// The top categories, largest first.
    pub categories: Vec<TripCategory>,
    /// The spending outside the top categories, positive, and its share in percent.
    pub rest: f64,
    pub rest_share: f64,
    pub transaction_ids: Vec<String>,
    /// Flights and lodging bought before the trip, also in `transaction_ids`.
    pub bookings: Vec<TripBooking>,
    /// The latest evidence is still away from home, and recent.
    pub ongoing: bool,
    /// Most likely flown: abroad, far from home, a flight in the calendar, or an airline or
    /// airport charge on its days.
    pub flew: bool,
    /// Each city the trip reached, most purchases first, in the order of the first visit when
    /// tied. A city known only from a reservation has no purchases.
    pub stops: Vec<TripStop>,
    /// Who went, by person id, as saved by hand.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companions: Vec<String>,
}

/// A city a trip reached, at its centre or the bank's or phone's coordinates.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct TripStop {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub region: Option<String>,
    pub country: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lat: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lon: Option<f64>,
    pub purchases: u32,
    pub first: NaiveDate,
}

/// A place the user lived at for a stretch, such as one home and then another.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct TripBase {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub region: Option<String>,
    pub country: String,
    pub lat: f64,
    pub lon: f64,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Trips {
    /// Newest first.
    pub trips: Vec<Trip>,
    /// Each year with a trip, newest first.
    pub years: Vec<TripYear>,
    /// The bases, oldest first, when there was enough to find them.
    pub bases: Vec<TripBase>,
}

/// How many calendar days a trip covered, counting both ends.
fn day_count(trip: &Trip) -> i64 {
    (trip.end - trip.start).num_days() + 1
}

/// Each year's trips summed, from trips newest first.
fn years(trips: &[Trip]) -> Vec<TripYear> {
    let mut years: Vec<TripYear> = Vec::new();
    for trip in trips {
        let year = trip.start.year();
        if years.last().is_none_or(|last| last.year != year) {
            years.push(TripYear {
                year,
                count: 0,
                days: 0,
                spent: 0.0,
                abroad: 0,
                flown: 0,
                longest: trip.id.clone(),
            });
        }
        let entry = years.last_mut().expect("pushed above");
        let longest = trips.iter().find(|other| other.id == entry.longest);
        if longest.is_some_and(|longest| day_count(trip) > day_count(longest)) {
            entry.longest = trip.id.clone();
        }
        entry.count += 1;
        entry.days += day_count(trip);
        entry.spent = from_minor(minor_units(entry.spent) + minor_units(trip.total.abs()));
        entry.abroad += usize::from(trip.abroad);
        entry.flown += usize::from(trip.flew);
    }
    years
}

/// Everything bought on the trip, including purchases made for someone else, since where they
/// were made shows where the cardholder was.
fn spending(transaction: &Transaction) -> bool {
    transaction.classification.as_ref().is_some_and(|value| {
        matches!(value.kind, Kind::Expense | Kind::Reimbursement)
            || (value.kind == Kind::Owed && value.others_spending)
    })
}

/// Made in person, as `merchants` decides; airlines bill from their headquarters, and a
/// descriptor with a web address names the merchant's site.
fn in_person(transaction: &Transaction) -> bool {
    transaction
        .presence
        .or_else(|| merchants::presence(transaction))
        == Some(merchants::Presence::InPerson)
}

fn detail_starts(transaction: &Transaction, prefix: &str) -> bool {
    transaction
        .category_detail
        .as_deref()
        .is_some_and(|code| code.starts_with(prefix))
}

/// A flight or lodging charge made remotely, which can be booked weeks ahead.
fn booking(transaction: &Transaction) -> bool {
    !in_person(transaction)
        && (detail_starts(transaction, "TRAVEL_FLIGHTS")
            || detail_starts(transaction, "TRAVEL_LODGING"))
}

fn label(named: &Named<'_>, abroad: bool) -> Option<String> {
    if abroad {
        return Some(
            named
                .city
                .map(|city| format!("{city}, {}", named.country))
                .unwrap_or_else(|| named.country.to_owned()),
        );
    }
    match (named.city, named.region) {
        (Some(city), Some(region)) => Some(format!("{city}, {region}")),
        (Some(city), None) => Some(city.to_owned()),
        (None, Some(region)) => Some(region.to_owned()),
        (None, None) => None,
    }
}

/// A run of away days that qualified, before its bookings are known.
struct Run<'a> {
    start: NaiveDate,
    end: NaiveDate,
    away: Vec<Evidence<'a>>,
    ongoing: bool,
    members: BTreeSet<&'a str>,
}

impl Run<'_> {
    fn reserved(&self) -> bool {
        self.away
            .iter()
            .any(|piece| matches!(piece.source, Source::Reservation))
    }
}

pub fn trips(
    transactions: &[Transaction],
    reservations: &[Reservation],
    today: NaiveDate,
) -> Trips {
    let dated = transactions
        .iter()
        .filter(|transaction| spending(transaction))
        .filter_map(|transaction| {
            let date = calendar::transaction_date(&transaction.date)?;
            (date <= today).then_some((date, transaction))
        })
        .collect::<Vec<_>>();
    let mut evidence = dated
        .iter()
        .filter(|(_, transaction)| in_person(transaction))
        .filter_map(|(date, transaction)| {
            days::purchase(*date, transaction, transaction.place.as_ref()?)
        })
        .collect::<Vec<_>>();
    // A flight booked for next month says nothing yet.
    let reservations = reservations
        .iter()
        .filter(|reservation| match reservation {
            Reservation::Flight { departs, .. } => *departs <= today,
            Reservation::Stay { from, .. } => *from <= today,
        })
        .cloned()
        .collect::<Vec<_>>();
    let booked = days::reservations(&reservations, &evidence);
    evidence.extend(booked);
    let areas = days::areas(&evidence);
    let mut by_day = days::days(&evidence, &areas);
    let bases = days::bases(&by_day);
    if bases.is_empty() {
        return Trips::default();
    }
    days::flight_days(&reservations, &areas, &bases, &mut by_day);
    days::fill_stays(&reservations, &areas, &bases, &mut by_day);

    // The most common place each area's evidence names, for its label.
    let mut names = vec![BTreeMap::<Named<'_>, usize>::new(); areas.centres.len()];
    for (piece, &area) in evidence.iter().zip(&areas.of) {
        *names[area].entry(piece.named).or_default() += 1;
    }
    let name = |area: usize| {
        names[area]
            .iter()
            .max_by(|left, right| left.1.cmp(right.1).then(right.0.cmp(left.0)))
            .map(|(named, _)| *named)
    };
    let home = bases
        .iter()
        .max_by_key(|base| by_day.values().filter(|area| **area == base.area).count())
        .and_then(|base| name(base.area))
        .map_or("US", |named| named.country);

    let mut runs = days::away_runs(&by_day, &bases, GAP_DAYS)
        .into_iter()
        .filter_map(|(dates, closed)| {
            let (start, end) = (*dates.first()?, *dates.last()?);
            let away = evidence
                .iter()
                .zip(&areas.of)
                .filter(|(piece, &area)| {
                    piece.date >= start
                        && piece.date <= end
                        && !days::at_base(&bases, piece.date, area)
                })
                .map(|(piece, _)| *piece)
                .collect::<Vec<_>>();
            let merchants = away
                .iter()
                .filter_map(|piece| match piece.source {
                    Source::Purchase(transaction) => Some(transaction.merchant.to_lowercase()),
                    Source::Reservation => None,
                })
                .collect::<BTreeSet<_>>();
            if !away.iter().any(Evidence::strong) && merchants.len() < 2 {
                return None;
            }
            let mut members = away
                .iter()
                .filter_map(|piece| match piece.source {
                    Source::Purchase(transaction) => Some(transaction.id.as_str()),
                    Source::Reservation => None,
                })
                .collect::<BTreeSet<_>>();
            // Travel and transport bought online (rides, hotels billed online) belong to the
            // trip on its days; their descriptors name the merchant's headquarters.
            for (date, transaction) in &dated {
                if *date >= start
                    && *date <= end
                    && !in_person(transaction)
                    && matches!(transaction.category.as_str(), "Travel" | "Transport")
                {
                    members.insert(&transaction.id);
                }
            }
            Some(Run {
                start,
                end,
                away,
                ongoing: !closed && (today - end).num_days() <= GAP_DAYS,
                members,
            })
        })
        .collect::<Vec<_>>();

    // A booking belongs to the first trip after it, when that trip is near enough.
    let mut bookings = vec![Vec::<TripBooking>::new(); runs.len()];
    for (date, transaction) in &dated {
        if !booking(transaction)
            || runs
                .iter()
                .any(|run| run.members.contains(transaction.id.as_str()))
        {
            continue;
        }
        let Some(index) = runs.iter().position(|run| run.start > *date) else {
            continue;
        };
        let ahead = (runs[index].start - *date).num_days();
        if ahead > BOOKED_DAYS {
            continue;
        }
        let by_time = (1.0 - ahead as f64 / 90.0).clamp(0.3, 0.95);
        let confidence = if runs[index].reserved() {
            (by_time + 1.0) / 2.0
        } else {
            by_time
        };
        bookings[index].push(TripBooking {
            transaction_id: transaction.id.clone(),
            date: *date,
            confidence: round2(confidence),
        });
        runs[index].members.insert(&transaction.id);
    }
    for booked in &mut bookings {
        booked.sort_by_key(|booking| booking.date);
    }

    let context = Context {
        dated: &dated,
        bases: &bases,
        centres: &areas.centres,
        home,
        reservations: &reservations,
    };
    let mut trips = runs
        .iter()
        .zip(bookings)
        .map(|(run, booked)| trip(run, booked, &context))
        .collect::<Vec<_>>();
    trips.reverse();
    Trips {
        years: years(&trips),
        trips,
        bases: bases
            .iter()
            .map(|base| {
                let named = name(base.area);
                let (lat, lon) = areas.centres[base.area];
                TripBase {
                    city: named.and_then(|named| named.city).map(str::to_owned),
                    region: named.and_then(|named| named.region).map(str::to_owned),
                    country: named.map_or(home, |named| named.country).to_owned(),
                    lat,
                    lon,
                    start: base.start,
                    end: base.end,
                }
            })
            .collect(),
    }
}

struct Context<'a, 'b> {
    dated: &'b [(NaiveDate, &'a Transaction)],
    bases: &'b [days::Base],
    centres: &'b [(f64, f64)],
    home: &'b str,
    reservations: &'b [Reservation],
}

fn trip(run: &Run<'_>, bookings: Vec<TripBooking>, context: &Context<'_, '_>) -> Trip {
    let (start, end) = (run.start, run.end);
    let members = context
        .dated
        .iter()
        .filter(|(_, transaction)| run.members.contains(transaction.id.as_str()))
        .collect::<Vec<_>>();
    let mut by_category = BTreeMap::<String, i64>::new();
    let mut by_day = BTreeMap::<NaiveDate, (i64, u32)>::new();
    let (mut getting_there, mut staying, mut on_the_ground) = (0_i64, 0_i64, 0_i64);
    for (date, transaction) in &members {
        let on_trip = *date >= start && *date <= end;
        if on_trip {
            by_day.entry(*date).or_default().1 += 1;
        }
        for (category, amount) in transaction.parts() {
            let cents = minor_units(amount);
            *by_category.entry(category.to_owned()).or_default() += cents;
            if on_trip {
                by_day.entry(*date).or_default().0 += cents;
            }
            if detail_starts(transaction, "TRAVEL_FLIGHTS") {
                getting_there += cents;
            } else if detail_starts(transaction, "TRAVEL_LODGING") {
                staying += cents;
            } else {
                on_the_ground += cents;
            }
        }
    }
    let total = by_category.values().sum::<i64>();
    let day_count = (end - start).num_days() + 1;
    let days = (0..day_count)
        .map(|offset| {
            let date = start + Duration::days(offset);
            let (spent, purchases) = by_day.get(&date).copied().unwrap_or_default();
            TripDay {
                date,
                spent: from_minor(spent),
                purchases,
            }
        })
        .collect::<Vec<_>>();
    let per_day = (total as f64 / day_count as f64).round() / 100.0;
    let mut categories = by_category
        .into_iter()
        .filter(|(_, amount)| *amount < 0)
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));
    let top = categories
        .into_iter()
        .take(TOP_CATEGORIES)
        .collect::<Vec<_>>();
    // What the top categories leave of the trip's net spending, never below nothing.
    let rest = (total.abs() - top.iter().map(|(_, amount)| amount.abs()).sum::<i64>()).max(0);
    let share = |cents: i64| {
        if total == 0 {
            0.0
        } else {
            round2(cents.abs() as f64 / total.abs() as f64 * 100.0)
        }
    };

    let abroad = run
        .away
        .iter()
        .any(|piece| piece.named.country != context.home);
    // Distance from the bases the trip left from, or from any when none was active.
    let active = context
        .bases
        .iter()
        .filter(|base| base.start <= start && start <= base.end)
        .map(|base| context.centres[base.area])
        .collect::<Vec<_>>();
    let homes = if active.is_empty() {
        context
            .bases
            .iter()
            .map(|base| context.centres[base.area])
            .collect()
    } else {
        active
    };
    let far = run.away.iter().any(|piece| {
        homes
            .iter()
            .all(|home| days::distance_km(*home, piece.point) > FLIGHT_KM)
    });
    let near_trip =
        |date: NaiveDate| date >= start - Duration::days(1) && date <= end + Duration::days(1);
    // Bag and seat fees post on the travel days; tickets, bought weeks ahead, don't count.
    let airline = context.dated.iter().any(|(date, transaction)| {
        near_trip(*date) && detail_starts(transaction, "TRAVEL_FLIGHTS")
    });
    let at_airport = run.away.iter().any(|piece| match piece.source {
        Source::Purchase(transaction) => transaction
            .location
            .as_ref()
            .and_then(|location| location.address.as_deref())
            .is_some_and(|address| address.to_ascii_lowercase().contains("airport")),
        Source::Reservation => false,
    });
    let booked_flight = context.reservations.iter().any(|reservation| {
        matches!(reservation, Reservation::Flight { departs, arrives, .. }
            if near_trip(*departs) || near_trip(*arrives))
    });
    let flew = abroad || far || airline || at_airport || booked_flight;

    // Name the trip from what was bought there, or from its reservations when nothing was.
    let bought = run
        .away
        .iter()
        .filter(|piece| matches!(piece.source, Source::Purchase(_)))
        .collect::<Vec<_>>();
    let naming = if bought.is_empty() {
        run.away.iter().collect::<Vec<_>>()
    } else {
        bought
    };
    let mut places = BTreeMap::<String, usize>::new();
    for piece in naming {
        if let Some(place) = label(&piece.named, abroad) {
            *places.entry(place).or_default() += 1;
        }
    }
    let destination = places
        .iter()
        .max_by(|left, right| left.1.cmp(right.1).then(right.0.cmp(left.0)))
        .map(|(place, _)| place.clone())
        .unwrap_or_else(|| "Away".into());

    Trip {
        id: format!("trip:{start}:{end}"),
        start,
        end,
        days,
        per_day,
        destination,
        abroad,
        total: from_minor(total),
        getting_there: from_minor(getting_there),
        staying: from_minor(staying),
        on_the_ground: from_minor(on_the_ground),
        categories: top
            .into_iter()
            .map(|(name, amount)| TripCategory {
                name,
                amount: from_minor(amount),
                share: share(amount),
            })
            .collect(),
        rest: from_minor(rest),
        rest_share: share(rest),
        transaction_ids: run.members.iter().map(|id| (*id).to_owned()).collect(),
        bookings,
        ongoing: run.ongoing,
        flew,
        stops: stops(&run.away),
        companions: Vec::new(),
    }
}

/// Each city the evidence names: purchases counted, reservations adding the cities no purchase
/// was made near.
fn stops(away: &[Evidence<'_>]) -> Vec<TripStop> {
    let mut stops = Vec::<TripStop>::new();
    let mut ordered = away.to_vec();
    // Purchases first, so a reservation's city joins a purchase's stop nearby.
    ordered.sort_by_key(|piece| (matches!(piece.source, Source::Reservation), piece.date));
    for piece in &ordered {
        let bought = matches!(piece.source, Source::Purchase(_));
        let same = |stop: &TripStop| {
            if bought {
                stop.city.as_deref() == piece.named.city
                    && stop.region.as_deref() == piece.named.region
                    && stop.country == piece.named.country
            } else {
                stop.lat
                    .zip(stop.lon)
                    .is_some_and(|point| days::distance_km(point, piece.point) <= SAME_STOP_KM)
            }
        };
        match stops.iter_mut().find(|stop| same(stop)) {
            Some(stop) => {
                stop.purchases += u32::from(bought);
                stop.first = stop.first.min(piece.date);
            }
            None => stops.push(TripStop {
                city: piece.named.city.map(str::to_owned),
                region: piece.named.region.map(str::to_owned),
                country: piece.named.country.to_owned(),
                lat: Some(piece.point.0),
                lon: Some(piece.point.1),
                purchases: u32::from(bought),
                first: piece.date,
            }),
        }
    }
    stops.sort_by(|left, right| {
        right
            .purchases
            .cmp(&left.purchases)
            .then(left.first.cmp(&right.first))
    });
    stops
}

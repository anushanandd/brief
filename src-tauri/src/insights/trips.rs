//! Trips: runs of in-store spending away from home. Home is every region that holds a real
//! share of the year's days with in-store purchases across several months, so a commuter's
//! neighboring state is home too while even a long trip is not.
//! With coordinates on both sides, distance decides instead, so a trip within a large state
//! still counts. A trip is two or more away purchases no more than a few days apart, ending at
//! the first purchase back home; it also claims the travel and transport charges that carry no
//! place (rideshares, hotels billed online) on its days. Online shopping never counts.
//! Amounts are summed in whole cents.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use ts_rs::TS;

use super::calendar;
use crate::finance_contract::Transaction;
use crate::merchants;
use crate::numeric::{from_minor, median, minor_units, round2};
use crate::places::{self, Place};
use crate::transaction_policy::Kind;

/// A region with purchases on at least this share of days, in at least `HOME_MONTHS`
/// calendar months, is home.
const HOME_SHARE: f64 = 0.15;
const HOME_MONTHS: usize = 3;
/// Without this many located in-store purchases there is no home to be away from.
const HOME_MINIMUM: usize = 10;
const HOME_DAYS: i64 = 365;
/// Away purchases further apart than this are separate trips.
const GAP_DAYS: i64 = 3;
/// Beyond this distance from home a purchase is away, whatever its region.
const AWAY_KM: f64 = 160.0;
const HISTORY_DAYS: i64 = 730;
/// Farther than this from home, a trip was most likely flown.
const FLIGHT_KM: f64 = 800.0;
const TOP_CATEGORIES: usize = 3;

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

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Trip {
    pub id: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// Every day from `start` to `end`, in order.
    pub days: Vec<TripDay>,
    /// `total` over the trip's days, negative, to the cent.
    pub per_day: f64,
    /// The most frequent city and region of the away purchases, or the country abroad.
    pub destination: String,
    pub abroad: bool,
    /// Net spending, negative.
    pub total: f64,
    /// The top categories, largest first.
    pub categories: Vec<TripCategory>,
    /// The spending outside the top categories, positive, and its share in percent.
    pub rest: f64,
    pub rest_share: f64,
    pub transaction_ids: Vec<String>,
    /// The latest located purchase is still away from home.
    pub ongoing: bool,
    /// Most likely flown: abroad, far from home, or with an airline or airport charge on its days.
    pub flew: bool,
    /// Each city the away purchases were made in, most purchases first, in the order of the
    /// first visit when tied.
    pub stops: Vec<TripStop>,
    /// Who went, by person id, as saved by hand.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub companions: Vec<String>,
}

/// A city a trip's purchases were made in, at its centre or the bank's coordinates.
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

#[derive(Clone, Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Trips {
    /// Newest first.
    pub trips: Vec<Trip>,
    /// Each year with a trip, newest first.
    pub years: Vec<TripYear>,
    /// The home regions, as `region, country`, when there was enough to find them.
    pub home: Vec<String>,
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

fn region_key(place: &Place) -> Option<String> {
    let region = place.region.as_deref()?.trim();
    (!region.is_empty()).then(|| format!("{region}, {}", place.country))
}

fn coordinates(place: &Place) -> Option<(f64, f64)> {
    place.coordinates()
}

/// Great-circle distance in kilometres.
fn distance_km(left: (f64, f64), right: (f64, f64)) -> f64 {
    let (lat1, lon1) = (left.0.to_radians(), left.1.to_radians());
    let (lat2, lon2) = (right.0.to_radians(), right.1.to_radians());
    let a = ((lat2 - lat1) / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * 6_371.0 * a.sqrt().asin()
}

struct Home {
    regions: BTreeSet<String>,
    country: String,
    /// Each home region's centre, where enough of its purchases carry coordinates. After a
    /// move there are two homes, and near either one is home.
    centres: Vec<(String, (f64, f64))>,
}

impl Home {
    /// How far a place is from the nearest home centre.
    fn distance(&self, here: (f64, f64)) -> Option<f64> {
        self.centres
            .iter()
            .map(|(_, centre)| distance_km(here, *centre))
            .min_by(f64::total_cmp)
    }

    fn away(&self, place: &Place) -> Option<bool> {
        if let Some(distance) = coordinates(place).and_then(|here| self.distance(here)) {
            // A home region too thinly located for a centre still counts by name.
            let named = region_key(place).is_some_and(|region| {
                self.regions.contains(&region)
                    && !self.centres.iter().any(|(home, _)| *home == region)
            });
            return Some(distance > AWAY_KM && !named);
        }
        if place.country != self.country {
            return Some(true);
        }
        region_key(place).map(|region| !self.regions.contains(&region))
    }
}

/// Whether a purchase shows where the cardholder was: made in person, as `merchants` decides.
/// Airlines bill from their headquarters, and a descriptor with a web address names the
/// merchant's site.
fn presence(transaction: &Transaction) -> bool {
    transaction
        .presence
        .or_else(|| merchants::presence(transaction))
        == Some(merchants::Presence::InPerson)
}

/// Everything bought on the trip, including purchases made for someone else, since where they
/// were made shows where the cardholder was.
fn spending(transaction: &Transaction) -> bool {
    transaction.classification.as_ref().is_some_and(|value| {
        matches!(value.kind, Kind::Expense | Kind::Reimbursement)
            || (value.kind == Kind::Owed && value.others_spending)
    })
}

pub fn trips(transactions: &[Transaction], today: NaiveDate) -> Trips {
    let dated = transactions
        .iter()
        .filter(|transaction| spending(transaction))
        .filter_map(|transaction| {
            let date = calendar::transaction_date(&transaction.date)?;
            (date > today - Duration::days(HISTORY_DAYS) && date <= today)
                .then_some((date, transaction))
        })
        .collect::<Vec<_>>();
    // The descriptor's place first, then the provider's: see `places`.
    let resolved = dated
        .iter()
        .map(|(_, transaction)| {
            transaction
                .place
                .clone()
                .or_else(|| places::resolve(transaction))
        })
        .collect::<Vec<_>>();
    let located = dated
        .iter()
        .zip(&resolved)
        .filter(|((_, transaction), _)| presence(transaction))
        .filter_map(|((date, transaction), place)| Some((*date, *transaction, place.as_ref()?)))
        .collect::<Vec<_>>();

    let Some(home) = infer_home(&located, today) else {
        return Trips::default();
    };

    // Each located purchase as home or away, in date order.
    let mut marks = located
        .iter()
        .filter_map(|(date, transaction, location)| {
            Some((*date, *transaction, *location, home.away(location)?))
        })
        .collect::<Vec<_>>();
    marks.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.id.cmp(&right.1.id)));

    let (runs, ongoing_run) = away_runs(&marks);
    let last_run = runs.len().saturating_sub(1);

    let mut trips = runs
        .into_iter()
        .enumerate()
        .filter(|(_, run)| run.len() >= 2)
        .map(|(index, run)| trip(&run, &dated, &home, ongoing_run && index == last_run))
        .collect::<Vec<_>>();
    trips.reverse();
    Trips {
        years: years(&trips),
        trips,
        home: home.regions.into_iter().collect(),
    }
}

/// A dated in-store purchase and where it was made.
type Located<'a> = (NaiveDate, &'a Transaction, &'a Place);

/// Home from recent in-store purchases: the region with the most purchase days, and any other
/// region visited often enough over enough months, with the main region's country and each
/// region's centre.
fn infer_home(located: &[Located<'_>], today: NaiveDate) -> Option<Home> {
    let recent = located
        .iter()
        .filter(|(date, _, _)| *date > today - Duration::days(HOME_DAYS))
        .filter_map(|(date, _, location)| Some((*date, region_key(location)?, *location)))
        .collect::<Vec<_>>();
    if recent.len() < HOME_MINIMUM {
        return None;
    }
    let days = recent
        .iter()
        .map(|(date, _, _)| *date)
        .collect::<BTreeSet<_>>()
        .len();
    // Per region: the days and the calendar months it had purchases.
    let mut presence = BTreeMap::<&str, (BTreeSet<NaiveDate>, BTreeSet<i64>)>::new();
    for (date, region, _) in &recent {
        let entry = presence.entry(region.as_str()).or_default();
        entry.0.insert(*date);
        entry.1.insert(calendar::month_index(*date));
    }
    let main = presence
        .iter()
        .max_by(|left, right| {
            left.1
                 .0
                .len()
                .cmp(&right.1 .0.len())
                .then(right.0.cmp(left.0))
        })
        .map(|(region, _)| *region)?;
    let regions = presence
        .iter()
        .filter(|(region, (region_days, months))| {
            **region == main
                || (region_days.len() as f64 >= days as f64 * HOME_SHARE
                    && months.len() >= HOME_MONTHS)
        })
        .map(|(region, _)| (*region).to_owned())
        .collect::<BTreeSet<_>>();
    let main_country = recent
        .iter()
        .find(|(_, region, _)| region == main)
        .map(|(_, _, place)| place.country.clone())
        .unwrap_or_else(|| "US".into());
    let centres = regions
        .iter()
        .filter_map(|home| {
            let (mut lats, mut lons): (Vec<f64>, Vec<f64>) = recent
                .iter()
                .filter(|(_, region, _)| region == home)
                .filter_map(|(_, _, location)| coordinates(location))
                .unzip();
            (lats.len() >= HOME_MINIMUM / 2)
                .then(|| median(&mut lats).zip(median(&mut lons)))
                .flatten()
                .map(|centre| (home.clone(), centre))
        })
        .collect();
    Some(Home {
        regions,
        country: main_country,
        centres,
    })
}

/// Away purchases grouped into runs: a long gap or a later day back home ends a run. Returns
/// whether the last run is still going.
fn away_runs<'a>(
    marks: &[(NaiveDate, &'a Transaction, &'a Place, bool)],
) -> (Vec<Vec<Located<'a>>>, bool) {
    let mut runs: Vec<Vec<Located<'a>>> = Vec::new();
    let mut current: Vec<Located<'a>> = Vec::new();
    for (index, (date, transaction, place, away)) in marks.iter().enumerate() {
        if *away {
            if current
                .last()
                .is_some_and(|(last, _, _)| (*date - *last).num_days() > GAP_DAYS)
            {
                runs.push(std::mem::take(&mut current));
            }
            current.push((*date, transaction, place));
        } else if let Some((last, _, _)) = current.last().filter(|(last, _, _)| date > last) {
            // Back home on a later day ends the trip; a home purchase the same day doesn't, and
            // neither does a lone one between away purchases a few days apart, which is an
            // online merchant reporting its headquarters.
            let resumes = marks
                .get(index + 1)
                .is_some_and(|(next, _, _, away)| *away && (*next - *last).num_days() <= GAP_DAYS);
            if !resumes {
                runs.push(std::mem::take(&mut current));
            }
        }
    }
    let ongoing_run = !current.is_empty();
    if ongoing_run {
        runs.push(current);
    }
    (runs, ongoing_run)
}

/// A run of away purchases as a trip, with the placeless travel and transport bought during it.
fn trip(
    run: &[Located<'_>],
    dated: &[(NaiveDate, &Transaction)],
    home: &Home,
    ongoing: bool,
) -> Trip {
    let start = run[0].0;
    let end = run[run.len() - 1].0;
    let mut ids = run
        .iter()
        .map(|(_, transaction, _)| transaction.id.clone())
        .collect::<BTreeSet<_>>();
    for (date, transaction) in dated {
        // Travel and transport bought online (rides, hotels billed online) belong to the trip
        // on its days; their descriptors name the merchant's headquarters, not where it was.
        if *date >= start
            && *date <= end
            && !presence(transaction)
            && matches!(transaction.category.as_str(), "Travel" | "Transport")
        {
            ids.insert(transaction.id.clone());
        }
    }
    let members = dated
        .iter()
        .filter(|(_, transaction)| ids.contains(&transaction.id))
        .map(|(date, transaction)| (*date, *transaction))
        .collect::<Vec<_>>();
    let mut by_category = BTreeMap::<String, i64>::new();
    let mut by_day = BTreeMap::<NaiveDate, (i64, u32)>::new();
    for (date, transaction) in &members {
        let day = by_day.entry(*date).or_default();
        day.1 += 1;
        for (category, amount) in transaction.parts() {
            let cents = minor_units(amount);
            *by_category.entry(category.to_owned()).or_default() += cents;
            day.0 += cents;
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
                spent: spent as f64 / 100.0,
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
        .iter()
        .any(|(_, _, place)| place.country != home.country);
    let far = run
        .iter()
        .filter_map(|(_, _, location)| coordinates(location))
        .any(|place| {
            home.distance(place)
                .is_some_and(|distance| distance > FLIGHT_KM)
        });
    // Bag and seat fees post on the travel days; tickets, bought weeks ahead, don't count.
    let airline = dated.iter().any(|(date, transaction)| {
        *date >= start - Duration::days(1)
            && *date <= end + Duration::days(1)
            && transaction
                .category_detail
                .as_deref()
                .is_some_and(|code| code.starts_with("TRAVEL_FLIGHTS"))
    });
    let at_airport = run.iter().any(|(_, transaction, _)| {
        transaction
            .location
            .as_ref()
            .and_then(|location| location.address.as_deref())
            .is_some_and(|address| address.to_ascii_lowercase().contains("airport"))
    });
    let flew = abroad || far || airline || at_airport;
    let mut places = BTreeMap::<String, usize>::new();
    for (_, _, location) in run {
        let place = if abroad {
            location
                .city
                .clone()
                .map(|city| format!("{city}, {}", location.country))
                .unwrap_or_else(|| location.country.clone())
        } else {
            match (&location.city, &location.region) {
                (Some(city), Some(region)) => format!("{city}, {region}"),
                (Some(city), None) => city.clone(),
                (None, Some(region)) => region.clone(),
                (None, None) => continue,
            }
        };
        *places.entry(place).or_default() += 1;
    }
    let destination = places
        .iter()
        .max_by(|left, right| left.1.cmp(right.1).then(right.0.cmp(left.0)))
        .map(|(place, _)| place.clone())
        .unwrap_or_else(|| "Away".into());
    let mut stops = Vec::<TripStop>::new();
    for (date, _, place) in run {
        let same = |stop: &TripStop| {
            stop.city == place.city && stop.region == place.region && stop.country == place.country
        };
        match stops.iter_mut().find(|stop| same(stop)) {
            Some(stop) => stop.purchases += 1,
            None => stops.push(TripStop {
                city: place.city.clone(),
                region: place.region.clone(),
                country: place.country.clone(),
                lat: place.lat,
                lon: place.lon,
                purchases: 1,
                first: *date,
            }),
        }
    }
    stops.sort_by(|left, right| {
        right
            .purchases
            .cmp(&left.purchases)
            .then(left.first.cmp(&right.first))
    });
    Trip {
        id: format!("trip:{start}:{end}"),
        start,
        end,
        days,
        per_day,
        destination,
        abroad,
        total: total as f64 / 100.0,
        categories: top
            .into_iter()
            .map(|(name, amount)| TripCategory {
                name,
                amount: amount as f64 / 100.0,
                share: share(amount),
            })
            .collect(),
        rest: from_minor(rest),
        rest_share: share(rest),
        transaction_ids: ids.into_iter().collect(),
        ongoing,
        flew,
        stops,
        companions: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn purchase(
        id: &str,
        date: &str,
        amount: f64,
        category: &str,
        place: Option<Value>,
    ) -> Transaction {
        let mut value = json!({
            "id": id, "merchant": id, "category": category, "date": date, "amount": amount,
            "account": "Card", "accountId": "plaid:card", "pending": false, "description": null,
            "categorySource": "provider", "paymentChannel": if place.is_some() { "in store" } else { "online" },
            "classification": { "mark": "initial", "kind": "expense", "spending": true, "credit": false, "zelle": false }
        });
        if let Some(place) = place {
            value["location"] = place;
        }
        serde_json::from_value(value).unwrap()
    }

    fn at(city: &str, region: &str) -> Option<Value> {
        Some(json!({ "city": city, "region": region, "country": "US" }))
    }

    fn home_life() -> Vec<Transaction> {
        let mut life = (1..=12)
            .map(|index| {
                purchase(
                    &format!("home-{index}"),
                    &format!("2026-0{}-0{}", 1 + index % 8, 1 + index % 9),
                    -20.0,
                    "Groceries",
                    at("Oakland", "CA"),
                )
            })
            .collect::<Vec<_>>();
        // A neighboring region visited often is home too.
        for index in 0..3 {
            life.push(purchase(
                &format!("reno-{index}"),
                &format!("2026-0{}-15", 2 + index),
                -15.0,
                "Dining",
                at("Reno", "NV"),
            ));
        }
        life
    }

    #[test]
    fn groups_away_purchases_into_a_trip_with_its_placeless_travel() {
        let mut transactions = home_life();
        transactions.extend([
            purchase("hotel", "2026-09-03", -600.0, "Travel", None),
            purchase("ride", "2026-09-04", -32.0, "Transport", None),
            purchase("stream", "2026-09-04", -12.0, "Entertainment", None),
            purchase("tacos", "2026-09-03", -40.0, "Dining", at("Austin", "TX")),
            purchase(
                "boots",
                "2026-09-05",
                -210.0,
                "Shopping",
                at("Austin", "TX"),
            ),
            purchase("bbq", "2026-09-06", -55.0, "Dining", at("Lockhart", "TX")),
            purchase(
                "back",
                "2026-09-08",
                -30.0,
                "Groceries",
                at("Oakland", "CA"),
            ),
            // One purchase passing through is not a trip.
            purchase("gas", "2026-09-12", -45.0, "Transport", at("Fresno", "CA")),
        ]);
        let result = trips(&transactions, day("2026-09-20"));
        assert_eq!(result.home, ["CA, US", "NV, US"]);
        assert_eq!(result.trips.len(), 1);
        let trip = &result.trips[0];
        assert_eq!(
            (trip.start, trip.end),
            (day("2026-09-03"), day("2026-09-06"))
        );
        assert_eq!(trip.destination, "Austin, TX");
        assert_eq!(trip.total, -937.0);
        // Four days, each with its own spending, even one with nothing bought.
        assert_eq!(trip.per_day, -234.25);
        assert_eq!(trip.days.len(), 4);
        assert_eq!(trip.days.iter().map(|day| day.spent).sum::<f64>(), -937.0);
        assert_eq!(trip.days.iter().map(|day| day.purchases).sum::<u32>(), 5);
        assert_eq!(
            trip.transaction_ids,
            ["bbq", "boots", "hotel", "ride", "tacos"]
        );
        assert_eq!(
            trip.categories[0],
            TripCategory {
                name: "Travel".into(),
                amount: -600.0,
                share: 64.03,
            }
        );
        // Whatever the top categories leave closes the split.
        let ranked = trip
            .categories
            .iter()
            .map(|category| minor_units(category.amount.abs()))
            .sum::<i64>();
        assert_eq!(minor_units(trip.rest) + ranked, 93_700);
        // The year adds its trips.
        assert_eq!(
            result.years,
            [TripYear {
                year: 2026,
                count: 1,
                days: 4,
                spent: 937.0,
                abroad: 0,
                flown: 1,
                longest: trip.id.clone(),
            }]
        );
        assert!(!trip.ongoing && !trip.abroad);
        // Austin is over 800 km from Oakland, so the trip was most likely flown.
        assert!(trip.flew);
    }

    #[test]
    fn a_trip_with_a_flight_charge_counts_as_flown() {
        let mut transactions = home_life();
        let mut bags = purchase("bags", "2026-09-03", -35.0, "Travel", None);
        bags.category_detail = Some("TRAVEL_FLIGHTS".into());
        transactions.extend([
            bags,
            purchase("tacos", "2026-09-03", -40.0, "Dining", at("Austin", "TX")),
            purchase("bbq", "2026-09-06", -55.0, "Dining", at("Austin", "TX")),
            purchase(
                "back",
                "2026-09-08",
                -30.0,
                "Groceries",
                at("Oakland", "CA"),
            ),
        ]);
        let trip = &trips(&transactions, day("2026-09-20")).trips[0];
        assert!(trip.flew);
    }

    #[test]
    fn counts_purchases_made_for_someone_else() {
        let mut transactions = home_life();
        let mut dinner = purchase("dinner", "2026-09-04", -80.0, "Dining", at("Austin", "TX"));
        let owed = dinner.classification.as_mut().unwrap();
        crate::transaction_policy::owe(owed, -80.0);
        transactions.extend([
            dinner,
            purchase("tacos", "2026-09-03", -40.0, "Dining", at("Austin", "TX")),
            purchase(
                "back",
                "2026-09-08",
                -30.0,
                "Groceries",
                at("Oakland", "CA"),
            ),
        ]);
        let trip = &trips(&transactions, day("2026-09-20")).trips[0];
        assert_eq!(trip.transaction_ids, ["dinner", "tacos"]);
        assert_eq!(trip.total, -120.0);
    }

    fn charged(id: &str, date: &str, descriptor: &str) -> Transaction {
        let mut transaction = purchase(id, date, -25.0, "Dining", None);
        transaction.payment_channel = Some("in store".into());
        transaction.description = Some(descriptor.into());
        transaction
    }

    #[test]
    fn finds_a_trip_abroad_from_descriptors_when_the_provider_sends_no_location() {
        let mut transactions = home_life();
        transactions.extend([
            charged("berlin", "2026-09-26", "AplPay EXAMPLE CAFE BERLIN DE"),
            charged("munich", "2026-09-28", "AplPay SUMUP*EXAMPLE MUENCHEN DE"),
            // A subscription billed from home in the middle of the trip doesn't end it.
            charged(
                "software",
                "2026-09-29",
                "EXAMPLE SOFTWARE SUBSAN FRANCISCO CA",
            ),
            charged("zurich", "2026-10-01", "AplPay EXAMPLE KIOSK ZÜRICH CH"),
            charged(
                "grindelwald",
                "2026-10-02",
                "AplPay EXAMPLE RAILWAY GRINDELWALD CH",
            ),
            purchase(
                "back",
                "2026-10-06",
                -30.0,
                "Groceries",
                at("Oakland", "CA"),
            ),
        ]);
        let result = trips(&transactions, day("2026-10-10"));
        assert_eq!(result.trips.len(), 1);
        let trip = &result.trips[0];
        assert_eq!(
            (trip.start, trip.end),
            (day("2026-09-26"), day("2026-10-02"))
        );
        assert!(trip.abroad && trip.flew);
        assert_eq!(
            trip.transaction_ids,
            ["berlin", "grindelwald", "munich", "zurich"]
        );
        // Each city is a stop, in the order first visited when the purchases tie.
        let stops = trip
            .stops
            .iter()
            .map(|stop| (stop.city.as_deref(), stop.country.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            stops,
            [
                (Some("Berlin"), "DE"),
                (Some("Munich"), "DE"),
                (Some("Zürich"), "CH"),
                (Some("Grindelwald"), "CH")
            ]
        );
    }

    #[test]
    fn needs_enough_history_to_know_home() {
        let transactions = [
            purchase("a", "2026-09-03", -40.0, "Dining", at("Austin", "TX")),
            purchase("b", "2026-09-04", -40.0, "Dining", at("Austin", "TX")),
        ];
        assert_eq!(trips(&transactions, day("2026-09-20")), Trips::default());
    }

    #[test]
    fn measures_distance_when_coordinates_exist_and_marks_a_trip_still_underway() {
        let mut transactions = (1..=12)
            .map(|index| {
                purchase(
                    &format!("home-{index}"),
                    &format!("2026-0{}-0{}", 1 + index % 8, 1 + index % 9),
                    -20.0,
                    "Groceries",
                    Some(json!({ "city": "Los Angeles", "region": "CA", "country": "US", "lat": 34.05, "lon": -118.24 })),
                )
            })
            .collect::<Vec<_>>();
        let sf = json!({ "city": "San Francisco", "region": "CA", "country": "US", "lat": 37.77, "lon": -122.42 });
        transactions.push(purchase(
            "sf-1",
            "2026-09-18",
            -80.0,
            "Dining",
            Some(sf.clone()),
        ));
        transactions.push(purchase("sf-2", "2026-09-19", -60.0, "Dining", Some(sf)));
        let result = trips(&transactions, day("2026-09-20"));
        assert_eq!(result.trips.len(), 1);
        assert_eq!(result.trips[0].destination, "San Francisco, CA");
        assert!(result.trips[0].ongoing);
        // Under 800 km from home with no airline charge: likely driven.
        assert!(!result.trips[0].flew);
        let nyc = json!({ "city": "New York", "region": "NY", "country": "US", "lat": 40.71, "lon": -74.0 });
        transactions.push(purchase(
            "ny-1",
            "2026-09-25",
            -30.0,
            "Dining",
            Some(nyc.clone()),
        ));
        transactions.push(purchase("ny-2", "2026-09-26", -30.0, "Dining", Some(nyc)));
        let far = trips(&transactions, day("2026-09-26"));
        assert!(far.trips[0].flew);
    }

    #[test]
    fn after_a_move_both_homes_are_home() {
        // Six months in Denver, then six in San Francisco, with a weekend in Chicago.
        let mut transactions = Vec::new();
        for month in 1..=12 {
            for day in [3, 9, 15, 21, 27] {
                let (city, region) = if month <= 6 {
                    ("Denver", "CO")
                } else {
                    ("San Francisco", "CA")
                };
                transactions.push(purchase(
                    &format!("home-{month}-{day}"),
                    &format!("2026-{month:02}-{day:02}"),
                    -20.0,
                    "Groceries",
                    at(city, region),
                ));
            }
        }
        for (id, date) in [("chicago-1", "2026-05-11"), ("chicago-2", "2026-05-12")] {
            transactions.push(purchase(id, date, -40.0, "Dining", at("Chicago", "IL")));
        }
        let result = trips(&transactions, day("2026-12-31"));
        assert_eq!(result.home, ["CA, US", "CO, US"]);
        assert_eq!(
            result
                .trips
                .iter()
                .map(|trip| trip.destination.as_str())
                .collect::<Vec<_>>(),
            ["Chicago, IL"]
        );
    }
}

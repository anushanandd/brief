//! Trips: runs of in-store spending away from home. Home is every region that holds a real
//! share of the year's days with in-store purchases across several months, so a commuter's
//! neighboring state is home too while even a long trip is not.
//! With coordinates on both sides, distance decides instead, so a trip within a large state
//! still counts. A trip is two or more away purchases no more than a few days apart, ending at
//! the first purchase back home; it also claims the travel and transport charges that carry no
//! place (rideshares, hotels billed online) on its days. Online shopping never counts.
//! Amounts are summed in whole cents.
use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use super::calendar::{self, Zone};
use crate::finance_contract::Transaction;
use crate::finance_contract::TransactionLocation;
use crate::numeric::{median, minor_units};
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
const TOP_CATEGORIES: usize = 3;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TripCategory {
    pub name: String,
    /// Negative for spending.
    pub amount: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Trip {
    pub id: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// The most frequent city and region of the away purchases, or the country abroad.
    pub destination: String,
    pub abroad: bool,
    /// Net spending, negative.
    pub total: f64,
    pub categories: Vec<TripCategory>,
    pub transaction_ids: Vec<String>,
    /// The latest located purchase is still away from home.
    pub ongoing: bool,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Trips {
    /// Newest first.
    pub trips: Vec<Trip>,
    /// The home regions, as `region, country`, when there was enough to find them.
    pub home: Vec<String>,
}

fn region_key(location: &TransactionLocation) -> Option<String> {
    let region = location.region.as_deref()?.trim().to_ascii_uppercase();
    let country = location
        .country
        .as_deref()
        .unwrap_or("US")
        .trim()
        .to_ascii_uppercase();
    (!region.is_empty()).then(|| format!("{region}, {country}"))
}

fn country(location: &TransactionLocation) -> String {
    location
        .country
        .as_deref()
        .unwrap_or("US")
        .trim()
        .to_ascii_uppercase()
}

fn coordinates(location: &TransactionLocation) -> Option<(f64, f64)> {
    location.lat.zip(location.lon)
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
    centre: Option<(f64, f64)>,
}

impl Home {
    fn away(&self, location: &TransactionLocation) -> Option<bool> {
        if let (Some(here), Some(centre)) = (coordinates(location), self.centre) {
            return Some(distance_km(here, centre) > AWAY_KM);
        }
        if country(location) != self.country && location.country.is_some() {
            return Some(true);
        }
        region_key(location).map(|region| !self.regions.contains(&region))
    }
}

fn in_store(transaction: &Transaction) -> bool {
    transaction.payment_channel.as_deref() == Some("in store")
}

fn spending(transaction: &Transaction) -> bool {
    matches!(
        transaction.kind(),
        Some(Kind::Expense | Kind::Reimbursement)
    )
}

pub fn trips(
    transactions: &[Transaction],
    reference: &str,
    today: NaiveDate,
    zone: &dyn Zone,
) -> Trips {
    let dated = transactions
        .iter()
        .filter(|transaction| spending(transaction))
        .filter_map(|transaction| {
            let date = calendar::transaction_date(&transaction.date, reference, zone)?;
            (date > today - Duration::days(HISTORY_DAYS) && date <= today)
                .then_some((date, transaction))
        })
        .collect::<Vec<_>>();
    let located = dated
        .iter()
        .filter(|(_, transaction)| in_store(transaction))
        .filter_map(|(date, transaction)| {
            Some((*date, *transaction, transaction.location.as_ref()?))
        })
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
        trips,
        home: home.regions.into_iter().collect(),
    }
}

/// A dated in-store purchase and where it was made.
type Located<'a> = (NaiveDate, &'a Transaction, &'a TransactionLocation);

/// Home from recent in-store purchases: the region with the most purchase days, and any other
/// region visited often enough over enough months, with the main region's country and centre.
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
        .map(|(_, _, location)| country(location))
        .unwrap_or_else(|| "US".into());
    let (mut lats, mut lons): (Vec<f64>, Vec<f64>) = recent
        .iter()
        .filter(|(_, region, _)| region == main)
        .filter_map(|(_, _, location)| coordinates(location))
        .unzip();
    Some(Home {
        regions,
        country: main_country,
        centre: (lats.len() >= HOME_MINIMUM / 2)
            .then(|| median(&mut lats).zip(median(&mut lons)))
            .flatten(),
    })
}

/// Away purchases grouped into runs: a long gap or a later day back home ends a run. Returns
/// whether the last run is still going.
fn away_runs<'a>(
    marks: &[(NaiveDate, &'a Transaction, &'a TransactionLocation, bool)],
) -> (Vec<Vec<Located<'a>>>, bool) {
    let mut runs: Vec<Vec<Located<'a>>> = Vec::new();
    let mut current: Vec<Located<'a>> = Vec::new();
    for (date, transaction, location, away) in marks {
        if *away {
            if current
                .last()
                .is_some_and(|(last, _, _)| (*date - *last).num_days() > GAP_DAYS)
            {
                runs.push(std::mem::take(&mut current));
            }
            current.push((*date, transaction, location));
        } else if current.last().is_some_and(|(last, _, _)| date > last) {
            // Back home on a later day ends the trip; a home purchase the same day doesn't.
            runs.push(std::mem::take(&mut current));
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
        let placeless = transaction.location.is_none() && !in_store(transaction);
        if *date >= start
            && *date <= end
            && placeless
            && matches!(transaction.category.as_str(), "Travel" | "Transport")
        {
            ids.insert(transaction.id.clone());
        }
    }
    let members = dated
        .iter()
        .filter(|(_, transaction)| ids.contains(&transaction.id))
        .map(|(_, transaction)| *transaction)
        .collect::<Vec<_>>();
    let mut by_category = BTreeMap::<String, i64>::new();
    for transaction in &members {
        for (category, amount) in transaction.parts() {
            *by_category.entry(category.to_owned()).or_default() += minor_units(amount);
        }
    }
    let total = by_category.values().sum::<i64>();
    let mut categories = by_category
        .into_iter()
        .filter(|(_, amount)| *amount < 0)
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));
    let abroad = run
        .iter()
        .any(|(_, _, location)| location.country.is_some() && country(location) != home.country);
    let mut places = BTreeMap::<String, usize>::new();
    for (_, _, location) in run {
        let place = if abroad {
            location
                .city
                .clone()
                .map(|city| format!("{city}, {}", country(location)))
                .unwrap_or_else(|| country(location))
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
    Trip {
        id: format!("trip:{start}:{end}"),
        start,
        end,
        destination,
        abroad,
        total: total as f64 / 100.0,
        categories: categories
            .into_iter()
            .take(TOP_CATEGORIES)
            .map(|(name, amount)| TripCategory {
                name,
                amount: amount as f64 / 100.0,
            })
            .collect(),
        transaction_ids: ids.into_iter().collect(),
        ongoing,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::insights::calendar::NamedZone;

    const REFERENCE: &str = "2026-09-20T12:00:00Z";

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
        let result = trips(&transactions, REFERENCE, day("2026-09-20"), &NamedZone::Utc);
        assert_eq!(result.home, ["CA, US", "NV, US"]);
        assert_eq!(result.trips.len(), 1);
        let trip = &result.trips[0];
        assert_eq!(
            (trip.start, trip.end),
            (day("2026-09-03"), day("2026-09-06"))
        );
        assert_eq!(trip.destination, "Austin, TX");
        assert_eq!(trip.total, -937.0);
        assert_eq!(
            trip.transaction_ids,
            ["bbq", "boots", "hotel", "ride", "tacos"]
        );
        assert_eq!(
            trip.categories[0],
            TripCategory {
                name: "Travel".into(),
                amount: -600.0
            }
        );
        assert!(!trip.ongoing && !trip.abroad);
    }

    #[test]
    fn needs_enough_history_to_know_home() {
        let transactions = [
            purchase("a", "2026-09-03", -40.0, "Dining", at("Austin", "TX")),
            purchase("b", "2026-09-04", -40.0, "Dining", at("Austin", "TX")),
        ];
        assert_eq!(
            trips(&transactions, REFERENCE, day("2026-09-20"), &NamedZone::Utc),
            Trips::default()
        );
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
        let result = trips(&transactions, REFERENCE, day("2026-09-20"), &NamedZone::Utc);
        assert_eq!(result.trips.len(), 1);
        assert_eq!(result.trips[0].destination, "San Francisco, CA");
        assert!(result.trips[0].ongoing);
    }
}

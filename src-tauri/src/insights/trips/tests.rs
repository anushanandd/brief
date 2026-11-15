use serde_json::json;

use super::*;
use crate::event_calendar::Event;
use crate::merchants::Presence;
use crate::places::{Place, PlacePrecision, PlaceSource};

fn day(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn at(city: &str, region: &str, country: &str, lat: f64, lon: f64) -> Place {
    Place {
        address: None,
        postal_code: None,
        city: Some(city.into()),
        region: Some(region.into()),
        country: country.into(),
        lat: Some(lat),
        lon: Some(lon),
        source: PlaceSource::Descriptor,
        precision: PlacePrecision::City,
    }
}

fn oakland() -> Place {
    at("Oakland", "CA", "US", 37.80, -122.27)
}
fn austin() -> Place {
    at("Austin", "TX", "US", 30.27, -97.74)
}

/// A purchase made in person at `place`, or online without one.
fn purchase(
    id: &str,
    date: &str,
    amount: f64,
    category: &str,
    place: Option<Place>,
) -> Transaction {
    let mut transaction: Transaction = serde_json::from_value(json!({
        "id": id, "merchant": id, "category": category, "date": date, "amount": amount,
        "account": "Card", "accountId": "plaid:card", "pending": false, "description": null,
        "categorySource": "provider",
        "paymentChannel": if place.is_some() { "in store" } else { "online" },
        "classification": { "mark": "initial", "kind": "expense", "spending": true, "credit": false, "zelle": false }
    }))
    .unwrap();
    transaction.presence = Some(if place.is_some() {
        Presence::InPerson
    } else {
        Presence::Remote
    });
    transaction.place = place;
    transaction
}

/// Groceries at home every week of 2026 through August.
fn home_life() -> Vec<Transaction> {
    (0..34)
        .map(|week| {
            let date = day("2026-01-02") + Duration::days(7 * week);
            purchase(
                &format!("home-{week}"),
                &date.to_string(),
                -20.0,
                "Groceries",
                Some(oakland()),
            )
        })
        .collect()
}

#[test]
fn groups_away_purchases_into_a_trip_with_its_placeless_travel() {
    let mut transactions = home_life();
    transactions.extend([
        purchase("hotel", "2026-09-03", -600.0, "Travel", None),
        purchase("ride", "2026-09-04", -32.0, "Transport", None),
        purchase("stream", "2026-09-04", -12.0, "Entertainment", None),
        purchase("tacos", "2026-09-03", -40.0, "Dining", Some(austin())),
        purchase("boots", "2026-09-05", -210.0, "Shopping", Some(austin())),
        purchase(
            "bbq",
            "2026-09-06",
            -55.0,
            "Dining",
            Some(at("Lockhart", "TX", "US", 29.88, -97.67)),
        ),
        purchase("back", "2026-09-08", -30.0, "Groceries", Some(oakland())),
        // One merchant's city, passing through, is not a trip.
        purchase(
            "gas",
            "2026-09-12",
            -45.0,
            "Transport",
            Some(at("Fresno", "CA", "US", 36.74, -119.79)),
        ),
    ]);
    let result = trips(&transactions, &[], day("2026-09-20"));
    assert_eq!(result.bases.len(), 1);
    assert_eq!(result.bases[0].city.as_deref(), Some("Oakland"));
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
    let ranked = trip
        .categories
        .iter()
        .map(|category| minor_units(category.amount.abs()))
        .sum::<i64>();
    assert_eq!(minor_units(trip.rest) + ranked, 93_700);
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
    let los_angeles = at("Los Angeles", "CA", "US", 34.05, -118.24);
    transactions.extend([
        bags,
        purchase(
            "tacos",
            "2026-09-03",
            -40.0,
            "Dining",
            Some(los_angeles.clone()),
        ),
        purchase("ramen", "2026-09-05", -55.0, "Dining", Some(los_angeles)),
        purchase("back", "2026-09-08", -30.0, "Groceries", Some(oakland())),
    ]);
    let trip = &trips(&transactions, &[], day("2026-09-20")).trips[0];
    assert!(trip.flew);
    assert_eq!(trip.getting_there, -35.0);
}

#[test]
fn counts_purchases_made_for_someone_else() {
    let mut transactions = home_life();
    let mut dinner = purchase("dinner", "2026-09-04", -80.0, "Dining", Some(austin()));
    crate::transaction_policy::owe(dinner.classification.as_mut().unwrap(), -80.0);
    transactions.extend([
        dinner,
        purchase("tacos", "2026-09-03", -40.0, "Dining", Some(austin())),
        purchase("back", "2026-09-08", -30.0, "Groceries", Some(oakland())),
    ]);
    let trip = &trips(&transactions, &[], day("2026-09-20")).trips[0];
    assert_eq!(trip.transaction_ids, ["dinner", "tacos"]);
    assert_eq!(trip.total, -120.0);
}

#[test]
fn one_merchants_city_is_not_a_trip_unless_the_phone_was_there() {
    let far = at("Plainview", "NY", "US", 40.78, -73.47);
    let mut transactions = home_life();
    transactions.extend([
        purchase(
            "wash",
            "2026-09-10",
            -1.5,
            "Personal Care",
            Some(far.clone()),
        ),
        purchase(
            "dry",
            "2026-09-10",
            -1.25,
            "Personal Care",
            Some(far.clone()),
        ),
    ]);
    for transaction in transactions.iter_mut().filter(|t| t.id == "dry") {
        transaction.merchant = "wash".into();
    }
    assert!(trips(&transactions, &[], day("2026-09-20"))
        .trips
        .is_empty());
    let mut placed = far;
    placed.source = PlaceSource::Device;
    transactions.push(purchase(
        "museum",
        "2026-09-25",
        -20.0,
        "Entertainment",
        Some(placed),
    ));
    let found = trips(&transactions, &[], day("2026-10-20")).trips;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].start, day("2026-09-25"));
}

#[test]
fn a_semester_away_is_a_base_and_its_weekend_trips_are_trips() {
    let sunnyvale = at("Sunnyvale", "CA", "US", 37.37, -122.04);
    let london = at("London", "England", "GB", 51.51, -0.13);
    let geneva = at("Geneva", "GE", "CH", 46.20, 6.14);
    let mut transactions = (0..30)
        .map(|index| {
            let date = day("2024-01-05") + Duration::days(8 * index);
            purchase(
                &format!("home-{index}"),
                &date.to_string(),
                -20.0,
                "Groceries",
                Some(sunnyvale.clone()),
            )
        })
        .collect::<Vec<_>>();
    for index in 0..45 {
        let date = day("2024-09-10") + Duration::days(2 * index);
        if (day("2024-10-03")..=day("2024-10-07")).contains(&date) {
            continue;
        }
        transactions.push(purchase(
            &format!("london-{index}"),
            &date.to_string(),
            -12.0,
            "Dining",
            Some(london.clone()),
        ));
    }
    transactions.extend([
        purchase(
            "fondue",
            "2024-10-04",
            -45.0,
            "Dining",
            Some(geneva.clone()),
        ),
        purchase("lake", "2024-10-05", -30.0, "Entertainment", Some(geneva)),
        purchase(
            "home again",
            "2024-12-20",
            -20.0,
            "Groceries",
            Some(sunnyvale),
        ),
    ]);
    let result = trips(&transactions, &[], day("2025-01-31"));
    let bases = result
        .bases
        .iter()
        .map(|base| base.city.as_deref().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(bases, ["Sunnyvale", "London"]);
    let found = result
        .trips
        .iter()
        .map(|trip| (trip.destination.as_str(), trip.start, trip.end))
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        [("Geneva, CH", day("2024-10-04"), day("2024-10-05"))]
    );
}

#[test]
fn after_a_move_the_old_home_stays_home_for_its_time() {
    let earlier = at("Salt Lake City", "UT", "US", 40.76, -111.89);
    let mut transactions = (0..110)
        .map(|index| {
            let date = day("2023-01-03") + Duration::days(5 * index);
            purchase(
                &format!("earlier-{index}"),
                &date.to_string(),
                -20.0,
                "Groceries",
                Some(earlier.clone()),
            )
        })
        .collect::<Vec<_>>();
    transactions.extend((0..60).map(|index| {
        let date = day("2024-07-01") + Duration::days(5 * index);
        purchase(
            &format!("home-{index}"),
            &date.to_string(),
            -20.0,
            "Groceries",
            Some(oakland()),
        )
    }));
    // Long after the move, the years at the earlier base are still not a trip.
    let result = trips(&transactions, &[], day("2026-11-30"));
    assert_eq!(result.bases.len(), 2);
    assert!(result.trips.is_empty());
}

#[test]
fn silence_ends_a_trip_only_after_a_week() {
    let mut transactions = home_life();
    transactions.extend([
        purchase(
            "shells",
            "2026-09-01",
            -36.0,
            "Shopping",
            Some(at("Honolulu", "HI", "US", 21.31, -157.86)),
        ),
        purchase(
            "park",
            "2026-09-07",
            -52.0,
            "Entertainment",
            Some(at("Hilo", "HI", "US", 19.72, -155.08)),
        ),
        purchase("back", "2026-09-09", -30.0, "Groceries", Some(oakland())),
    ]);
    let found = trips(&transactions, &[], day("2026-09-20")).trips;
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].start, found[0].end),
        (day("2026-09-01"), day("2026-09-07"))
    );
    assert_eq!(found[0].stops.len(), 2);
}

#[test]
fn a_trip_is_ongoing_only_while_recent() {
    let mut transactions = home_life();
    transactions.extend([
        purchase("tacos", "2026-11-01", -40.0, "Dining", Some(austin())),
        purchase("boots", "2026-11-02", -90.0, "Shopping", Some(austin())),
    ]);
    assert!(trips(&transactions, &[], day("2026-11-05")).trips[0].ongoing);
    assert!(!trips(&transactions, &[], day("2026-11-20")).trips[0].ongoing);
}

#[test]
fn needs_enough_history_to_know_home() {
    let transactions = [
        purchase("a", "2026-09-03", -40.0, "Dining", Some(austin())),
        purchase("b", "2026-09-04", -40.0, "Dining", Some(austin())),
    ];
    assert_eq!(
        trips(&transactions, &[], day("2026-09-20")),
        Trips::default()
    );
}

#[test]
fn flights_and_lodging_booked_ahead_join_the_next_trip() {
    let booked = |id: &str, date: &str, amount: f64, detail: &str| {
        let mut transaction = purchase(id, date, amount, "Travel", None);
        transaction.category_detail = Some(detail.into());
        transaction
    };
    let mut transactions = home_life();
    transactions.extend([
        booked("flight", "2026-08-14", -300.0, "TRAVEL_FLIGHTS"),
        booked("hotel", "2026-07-20", -500.0, "TRAVEL_LODGING"),
        booked("membership", "2026-08-01", -200.0, "TRAVEL_OTHER_TRAVEL"),
        booked("too early", "2026-06-01", -250.0, "TRAVEL_FLIGHTS"),
        purchase("tacos", "2026-09-03", -40.0, "Dining", Some(austin())),
        purchase("boots", "2026-09-05", -60.0, "Shopping", Some(austin())),
        purchase("back", "2026-09-08", -30.0, "Groceries", Some(oakland())),
    ]);
    let trip = &trips(&transactions, &[], day("2026-09-20")).trips[0];
    let bookings = trip
        .bookings
        .iter()
        .map(|booking| (booking.transaction_id.as_str(), booking.confidence))
        .collect::<Vec<_>>();
    assert_eq!(bookings, [("hotel", 0.5), ("flight", 0.78)]);
    assert_eq!(trip.total, -900.0);
    assert_eq!(
        (trip.getting_there, trip.staying, trip.on_the_ground),
        (-300.0, -500.0, -100.0)
    );
    // What was spent on the trip's days leaves the bookings out.
    assert_eq!(trip.days.iter().map(|day| day.spent).sum::<f64>(), -100.0);
}

fn event(title: &str, start: &str, end: &str, location: Option<&str>) -> Event {
    Event {
        id: title.into(),
        calendar: "someone@example.com".into(),
        title: title.into(),
        start: start.into(),
        end: end.into(),
        all_day: false,
        color: None,
        url: None,
        notes: None,
        location: location.map(str::to_owned),
        attendees: Vec::new(),
    }
}

#[test]
fn reads_flights_and_stays_from_their_titles() {
    let read = reservations::read(&[
        event(
            "Flight to Berlin/\u{200b}Brandenburg (LX 978)",
            "2026-09-24T08:00:00-07:00",
            "2026-09-24T10:00:00-07:00",
            Some("Zurich ZRH"),
        ),
        event(
            "Flight to SFO",
            "2026-10-06T08:00:00-07:00",
            "2026-10-06T10:00:00-07:00",
            None,
        ),
        event(
            "Stay at Example Inn",
            "2026-10-01",
            "2026-10-03",
            Some("Höheweg 95, Interlaken, 3800 Switzerland"),
        ),
        event(
            "Team lunch",
            "2026-10-02T12:00:00-07:00",
            "2026-10-02T13:00:00-07:00",
            None,
        ),
    ]);
    assert_eq!(read.len(), 2);
    let Reservation::Flight { from, to, .. } = &read[0] else {
        panic!("a flight");
    };
    assert_eq!((to.name.as_str(), to.country.as_str()), ("Berlin", "DE"));
    assert_eq!(from.map(|city| city.country.as_str()), Some("CH"));
    let Reservation::Stay {
        from,
        until,
        cities,
    } = &read[1]
    else {
        panic!("a stay");
    };
    assert_eq!((*from, *until), (day("2026-10-01"), day("2026-10-03")));
    assert!(cities
        .iter()
        .any(|city| city.name == "Interlaken" && city.country == "CH"));
}

#[test]
fn a_reservation_places_a_trip_nothing_was_bought_on() {
    let mut transactions = home_life();
    transactions.push(purchase(
        "back",
        "2026-09-20",
        -30.0,
        "Groceries",
        Some(oakland()),
    ));
    let read = reservations::read(&[
        event(
            "Flight to Lima (XX 101)",
            "2026-09-01T22:00:00-07:00",
            "2026-09-03T06:00:00-07:00",
            Some("Oakland OAK"),
        ),
        event(
            "Flight to Oakland (XX 102)",
            "2026-09-12T01:00:00-07:00",
            "2026-09-12T20:00:00-07:00",
            Some("Lima LIM"),
        ),
        event(
            "Stay at Example Inn",
            "2026-09-14",
            "2026-09-16",
            Some("1 Main St, Seattle, WA 98101"),
        ),
    ]);
    let result = trips(&transactions, &read, day("2026-09-30"));
    let found = result
        .trips
        .iter()
        .map(|trip| (trip.destination.as_str(), trip.start, trip.end, trip.flew))
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        [
            ("Seattle, WA", day("2026-09-14"), day("2026-09-15"), true),
            // The flights bound the trip, and between them every day was there.
            ("Lima, PE", day("2026-09-01"), day("2026-09-12"), true),
        ]
    );
    // A flight booked for later says nothing yet.
    assert!(trips(&transactions, &read, day("2026-08-30"))
        .trips
        .is_empty());
}

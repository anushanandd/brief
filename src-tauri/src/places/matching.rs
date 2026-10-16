//! Which business an in-person purchase was made at, from Overture's places near it. A match
//! must be unambiguous, or there is none and the purchase keeps the place its records give:
//!
//! - At the issuer's street address, the one business there whose name is the brand the
//!   records name.
//! - In a city, without an address, the one location of a business outside any chain whose
//!   name spells out the descriptor's.
//!
//! Offices never match a purchase (a chain's head office, a billing address), and a place
//! listed twice counts once.
use std::collections::BTreeMap;

use super::{
    lookup, normalize,
    overture::{Area, OverturePlace},
    Business, PlacePrecision, PlaceSource,
};
use crate::{
    finance_contract::Transaction,
    merchants::{self, Presence},
};

/// Categories that are never where a purchase was made.
const OFFICES: [&str; 9] = [
    "office",
    "corporate",
    "headquarters",
    "legal",
    "real_estate",
    "property_management",
    "b2b",
    "supplier",
    "distribution",
];
/// Below this, Overture doubts the place exists.
const MINIMUM_CONFIDENCE: f64 = 0.3;
/// Two listings of one name this close are one place.
const SAME_PLACE_METRES: f64 = 150.0;
/// A name shorter than this, run together, is too common to find in a city by name alone.
const CITY_NAME_LETTERS: usize = 6;
const DIRECTIONS: [&str; 12] = [
    "N", "S", "E", "W", "NE", "NW", "SE", "SW", "NORTH", "SOUTH", "EAST", "WEST",
];
/// Spelled several ways, so never the word a street is known by.
const STREET_TYPES: [&str; 16] = [
    "ST", "STREET", "AVE", "AVENUE", "RD", "ROAD", "BLVD", "DR", "DRIVE", "WAY", "LN", "PL", "CT",
    "PKWY", "PLZ", "HWY",
];

/// What to look for, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct Need {
    /// Identifies the search: the same purchase key always searches the same way.
    pub key: String,
    pub area: Area,
    street: Option<(String, String)>,
}

/// A street address's number and the word that names its street: the longer of the two after
/// any direction ("125 E EL CAMINO REAL" is 125 and CAMINO, "1 S VAN NESS AVE" 1 and NESS).
fn street(address: &str) -> Option<(String, String)> {
    let normalized = normalize(address);
    let mut words = normalized.split(' ');
    let number = words.next()?;
    if !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let name = words
        .filter(|word| !DIRECTIONS.contains(word) && !STREET_TYPES.contains(word))
        .take(2)
        .max_by_key(|word| word.len())?;
    Some((number.to_owned(), name.to_owned()))
}

/// Half the side of the box searched around a city's centre, wider for bigger cities.
fn reach(population: u32) -> f64 {
    match population {
        0..50_000 => 0.04,
        50_000..300_000 => 0.06,
        _ => 0.09,
    }
}

fn in_person(transaction: &Transaction) -> bool {
    transaction.presence == Some(Presence::InPerson)
}

/// The search a purchase needs, if any: one made in person, at the issuer's street address or
/// in the city the descriptor names. Plaid's location alone can be a head office, and a vending
/// processor's address is its own.
pub fn need(transaction: &Transaction) -> Option<Need> {
    if !in_person(transaction) || merchants::bills_elsewhere(transaction) {
        return None;
    }
    let place = transaction.place.as_ref()?;
    if place.source == PlaceSource::Bank {
        return None;
    }
    let city_name = place.city.as_deref()?;
    let city = lookup(city_name, &place.country, place.region.as_deref());
    let area = match city {
        Some(city) => Area::around(city.lat, city.lon, reach(city.population)),
        None => {
            let (lat, lon) = place.coordinates()?;
            Area::around(lat, lon, reach(0))
        }
    };
    let town = format!(
        "{}|{}|{}",
        place.country,
        place.region.as_deref().unwrap_or_default(),
        normalize(city_name)
    );
    if place.source == PlaceSource::Issuer {
        let (number, name) = street(place.address.as_deref()?)?;
        return Some(Need {
            key: format!("street|{town}|{number}|{name}"),
            area,
            street: Some((number, name)),
        });
    }
    let evidence = merchants::evidence(transaction);
    if evidence.concat().len() < CITY_NAME_LETTERS || merchants::numbered_store(transaction) {
        return None;
    }
    Some(Need {
        key: format!(
            "city|{town}|{}",
            evidence
                .iter()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        ),
        area,
        street: None,
    })
}

/// What to keep from an area for this search: the businesses at the address, or whose name
/// starts like the merchant's.
pub fn candidates(need: &Need, places: &[OverturePlace]) -> Vec<OverturePlace> {
    let first = need
        .key
        .rsplit('|')
        .next()
        .unwrap_or_default()
        .split(' ')
        .next();
    places
        .iter()
        .filter(|place| match &need.street {
            Some((number, name)) => street(place.address.as_deref().unwrap_or_default())
                .is_some_and(|(at, called)| &at == number && &called == name),
            None => first.is_some_and(|first| {
                let words = merchants::words(&place.name);
                words.first().is_some_and(|word| {
                    word.starts_with(first) || (first.starts_with(word.as_str()) && word.len() >= 3)
                })
            }),
        })
        .take(200)
        .cloned()
        .collect()
}

fn metres(left: &OverturePlace, right: &OverturePlace) -> f64 {
    let (lat1, lon1) = (left.lat.to_radians(), left.lon.to_radians());
    let (lat2, lon2) = (right.lat.to_radians(), right.lon.to_radians());
    let a = ((lat2 - lat1) / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * 6_371_000.0 * a.sqrt().asin()
}

/// The one business that fits, or none when nothing or more than one does.
pub fn choose(
    transaction: &Transaction,
    need: &Need,
    candidates: &[OverturePlace],
) -> Option<Business> {
    let evidence = merchants::evidence(transaction);
    let fits = candidates
        .iter()
        .filter(|place| place.confidence >= MINIMUM_CONFIDENCE)
        .filter(|place| {
            !place
                .category
                .as_deref()
                .is_some_and(|category| OFFICES.iter().any(|office| category.contains(office)))
        })
        .filter(|place| {
            let named = merchants::names(&merchants::words(&place.name), &evidence);
            // In a city, a chain's lone listing nearby may not be the only one, and a listing
            // in a neighbouring city is another store.
            let in_city = need.street.is_some()
                || (place.brand.is_none()
                    && place.locality.as_deref().is_none_or(|locality| {
                        need.key.split('|').nth(3) == Some(normalize(locality).as_str())
                    }));
            named && in_city
        })
        .collect::<Vec<_>>();
    // One entry per name, its listings collapsed when they are one place.
    let mut names = BTreeMap::<String, Vec<&OverturePlace>>::new();
    for place in fits {
        let listings = names
            .entry(merchants::words(&place.name).concat())
            .or_default();
        match listings
            .iter_mut()
            .find(|listing| metres(listing, place) <= SAME_PLACE_METRES)
        {
            Some(listing) if place.confidence > listing.confidence => *listing = place,
            Some(_) => {}
            None => listings.push(place),
        }
    }
    // A department or longer name ("Safeway Pharmacy") gives way to the name itself.
    if names.len() > 1 {
        names.retain(|_, listings| {
            merchants::names_exactly(&merchants::words(&listings[0].name), &evidence)
        });
    }
    let [(_, listings)] = Vec::from_iter(names).try_into().ok()?;
    let [place] = listings.try_into().ok()?;
    Some(Business {
        id: place.id.clone(),
        name: place.name.clone(),
        category: place.category.clone(),
        address: place.address.clone(),
        phone: place.phone.clone(),
        website: place.website.clone(),
        lat: place.lat,
        lon: place.lon,
        closed: place.closed,
    })
}

/// How far matching has got, for Settings.
#[derive(Clone, Debug, Default, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub in_person: usize,
    pub matched: usize,
    /// Purchase keys Brief can search for.
    pub searchable: usize,
    /// Of those, the ones not searched yet.
    pub unsearched: usize,
}

pub fn status(
    transactions: &[Transaction],
    searches: &BTreeMap<String, Vec<OverturePlace>>,
) -> Status {
    let keys = transactions
        .iter()
        .filter_map(need)
        .map(|need| need.key)
        .collect::<std::collections::BTreeSet<_>>();
    Status {
        in_person: transactions
            .iter()
            .filter(|transaction| in_person(transaction))
            .count(),
        matched: transactions
            .iter()
            .filter(|transaction| transaction.business.is_some())
            .count(),
        searchable: keys.len(),
        unsearched: keys
            .iter()
            .filter(|key| !searches.contains_key(*key))
            .count(),
    }
}

/// Matches each in-person purchase to its business from the saved searches, then takes the
/// business's coordinates, and its name where Plaid's was contradicted or unsure.
pub fn apply(transactions: &mut [Transaction], searches: &BTreeMap<String, Vec<OverturePlace>>) {
    for transaction in transactions {
        let Some(business) =
            need(transaction).and_then(|need| choose(transaction, &need, searches.get(&need.key)?))
        else {
            continue;
        };
        if let Some(place) = transaction.place.as_mut() {
            place.lat = Some(business.lat);
            place.lon = Some(business.lon);
            place.precision = PlacePrecision::Exact;
        }
        let plaid = merchants::plaid_named(transaction);
        if transaction.provider_merchant.is_some() || !plaid || !merchants::plaid_sure(transaction)
        {
            let shown = std::mem::replace(&mut transaction.merchant, business.name.clone());
            if plaid && transaction.provider_merchant.is_none() {
                transaction.provider_merchant = Some(shown);
            }
        }
        transaction.business = Some(business);
    }
}

#[cfg(test)]
mod tests;

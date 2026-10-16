//! Where a purchase was made: the bank's own descriptor first, the provider's location second.
//!
//! Card descriptors end in the merchant's city and a state or country code, often run together
//! with a truncated merchant name ("AplPay COUNTER POINTMENLO PARK CA"), so the city is the
//! longest known city name ending the text. Providers send no location for most purchases
//! abroad, and sometimes a chain's corporate address instead of the store's, while the
//! descriptor names the store's city. City names and coordinates come from GeoNames (CC BY 4.0),
//! bundled and read offline; `scripts/build-places.py` rebuilds them.
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    sync::LazyLock,
};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::finance_contract::{Transaction, TransactionLocation};

/// One city of the gazetteer.
pub struct City {
    pub name: String,
    pub country: String,
    /// The state for the US; the first-level region code elsewhere.
    pub admin1: String,
    pub lat: f64,
    pub lon: f64,
    pub population: u32,
}

struct Gazetteer {
    cities: Vec<City>,
    /// Normalized spelling to the cities that use it.
    spellings: HashMap<String, Vec<u32>>,
    countries: HashSet<String>,
}

static GAZETTEER: LazyLock<Gazetteer> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("places/cities.tsv.gz")[..])
        .read_to_string(&mut text)
        .expect("the bundled gazetteer is valid gzip");
    let mut gazetteer = Gazetteer {
        cities: Vec::new(),
        spellings: HashMap::new(),
        countries: HashSet::new(),
    };
    for line in text.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        let [name, country, admin1, lat, lon, population, spellings] = fields[..] else {
            continue;
        };
        let (Ok(lat), Ok(lon)) = (lat.parse(), lon.parse()) else {
            continue;
        };
        let index = gazetteer.cities.len() as u32;
        gazetteer.countries.insert(country.to_owned());
        gazetteer.cities.push(City {
            name: name.to_owned(),
            country: country.to_owned(),
            admin1: admin1.to_owned(),
            lat,
            lon,
            population: population.parse().unwrap_or(0),
        });
        for spelling in spellings.split('|') {
            gazetteer
                .spellings
                .entry(spelling.to_owned())
                .or_default()
                .push(index);
        }
    }
    gazetteer
});

const US_STATES: [&str; 52] = [
    "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL", "IN", "IA", "KS",
    "KY", "LA", "ME", "MD", "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV", "NH", "NJ", "NM", "NY",
    "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC", "SD", "TN", "TX", "UT", "VT", "VA", "WA", "WV",
    "WI", "WY", "DC", "PR",
];
/// Canadian province and territory codes, with GeoNames' code for each.
const PROVINCES: [(&str, &str); 13] = [
    ("AB", "01"),
    ("BC", "02"),
    ("MB", "03"),
    ("NB", "04"),
    ("NL", "05"),
    ("NS", "07"),
    ("ON", "08"),
    ("PE", "09"),
    ("QC", "10"),
    ("SK", "11"),
    ("YT", "12"),
    ("NT", "13"),
    ("NU", "14"),
];
/// Wallet prefixes ahead of the merchant name.
const PREFIXES: [&str; 2] = ["APLPAY ", "APPLE PAY "];
/// Where a bank's own trailer begins ("… LACHINE QC Transaction Date : 10/05/2026").
const TRAILERS: [&str; 1] = [" TRANSACTION DATE"];
/// A city name ending a descriptor with no state or country code is read only when no other
/// city shares it and it is at least this long.
const UNCODED_CITY_LETTERS: usize = 5;
/// Below this, a code that names neither a state nor a country can't vouch for a city.
const FALLBACK_POPULATION: u32 = 50_000;

/// A web address names the merchant's site, not where the purchase was made.
static WEB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)https?:|www\.|\.(?:com|net|org|io|co)\b").expect("valid pattern")
});

/// Whether a descriptor carries a web address, naming the merchant's site rather than where the
/// purchase was made.
pub fn names_a_website(descriptor: &str) -> bool {
    WEB.is_match(descriptor)
}

/// Uppercase letters and digits separated by single spaces, accents folded: the form the
/// gazetteer's spellings are stored in.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for character in text.chars() {
        let folded = fold(character);
        let alphanumeric = folded.chars().all(|c| c.is_ascii_alphanumeric()) && !folded.is_empty();
        if alphanumeric {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.extend(folded.chars().map(|c| c.to_ascii_uppercase()));
        } else {
            space = true;
        }
    }
    out
}

/// The unaccented letters of a Latin character; anything else unchanged.
fn fold(character: char) -> String {
    let base = match character {
        'à'..='å' | 'À'..='Å' | 'ā' | 'Ā' | 'ă' | 'Ă' | 'ą' | 'Ą' => "A",
        'ç' | 'Ç' | 'ć' | 'Ć' | 'č' | 'Č' => "C",
        'ď' | 'Ď' | 'đ' | 'Đ' => "D",
        'è'..='ë' | 'È'..='Ë' | 'ē' | 'Ē' | 'ę' | 'Ę' | 'ě' | 'Ě' => "E",
        'ì'..='ï' | 'Ì'..='Ï' | 'ī' | 'Ī' | 'ı' => "I",
        'ł' | 'Ł' | 'ľ' | 'Ľ' => "L",
        'ñ' | 'Ñ' | 'ń' | 'Ń' | 'ň' | 'Ň' => "N",
        'ò'..='ö' | 'Ò'..='Ö' | 'ø' | 'Ø' | 'ō' | 'Ō' | 'ő' | 'Ő' => "O",
        'ř' | 'Ř' => "R",
        'ś' | 'Ś' | 'š' | 'Š' | 'ş' | 'Ş' => "S",
        'ß' => "SS",
        'ť' | 'Ť' | 'ţ' | 'Ţ' => "T",
        'ù'..='ü' | 'Ù'..='Ü' | 'ū' | 'Ū' | 'ů' | 'Ů' | 'ű' | 'Ű' => "U",
        'ý' | 'ÿ' | 'Ý' => "Y",
        'ž' | 'Ž' | 'ź' | 'Ź' | 'ż' | 'Ż' => "Z",
        'æ' | 'Æ' => "AE",
        'œ' | 'Œ' => "OE",
        _ => return character.to_string(),
    };
    base.to_owned()
}

/// The longest known city name ending `text` among the cities `accept` allows, most populous
/// first. A long name may run into the merchant's (fixed-width descriptors drop the space); a
/// short one must stand as its own word.
fn city_at_end(text: &str, accept: impl Fn(&City) -> bool) -> Option<(&'static City, usize)> {
    let gazetteer = &*GAZETTEER;
    for length in (4..=text.len().min(30)).rev() {
        let start = text.len() - length;
        if !text.is_char_boundary(start) {
            continue;
        }
        let suffix = &text[start..];
        if suffix.starts_with(' ') || (length < 5 && start > 0 && &text[start - 1..start] != " ") {
            continue;
        }
        let best = gazetteer
            .spellings
            .get(suffix)
            .into_iter()
            .flatten()
            .map(|&index| &gazetteer.cities[index as usize])
            .filter(|city| accept(city))
            .max_by_key(|city| city.population);
        if let Some(best) = best {
            return Some((best, start));
        }
    }
    None
}

/// What a descriptor says about where it was made.
#[derive(Clone, Copy)]
pub struct DescriptorPlace {
    pub country: &'static str,
    pub region: Option<&'static str>,
    pub city: Option<&'static City>,
}

pub(crate) fn us_state(code: &str) -> Option<&'static str> {
    US_STATES.iter().copied().find(|state| *state == code)
}

/// The place a descriptor ends with, if it ends with a state or country code.
pub fn parse_descriptor(descriptor: &str) -> Option<DescriptorPlace> {
    read_descriptor(descriptor, false).1
}

/// The merchant part of a descriptor, normalized: the text before the place it ends with, without
/// a wallet prefix.
pub fn descriptor_merchant(descriptor: &str) -> String {
    read_descriptor(descriptor, false).0
}

/// The place a purchase's descriptor ends with. Card issuers sometimes print a store abroad
/// with no code at all ("JEAN COUTU 0DORVAL"); a purchase's descriptor may then end with a city
/// no other city shares a name with. Other text ending that way (a payroll DEPOSIT) is a word.
pub fn parse_purchase_descriptor(descriptor: &str) -> Option<DescriptorPlace> {
    read_descriptor(descriptor, true).1
}

fn province(code: &str) -> Option<&'static str> {
    PROVINCES
        .iter()
        .find(|(province, _)| *province == code)
        .map(|(_, admin1)| *admin1)
}

/// The city ending `text` that no other city shares a name with.
fn uncoded_city(text: &str) -> Option<(&'static City, usize)> {
    let gazetteer = &*GAZETTEER;
    let (city, start) = city_at_end(text, |_| true)?;
    let suffix = &text[start..];
    let unique = gazetteer
        .spellings
        .get(suffix)
        .is_some_and(|cities| cities.len() == 1);
    // A place listed without a population (a merged town) is too small to vouch for itself.
    (unique && suffix.len() >= UNCODED_CITY_LETTERS && city.population > 0).then_some((city, start))
}

fn read_descriptor(descriptor: &str, uncoded: bool) -> (String, Option<DescriptorPlace>) {
    let gazetteer = &*GAZETTEER;
    let web = WEB.is_match(descriptor);
    let mut text = normalize(descriptor);
    for prefix in PREFIXES {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest.to_owned();
        }
    }
    for trailer in TRAILERS {
        if let Some(at) = text.find(trailer) {
            text.truncate(at);
        }
    }
    let Some((body, code)) = text
        .rsplit_once(' ')
        .filter(|(_, code)| code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()))
    else {
        let place = uncoded
            .then(|| uncoded_city(&text))
            .flatten()
            .filter(|_| !web)
            .map(|(city, start)| {
                (
                    text[..start].trim().to_owned(),
                    DescriptorPlace {
                        country: city.country.as_str(),
                        region: Some(city.admin1.as_str()),
                        city: Some(city),
                    },
                )
            });
        return match place {
            Some((name, place)) => (name, Some(place)),
            None => (text, None),
        };
    };
    let state = us_state(code);
    let province = province(code);
    let name = |start: usize| body[..start].trim().to_owned();
    if web {
        let place = state.map(|state| DescriptorPlace {
            country: "US",
            region: Some(state),
            city: None,
        });
        return (name(body.len()), place);
    }
    // A code can be a US state and a country (DE, CA, IN, GA): the reading whose city matches
    // wins, the longer name first, then the US.
    let mut readings = Vec::new();
    if let Some(state) = state {
        if let Some((city, start)) =
            city_at_end(body, |city| city.country == "US" && city.admin1 == state)
        {
            readings.push((
                normalize(&city.name).len(),
                1,
                ("US", Some(state), city, start),
            ));
        }
    }
    if let Some(province) = province {
        if let Some((city, start)) =
            city_at_end(body, |city| city.country == "CA" && city.admin1 == province)
        {
            readings.push((
                normalize(&city.name).len(),
                1,
                ("CA", Some(province), city, start),
            ));
        }
    }
    if let Some(country) = gazetteer.countries.get(code) {
        if let Some((city, start)) = city_at_end(body, |city| city.country == *country) {
            let country = city.country.as_str();
            readings.push((
                normalize(&city.name).len(),
                0,
                (country, Some(city.admin1.as_str()), city, start),
            ));
        }
    }
    if let Some((_, _, (country, region, city, start))) = readings
        .into_iter()
        .max_by_key(|(length, us, _)| (*length, *us))
    {
        let place = DescriptorPlace {
            country,
            region,
            city: Some(city),
        };
        return (name(start), Some(place));
    }
    if let Some(state) = state {
        let place = DescriptorPlace {
            country: "US",
            region: Some(state),
            city: None,
        };
        return (name(body.len()), Some(place));
    }
    // A province alone, when its code isn't also a country's (NL, PE, SK).
    if let Some(province) = province.filter(|_| !gazetteer.countries.contains(code)) {
        let place = DescriptorPlace {
            country: "CA",
            region: Some(province),
            city: None,
        };
        return (name(body.len()), Some(place));
    }
    // An unexpected code (Amex writes BERLIN BE): the most populous city of that name anywhere.
    match city_at_end(body, |city| city.population >= FALLBACK_POPULATION) {
        Some((city, start)) => (
            name(start),
            Some(DescriptorPlace {
                country: city.country.as_str(),
                region: Some(city.admin1.as_str()),
                city: Some(city),
            }),
        ),
        None => (text.clone(), None),
    }
}

/// The most populous city of this name in the country, and the state for the US.
pub fn lookup(city: &str, country: &str, region: Option<&str>) -> Option<&'static City> {
    let gazetteer = &*GAZETTEER;
    gazetteer
        .spellings
        .get(&normalize(city))?
        .iter()
        .map(|&index| &gazetteer.cities[index as usize])
        .filter(|candidate| {
            candidate.country == country
                && (country != "US" || region.is_none_or(|region| candidate.admin1 == region))
        })
        .max_by_key(|candidate| candidate.population)
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PlaceSource {
    /// The card issuer's export gave the street address.
    Issuer,
    /// The bank's descriptor named the place.
    Descriptor,
    /// Only the provider's location did.
    Bank,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PlacePrecision {
    /// The spot itself: a matched business, or the provider's coordinates for a city or street
    /// address the sources agree on.
    Exact,
    /// A street address, at its city's centre.
    Street,
    /// A city, at its centre.
    City,
    /// A state or country, without a city.
    Region,
}

/// Where a transaction was made, as best known.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    /// The street address, as the issuer printed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// The US state, or the region code used abroad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// ISO 3166 code.
    pub country: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lon: Option<f64>,
    pub source: PlaceSource,
    pub precision: PlacePrecision,
}

impl Place {
    pub fn coordinates(&self) -> Option<(f64, f64)> {
        self.lat.zip(self.lon)
    }
}

/// The business a purchase was made at, from Brief's place data (Overture Maps).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Business {
    /// Overture's place id.
    pub id: String,
    pub name: String,
    /// Overture's category, such as `sporting_goods_store`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    pub lat: f64,
    pub lon: f64,
    /// Overture lists it as permanently closed (it may have closed since the purchase).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
}

fn bank_country(location: &TransactionLocation) -> String {
    location
        .country
        .as_deref()
        .map(|country| country.trim().to_ascii_uppercase())
        .filter(|country| country.len() == 2)
        .unwrap_or_else(|| "US".into())
}

fn blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// The provider's location as a place, with a city's centre when it sent no coordinates.
fn bank_place(location: &TransactionLocation) -> Option<Place> {
    let city = blank(location.city.as_deref());
    let region = blank(location.region.as_deref()).map(str::to_ascii_uppercase);
    if city.is_none() && region.is_none() {
        return None;
    }
    let country = bank_country(location);
    let exact = location.lat.zip(location.lon);
    let centre = city
        .and_then(|city| lookup(city, &country, region.as_deref()))
        .map(|city| (city.lat, city.lon));
    let coordinates = exact.or(centre);
    Some(Place {
        address: None,
        postal_code: None,
        city: city.map(str::to_owned),
        region,
        country,
        lat: coordinates.map(|(lat, _)| lat),
        lon: coordinates.map(|(_, lon)| lon),
        source: PlaceSource::Bank,
        precision: if exact.is_some() {
            PlacePrecision::Exact
        } else if city.is_some() {
            PlacePrecision::City
        } else {
            PlacePrecision::Region
        },
    })
}

/// Where a transaction was made: the issuer's street address when it agrees with the descriptor,
/// then the descriptor's city, then the provider's location when the descriptor names none.
pub fn resolve(transaction: &Transaction) -> Option<Place> {
    let reported = reported(transaction);
    issuer_place(transaction, reported.as_ref()).or(reported)
}

/// The first word of a street address and its number, which identify it across spellings
/// ("125 E El Camino Real", "125 EAST EL CAMINO REAL").
fn street_number(address: &str) -> Option<String> {
    let number = normalize(address).split(' ').next()?.to_owned();
    number
        .bytes()
        .any(|byte| byte.is_ascii_digit())
        .then_some(number)
}

/// The issuer's street address, when it says where the purchase was made: in the descriptor's
/// city, or anywhere when the descriptor names none. An address in another city is where the
/// merchant bills from (a head office, an online order's), not where the cardholder was.
fn issuer_place(transaction: &Transaction, reported: Option<&Place>) -> Option<Place> {
    let issuer = transaction.issuer.as_ref()?;
    if crate::merchants::bills_elsewhere(transaction) {
        return None;
    }
    let address = blank(issuer.address.as_deref())?;
    let printed_city = blank(issuer.city.as_deref())?;
    let country = issuer
        .country
        .clone()
        .or_else(|| reported.map(|place| place.country.clone()))
        .unwrap_or_else(|| "US".into());
    let city = lookup(printed_city, &country, issuer.region.as_deref());
    let name = city.map_or_else(|| printed_city.to_owned(), |city| city.name.clone());
    if let Some(reported) = reported {
        let elsewhere = reported.country != country
            || match reported.city.as_deref() {
                Some(city) => normalize(city) != normalize(&name),
                None => issuer.region.is_some() && reported.region != issuer.region,
            };
        if elsewhere {
            return None;
        }
    }
    // Plaid's coordinates are the store's when it reports the same street address.
    let exact = transaction
        .location
        .as_ref()
        .filter(|location| {
            transaction.provider_merchant.is_none()
                && location.address.as_deref().and_then(street_number) == street_number(address)
        })
        .and_then(|location| location.lat.zip(location.lon));
    let coordinates = exact
        .or(city.map(|city| (city.lat, city.lon)))
        .or_else(|| reported.and_then(Place::coordinates));
    Some(Place {
        address: Some(address.to_owned()),
        postal_code: issuer.postal_code.clone(),
        city: Some(name),
        region: issuer
            .region
            .clone()
            .or_else(|| city.map(|city| city.admin1.clone())),
        country,
        lat: coordinates.map(|(lat, _)| lat),
        lon: coordinates.map(|(_, lon)| lon),
        source: PlaceSource::Issuer,
        precision: if exact.is_some() {
            PlacePrecision::Exact
        } else {
            PlacePrecision::Street
        },
    })
}

/// Where the bank's own text and Plaid say it was made. The provider's exact coordinates are
/// kept only when it agrees on the city, since a chain's corporate address can stand in for the
/// store's.
fn reported(transaction: &Transaction) -> Option<Place> {
    let descriptor = blank(transaction.original_description.as_deref())
        .or_else(|| blank(transaction.description.as_deref()));
    let in_store =
        transaction.amount < 0.0 && transaction.payment_channel.as_deref() == Some("in store");
    let parsed = descriptor.and_then(if in_store {
        parse_purchase_descriptor
    } else {
        parse_descriptor
    });
    // Plaid's location is the address of the merchant Plaid named; once the bank's records
    // contradict that merchant, it is another business's.
    let bank = transaction
        .location
        .as_ref()
        .filter(|_| transaction.provider_merchant.is_none());
    let bank_place = bank.and_then(bank_place);
    match parsed {
        Some(DescriptorPlace {
            country,
            region,
            city: Some(city),
        }) => {
            let agrees = bank.is_some_and(|bank| {
                blank(bank.city.as_deref())
                    .is_some_and(|name| normalize(name) == normalize(&city.name))
                    && bank_country(bank) == country
            });
            let exact = bank
                .filter(|_| agrees)
                .and_then(|bank| bank.lat.zip(bank.lon));
            let (lat, lon) = exact.unwrap_or((city.lat, city.lon));
            Some(Place {
                address: None,
                postal_code: None,
                city: Some(city.name.clone()),
                region: region.map(str::to_owned),
                country: country.to_owned(),
                lat: Some(lat),
                lon: Some(lon),
                source: PlaceSource::Descriptor,
                precision: if exact.is_some() {
                    PlacePrecision::Exact
                } else {
                    PlacePrecision::City
                },
            })
        }
        // A state alone: the provider's city in that state is more precise.
        Some(DescriptorPlace {
            country,
            region,
            city: None,
        }) => match bank_place {
            Some(place)
                if place.city.is_some()
                    && place.country == country
                    && place.region.as_deref() == region =>
            {
                Some(place)
            }
            _ => Some(Place {
                address: None,
                postal_code: None,
                city: None,
                region: region.map(str::to_owned),
                country: country.to_owned(),
                lat: None,
                lon: None,
                source: PlaceSource::Descriptor,
                precision: PlacePrecision::Region,
            }),
        },
        None => bank_place,
    }
}

pub mod cache;
pub mod matching;
pub mod overture;

#[cfg(test)]
mod tests;

//! Apple Wallet's local transaction store, read only, and off until turned on in Settings. It
//! is read on each refresh and, while the window is visible, within a minute of Wallet
//! writing; each read is a `Wallet` change that projects saved data again.
//!
//! `~/Library/Passes/passes*.sqlite` is the private database behind Wallet. Apple Card arrives
//! there through a CloudKit zone with its whole history (its records' `CKEncrypted*` values
//! are plain protobuf wrappers on disk), and a card whose issuer runs its own notification
//! service arrives within seconds of a charge, from the day this Mac registered with it. An
//! issuer that relies on the network's token service reports only Apple Pay, if anything.
//!
//! The same database holds card tokens and notification credentials, so every query here names
//! its columns and a test keeps them off the list; nothing is read with `select *`. Apple's
//! obfuscated tables (`apples`, `grapes`, `oranges`) and CloudKit blobs are optional: when one
//! goes, its figures are absent. Only `payment_transaction` is required, and when it can't be
//! read the last saved records stand.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};

use crate::finance_contract::{
    AppleCardAccount, DailyCash, WalletForeign, WalletMethod, WalletNetwork, WalletReward,
};

/// Seconds between the Unix epoch and Apple's, the start of 2001.
const APPLE_EPOCH: i64 = 978_307_200;
/// Wallet's amounts are in ten-thousandths of a unit, and its reward rates in millionths.
const AMOUNT_DIGITS: u32 = 4;
const RATE_PER_PERCENT: f64 = 10_000.0;
/// A record with no time zone of its own is near enough to now to take the Mac's.
const LIVE_DAYS: i64 = 3;
/// A device fix this close to the mapped merchant puts the cardholder at the store.
const AT_THE_STORE_METRES: f64 = 200.0;
const RETURNS_PROGRAM: &str = "REWARD_RETURNS";

const UNREADABLE: &str =
    "Brief can't read Wallet. If macOS blocks it, turn on Brief in Privacy & Security, Full Disk Access";
const UNKNOWN_FORMAT: &str = "Wallet's format changed, so Brief kept what it had saved";

const TRANSACTIONS: &str = "SELECT pid, service_identifier, source_pid, amount, currency_code,
       transaction_date, transaction_status_changed_date, transaction_status, transaction_type,
       transaction_source, merchant_name, merchant_raw_name, merchant_industry_code, locality,
       administrative_area, location_latitude, location_longitude,
       foreign_exchange_information_destination_currency_code,
       foreign_exchange_information_destination_amount,
       foreign_exchange_information_exchange_rate, maps_merchant_pid
     FROM payment_transaction";
const PASS_SOURCES: &str = "SELECT transaction_source_pid, organization_name FROM pass
     WHERE transaction_source_pid IS NOT NULL";
const APPLICATION_SOURCES: &str = "SELECT a.transaction_source_pid, p.organization_name
     FROM payment_application a JOIN pass p ON p.pid = a.pass_pid";
const MAPS: &str = "SELECT pid, name, phone_number, url, location_latitude, location_longitude,
       postal_address
     FROM maps_merchant";
const RECORDS: &str = "SELECT record_name, record_data FROM cloud_store_record
     WHERE record_type IN ('Transaction', 'TransactionDeviceData') AND record_data IS NOT NULL";
const REWARDS: &str = "SELECT b, e, g, m FROM apples WHERE b IS NOT NULL";
const PAYOUTS: &str = "SELECT g.d, a.c, a.f FROM avocados a JOIN grapes g ON g.pid = a.a
     WHERE g.c = 1 ORDER BY g.d";
const BALANCE: &str = "SELECT b, c, d FROM oranges LIMIT 1";
const ACCOUNT_UPDATED: &str = "SELECT last_updated_date FROM peanuts LIMIT 1";
const HOLDERS: &str = "SELECT count(*) FROM gooseberry";
const SHARED_ZONES: &str = "SELECT count(*) FROM cloud_store_zone
     WHERE zone_name LIKE 'card-participant%' AND zone_owner <> '__defaultOwner__'";

/// The fields read out of Wallet's CloudKit records; everything else in them is left alone.
const RECORD_FIELDS: [&str; 17] = [
    "adjustmentType",
    "brandName",
    "brandURL",
    "cardEntryMode",
    "cardType",
    "clearingNetworkData",
    "merchantCity",
    "merchantMapsName",
    "merchantPhoneNumber",
    "merchantPostalAddress",
    "merchantRawCountry",
    "merchantState",
    "merchantURL",
    "merchantZip",
    "timeZone",
    "transactionSource",
    "transactionType",
];

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WalletStatus {
    Pending,
    #[default]
    Approved,
    Declined,
    /// Expired, or a state this code doesn't know. It never counts.
    Other,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WalletKind {
    #[default]
    Purchase,
    Refund,
    Adjustment,
    Other,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct WalletPlace {
    pub street: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    /// ISO 3166 two-letter code.
    pub country: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

/// One Wallet record, kept as saved so a record Apple later drops stays in Brief.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct WalletRow {
    pub id: String,
    /// The card, as its pass names its organization: "Apple Card", "Example Bank".
    pub issuer: String,
    /// Ten-thousandths of the account's currency: positive for a purchase, as Wallet has it.
    pub amount: i64,
    pub currency: String,
    /// The day of purchase where it happened, when the record says where.
    pub date: String,
    pub posted_on: Option<String>,
    pub occurred_at: Option<String>,
    pub status: WalletStatus,
    pub kind: WalletKind,
    pub adjustment: Option<String>,
    pub merchant: Option<String>,
    pub raw_merchant: Option<String>,
    pub merchant_code: Option<u32>,
    pub method: Option<WalletMethod>,
    pub in_person: Option<bool>,
    pub place: Option<WalletPlace>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub foreign: Option<WalletForeign>,
    pub reward: Option<WalletReward>,
    pub network: Option<WalletNetwork>,
}

impl WalletRow {
    /// The record's amount in cents' precision, as Wallet signs it.
    pub fn amount(&self) -> Decimal {
        Decimal::new(self.amount, AMOUNT_DIGITS)
    }
}

/// What Brief keeps from Wallet, with what the user decided about it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct WalletData {
    pub read_at: String,
    pub rows: Vec<WalletRow>,
    /// Records the user confirmed though the provider never reported them.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub confirmed: BTreeSet<String>,
    pub daily_cash: Option<DailyCash>,
    pub apple_card: Option<AppleCardAccount>,
}

impl WalletData {
    /// Whether both hold the same records, whenever each was read.
    pub fn same_records(&self, other: &Self) -> bool {
        self.rows == other.rows
            && self.daily_cash == other.daily_cash
            && self.apple_card == other.apple_card
    }
}

/// A field out of a CloudKit record.
#[derive(Clone, Debug, PartialEq)]
enum Field {
    Text(String),
    Number(i64),
}

type Fields = BTreeMap<String, Field>;

fn text(fields: Option<&Fields>, key: &str) -> Option<String> {
    match fields?.get(key)? {
        Field::Text(value) if !value.trim().is_empty() => Some(value.trim().to_owned()),
        _ => None,
    }
}

fn number(fields: Option<&Fields>, key: &str) -> Option<i64> {
    match fields?.get(key)? {
        Field::Number(value) => Some(*value),
        Field::Text(value) => value.parse().ok(),
    }
}

fn varint(bytes: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0_u64;
    for (index, byte) in bytes.iter().enumerate().take(10) {
        value |= u64::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            return Some((value, index + 1));
        }
    }
    None
}

/// The value inside a `CKEncrypted*` field, which on disk is a plain protobuf wrapper: field 6
/// for a string, field 3 for an integer.
fn unwrapped(bytes: &[u8]) -> Option<Field> {
    let (tag, rest) = bytes.split_first()?;
    match tag {
        0x32 => {
            let (length, used) = varint(rest)?;
            let end = used.checked_add(usize::try_from(length).ok()?)?;
            Some(Field::Text(
                String::from_utf8(rest.get(used..end)?.to_vec()).ok()?,
            ))
        }
        0x18 => varint(rest).map(|(value, _)| Field::Number(value as i64)),
        _ => None,
    }
}

/// The wanted fields of an NSKeyedArchiver'd CKRecord.
fn record(blob: &[u8]) -> Option<Fields> {
    use plist::Value;
    let archive = Value::from_reader(std::io::Cursor::new(blob)).ok()?;
    let objects = archive.as_dictionary()?.get("$objects")?.as_array()?;
    let at = |value: &'_ Value| -> Option<Value> {
        match value {
            Value::Uid(uid) => objects.get(usize::try_from(uid.get()).ok()?).cloned(),
            other => Some(other.clone()),
        }
    };
    let mut fields = Fields::new();
    for object in objects {
        let Some(map) = object.as_dictionary() else {
            continue;
        };
        let (Some(keys), Some(values)) = (
            map.get("NS.keys").and_then(Value::as_array),
            map.get("NS.objects").and_then(Value::as_array),
        ) else {
            continue;
        };
        for (key, value) in keys.iter().zip(values) {
            let Some(key) = at(key) else { continue };
            let Some(key) = key.as_string().filter(|key| RECORD_FIELDS.contains(key)) else {
                continue;
            };
            if fields.contains_key(key) {
                continue;
            }
            let Some(value) = at(value) else { continue };
            let Some(data) = value
                .as_dictionary()
                .and_then(|value| value.get("EncryptedData"))
                .and_then(&at)
            else {
                continue;
            };
            let bytes = data.as_data().map(<[u8]>::to_vec).or_else(|| {
                let inner = data.as_dictionary()?.get("NS.data")?;
                at(inner)?.as_data().map(<[u8]>::to_vec)
            });
            if let Some(field) = bytes.as_deref().and_then(unwrapped) {
                fields.insert(key.to_owned(), field);
            }
        }
    }
    Some(fields)
}

fn format_error(error: rusqlite::Error) -> String {
    let detail = error.to_string();
    if detail.contains("no such") {
        UNKNOWN_FORMAT.into()
    } else {
        UNREADABLE.into()
    }
}

/// Rows of an optional query, or none when Apple has moved its table.
fn optional<T>(
    connection: &Connection,
    query: &str,
    read: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Vec<T> {
    connection
        .prepare(query)
        .and_then(|mut statement| {
            statement
                .query_map([], read)?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default()
}

fn dollars(value: i64) -> f64 {
    Decimal::new(value, AMOUNT_DIGITS)
        .round_dp(2)
        .to_f64()
        .unwrap_or_default()
}

fn moment(seconds: f64) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(seconds as i64 + APPLE_EPOCH, 0).single()
}

/// ISO 3166's two-letter code for the three-letter one card networks print.
fn country_code(alpha3: &str) -> Option<&'static str> {
    const COUNTRIES: [(&str, &str); 46] = [
        ("ARE", "AE"),
        ("ARG", "AR"),
        ("AUS", "AU"),
        ("AUT", "AT"),
        ("BEL", "BE"),
        ("BRA", "BR"),
        ("CAN", "CA"),
        ("CHE", "CH"),
        ("CHN", "CN"),
        ("COL", "CO"),
        ("CRI", "CR"),
        ("CZE", "CZ"),
        ("DEU", "DE"),
        ("DNK", "DK"),
        ("DOM", "DO"),
        ("EGY", "EG"),
        ("ESP", "ES"),
        ("EST", "EE"),
        ("FIN", "FI"),
        ("FRA", "FR"),
        ("GBR", "GB"),
        ("GRC", "GR"),
        ("HKG", "HK"),
        ("HRV", "HR"),
        ("HUN", "HU"),
        ("IND", "IN"),
        ("IRL", "IE"),
        ("ISL", "IS"),
        ("ISR", "IL"),
        ("ITA", "IT"),
        ("JPN", "JP"),
        ("KOR", "KR"),
        ("LUX", "LU"),
        ("MAR", "MA"),
        ("MEX", "MX"),
        ("NLD", "NL"),
        ("NOR", "NO"),
        ("NZL", "NZ"),
        ("POL", "PL"),
        ("PRT", "PT"),
        ("SGP", "SG"),
        ("SWE", "SE"),
        ("THA", "TH"),
        ("TUR", "TR"),
        ("USA", "US"),
        ("ZAF", "ZA"),
    ];
    let code = alpha3.trim().to_ascii_uppercase();
    COUNTRIES
        .iter()
        .find(|(three, _)| *three == code)
        .map(|(_, two)| *two)
}

/// How a purchase was paid: the card's form first, then the channel, then the entry mode.
fn method(card_type: Option<&str>, source: i64, entry_mode: Option<&str>) -> Option<WalletMethod> {
    match (card_type, source) {
        (Some("physical"), _) => Some(WalletMethod::Card),
        (Some("virtual"), _) => Some(WalletMethod::Online),
        (_, 1) => Some(WalletMethod::Tap),
        (_, 2) => Some(WalletMethod::App),
        (_, 3) => Some(WalletMethod::Web),
        _ if entry_mode == Some("M") => Some(WalletMethod::Tap),
        _ => None,
    }
}

fn metres_apart(left: (f64, f64), right: (f64, f64)) -> f64 {
    let (from, to) = (left.0.to_radians(), right.0.to_radians());
    let latitude = (right.0 - left.0).to_radians() / 2.0;
    let longitude = (right.1 - left.1).to_radians() / 2.0;
    let chord = latitude.sin().powi(2) + from.cos() * to.cos() * longitude.sin().powi(2);
    2.0 * 6_371_000.0 * chord.sqrt().asin()
}

/// A merchant as Apple Maps knows it.
#[derive(Clone, Debug, Default)]
struct MapsMerchant {
    name: Option<String>,
    phone: Option<String>,
    website: Option<String>,
    coordinates: Option<(f64, f64)>,
    address: Option<serde_json::Value>,
}

fn present(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn coordinates(lat: Option<f64>, lon: Option<f64>) -> Option<(f64, f64)> {
    lat.zip(lon)
        .filter(|(lat, lon)| lat.is_finite() && lon.is_finite() && (*lat != 0.0 || *lon != 0.0))
}

/// Where a purchase was, from the mapped address, then the card network's fields, then the city
/// Wallet worked out from the device.
fn place(
    address: Option<&serde_json::Value>,
    record: Option<&Fields>,
    locality: Option<String>,
    area: Option<String>,
    spot: Option<(f64, f64)>,
) -> Option<WalletPlace> {
    let field = |key: &str| {
        address
            .and_then(|address| address.get(key))
            .and_then(serde_json::Value::as_str)
            .and_then(|value| present(Some(value)))
    };
    let country = field("ISOCountryCode").or_else(|| {
        text(record, "merchantRawCountry")
            .as_deref()
            .and_then(country_code)
            .map(str::to_owned)
    });
    // Abroad, networks can print the country where a region belongs.
    let printed_region = text(record, "merchantState").filter(|state| state.len() <= 2);
    let place = WalletPlace {
        street: field("street"),
        city: field("city")
            .or_else(|| text(record, "merchantCity"))
            .or(locality),
        region: field("state").or(printed_region).or(area),
        postal_code: field("postalCode").or_else(|| text(record, "merchantZip")),
        country,
        lat: spot.map(|(lat, _)| lat),
        lon: spot.map(|(_, lon)| lon),
    };
    (place.city.is_some() || place.country.is_some() || spot.is_some()).then_some(place)
}

fn network(record: Option<&Fields>, merchant_code: Option<u32>) -> Option<WalletNetwork> {
    let clearing = text(record, "clearingNetworkData")
        .and_then(|data| serde_json::from_str::<serde_json::Value>(&data).ok());
    let element = |key: &str| {
        clearing
            .as_ref()
            .and_then(|data| data.get(key))
            .and_then(serde_json::Value::as_str)
            .and_then(|value| present(Some(value)))
    };
    let network = WalletNetwork {
        merchant_id: element("DE042"),
        terminal_id: element("DE041"),
        descriptor: element("DE043"),
        merchant_code,
        entry_mode: element("DE022").or_else(|| text(record, "cardEntryMode")),
    };
    (network != WalletNetwork::default()).then_some(network)
}

/// The path of Wallet's database: the newest version the Mac has.
fn store_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or(UNREADABLE)?;
    let passes = Path::new(&home).join("Library/Passes");
    let mut stores = std::fs::read_dir(passes)
        .map_err(|_| UNREADABLE)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| {
            let version = path
                .file_name()?
                .to_str()?
                .strip_prefix("passes")?
                .strip_suffix(".sqlite")?
                .parse::<u32>()
                .ok()?;
            Some((version, path))
        })
        .collect::<Vec<_>>();
    stores.sort();
    stores.pop().map(|(_, path)| path).ok_or(UNREADABLE.into())
}

/// Whether Wallet has written since `read_at`. Unknown counts as changed.
pub fn changed_since(read_at: Option<&str>) -> bool {
    let read_at = read_at.and_then(|value| DateTime::parse_from_rfc3339(value).ok());
    let modified = store_path()
        .ok()
        .and_then(|path| std::fs::metadata(path).ok())
        .and_then(|metadata| metadata.modified().ok())
        .map(DateTime::<Utc>::from);
    match (read_at, modified) {
        (Some(read_at), Some(modified)) => modified > read_at,
        _ => true,
    }
}

/// Reads Wallet's database, which is never opened for writing.
pub fn read(now: DateTime<Utc>) -> Result<WalletData, String> {
    read_store(&store_path()?, now)
}

fn read_store(path: &Path, now: DateTime<Utc>) -> Result<WalletData, String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| UNREADABLE)?;
    connection
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| UNREADABLE)?;
    connection
        .pragma_update(None, "query_only", true)
        .map_err(|_| UNREADABLE)?;
    let rows = transactions(&connection, now)?;
    Ok(WalletData {
        read_at: now.to_rfc3339(),
        rows,
        confirmed: BTreeSet::new(),
        daily_cash: daily_cash(&connection),
        apple_card: apple_card(&connection),
    })
}

fn issuers(connection: &Connection) -> BTreeMap<i64, String> {
    let read = |row: &rusqlite::Row<'_>| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?));
    let mut issuers = optional(connection, APPLICATION_SOURCES, read)
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    issuers.extend(optional(connection, PASS_SOURCES, read));
    issuers
}

fn maps(connection: &Connection) -> BTreeMap<i64, MapsMerchant> {
    optional(connection, MAPS, |row| {
        Ok((
            row.get::<_, i64>(0)?,
            MapsMerchant {
                name: row.get(1)?,
                phone: row.get(2)?,
                website: row.get(3)?,
                coordinates: coordinates(row.get(4)?, row.get(5)?),
                address: row
                    .get::<_, Option<String>>(6)?
                    .and_then(|address| serde_json::from_str(&address).ok()),
            },
        ))
    })
    .into_iter()
    .collect()
}

fn records(connection: &Connection) -> BTreeMap<String, Fields> {
    optional(connection, RECORDS, |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
    })
    .into_iter()
    .filter_map(|(name, blob)| Some((name, record(&blob)?)))
    .collect()
}

/// Each purchase's reward. A purchase has one; a return carries its own, reversed.
fn rewards(connection: &Connection) -> BTreeMap<i64, WalletReward> {
    optional(connection, REWARDS, |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            row.get::<_, Option<f64>>(2)?.unwrap_or_default(),
            row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        ))
    })
    .into_iter()
    .map(|(transaction, amount, rate, program)| {
        let reversed = program == RETURNS_PROGRAM;
        (
            transaction,
            WalletReward {
                amount: if reversed {
                    -dollars(amount)
                } else {
                    dollars(amount)
                },
                percent: rate / RATE_PER_PERCENT,
                program,
            },
        )
    })
    .collect()
}

fn transactions(connection: &Connection, now: DateTime<Utc>) -> Result<Vec<WalletRow>, String> {
    let issuers = issuers(connection);
    let maps = maps(connection);
    let records = records(connection);
    let rewards = rewards(connection);
    let mut statement = connection.prepare(TRANSACTIONS).map_err(format_error)?;
    let rows = statement
        .query_map([], |row| {
            let id: String = row.get(1)?;
            let source: i64 = row.get(2)?;
            let Some(issuer) = issuers.get(&source) else {
                return Ok(None);
            };
            let pid: i64 = row.get(0)?;
            let Some(when) = moment(row.get::<_, f64>(5)?) else {
                return Ok(None);
            };
            let record = records.get(&id);
            let device = records.get(&format!("device-{id}"));
            let maps_merchant = row
                .get::<_, Option<i64>>(20)?
                .and_then(|merchant| maps.get(&merchant));
            let zone = text(device, "timeZone").and_then(|zone| zone.parse::<chrono_tz::Tz>().ok());
            let local_day = |moment: DateTime<Utc>| match zone {
                Some(zone) => moment.with_timezone(&zone).date_naive(),
                None => moment.with_timezone(&Local).date_naive(),
            };
            let occurred_at = match zone {
                Some(zone) => Some(when.with_timezone(&zone).to_rfc3339()),
                None => (now - when <= Duration::days(LIVE_DAYS))
                    .then(|| when.with_timezone(&Local).to_rfc3339()),
            };
            let status = match row.get::<_, Option<i64>>(7)?.unwrap_or_default() {
                0 => WalletStatus::Pending,
                1 => WalletStatus::Approved,
                2 => WalletStatus::Declined,
                _ => WalletStatus::Other,
            };
            let kind = match row.get::<_, Option<i64>>(8)?.unwrap_or_default() {
                0 => WalletKind::Purchase,
                1 => WalletKind::Refund,
                7 => WalletKind::Adjustment,
                _ => WalletKind::Other,
            };
            let source_channel = number(device, "transactionSource")
                .or(row.get::<_, Option<i64>>(9)?)
                .unwrap_or_default();
            let entry_mode = text(record, "cardEntryMode");
            let method = method(
                text(record, "cardType").as_deref(),
                source_channel,
                entry_mode.as_deref(),
            );
            let spot = coordinates(row.get(15)?, row.get(16)?);
            let store = maps_merchant.and_then(|merchant| merchant.coordinates);
            let in_person = match method {
                Some(WalletMethod::Tap | WalletMethod::Card) => Some(true),
                Some(WalletMethod::App | WalletMethod::Web | WalletMethod::Online) => Some(false),
                None => spot
                    .zip(store)
                    .filter(|(spot, store)| metres_apart(*spot, *store) <= AT_THE_STORE_METRES)
                    .map(|_| true),
            };
            let device_address = text(device, "merchantPostalAddress")
                .and_then(|address| serde_json::from_str(&address).ok());
            let address = maps_merchant
                .and_then(|merchant| merchant.address.as_ref())
                .or(device_address.as_ref());
            let merchant_code = row
                .get::<_, Option<i64>>(12)?
                .and_then(|code| u32::try_from(code).ok())
                .filter(|code| *code > 0);
            let currency: String = row.get(4)?;
            let foreign = row
                .get::<_, Option<String>>(17)?
                .filter(|code| !code.is_empty() && *code != currency)
                .and_then(|code| {
                    Some(WalletForeign {
                        currency: code,
                        amount: dollars(row.get::<_, Option<i64>>(18).ok()??),
                        rate: row
                            .get::<_, Option<String>>(19)
                            .ok()?
                            .and_then(|rate| rate.parse().ok()),
                    })
                });
            let merchant = text(device, "brandName")
                .or_else(|| text(device, "merchantMapsName"))
                .or_else(|| maps_merchant.and_then(|merchant| present(merchant.name.as_deref())))
                .or_else(|| present(row.get::<_, Option<String>>(10).ok()?.as_deref()));
            Ok(Some(WalletRow {
                id,
                issuer: issuer.clone(),
                amount: row.get(3)?,
                date: local_day(when).to_string(),
                posted_on: row
                    .get::<_, Option<f64>>(6)?
                    .and_then(moment)
                    .map(|posted| local_day(posted).to_string()),
                occurred_at,
                status,
                kind,
                adjustment: text(record, "adjustmentType"),
                merchant,
                raw_merchant: present(row.get::<_, Option<String>>(11)?.as_deref()),
                merchant_code,
                method,
                in_person,
                place: place(
                    address,
                    record,
                    present(row.get::<_, Option<String>>(13)?.as_deref()),
                    present(row.get::<_, Option<String>>(14)?.as_deref()),
                    store.or(spot),
                ),
                phone: text(device, "merchantPhoneNumber").or_else(|| {
                    maps_merchant.and_then(|merchant| present(merchant.phone.as_deref()))
                }),
                website: text(device, "brandURL")
                    .or_else(|| text(device, "merchantURL"))
                    .or_else(|| {
                        maps_merchant.and_then(|merchant| present(merchant.website.as_deref()))
                    }),
                foreign,
                reward: rewards.get(&pid).cloned(),
                network: network(record, merchant_code),
                currency,
            }))
        })
        .map_err(format_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(format_error)?;
    Ok(rows.into_iter().flatten().collect())
}

/// Daily Cash earned on purchases against the payouts Apple attempted.
fn daily_cash(connection: &Connection) -> Option<DailyCash> {
    let earned = optional(connection, REWARDS, |row| {
        Ok((
            row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        ))
    });
    let payouts = optional(connection, PAYOUTS, |row| {
        Ok((
            row.get::<_, f64>(0)?,
            row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            row.get::<_, Option<String>>(2)?.unwrap_or_default(),
        ))
    });
    if earned.is_empty() && payouts.is_empty() {
        return None;
    }
    let day = |seconds: f64| {
        moment(seconds).map(|moment| moment.with_timezone(&Local).date_naive().to_string())
    };
    let total = |matches: &dyn Fn(&str) -> bool| {
        dollars(
            earned
                .iter()
                .filter(|(_, program)| matches(program))
                .map(|(amount, _)| *amount)
                .sum(),
        )
    };
    let paid = |wanted: &str| {
        dollars(
            payouts
                .iter()
                .filter(|(_, _, status)| status == wanted)
                .map(|(_, amount, _)| *amount)
                .sum(),
        )
    };
    let last_paid = payouts
        .iter()
        .rfind(|(_, _, status)| status == "success")
        .map(|(date, ..)| *date);
    Some(DailyCash {
        earned: total(&|program| program != RETURNS_PROGRAM),
        reversed: total(&|program| program == RETURNS_PROGRAM),
        paid: paid("success"),
        failed: paid("failed"),
        failed_payouts: payouts
            .iter()
            .filter(|(_, _, status)| status == "failed")
            .count(),
        last_paid_on: last_paid.and_then(day),
        failing_since: payouts
            .iter()
            .find(|(date, _, status)| {
                status == "failed" && last_paid.is_none_or(|last| *date > last)
            })
            .and_then(|(date, ..)| day(*date)),
    })
}

/// The whole account as Apple reports it, which may include other holders' charges. Apple's
/// table has no column names; a limit, what's left of it, and a balance are read from their
/// shape, so anything that doesn't fit that shape is left out.
fn apple_card(connection: &Connection) -> Option<AppleCardAccount> {
    let (limit, available, balance) = optional(connection, BALANCE, |row| {
        Ok((
            row.get::<_, Option<i64>>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })
    .into_iter()
    .next()
    .and_then(|(limit, available, balance)| Some((limit?, available?, balance?)))
    .filter(|(limit, available, balance)| {
        *limit > 0 && (0..=*limit).contains(available) && (0..=*limit).contains(balance)
    })?;
    let as_of = optional(connection, ACCOUNT_UPDATED, |row| {
        row.get::<_, Option<f64>>(0)
    })
    .into_iter()
    .flatten()
    .next()
    .and_then(moment)
    .map(|moment| moment.with_timezone(&Local).date_naive().to_string());
    let count = |query: &str| {
        optional(connection, query, |row| row.get::<_, i64>(0))
            .first()
            .copied()
            .unwrap_or_default()
    };
    Some(AppleCardAccount {
        limit: dollars(limit),
        available: dollars(available),
        balance: dollars(balance),
        as_of,
        holders: usize::try_from(count(HOLDERS)).unwrap_or_default(),
        shared: count(SHARED_ZONES) > 0,
    })
}

/// This read over what was saved: a record Apple dropped stays, and a live record keeps the time
/// it was first seen with.
pub fn merged(saved: Option<&WalletData>, read: WalletData) -> WalletData {
    let saved = saved.cloned().unwrap_or_default();
    let mut rows = saved
        .rows
        .iter()
        .map(|row| (row.id.clone(), row.clone()))
        .collect::<BTreeMap<_, _>>();
    for mut row in read.rows {
        if row.occurred_at.is_none() {
            row.occurred_at = rows.get(&row.id).and_then(|old| old.occurred_at.clone());
        }
        rows.insert(row.id.clone(), row);
    }
    let mut rows = rows.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then_with(|| left.id.cmp(&right.id))
    });
    WalletData {
        read_at: read.read_at,
        rows,
        confirmed: saved.confirmed.clone(),
        daily_cash: read.daily_cash.or_else(|| saved.daily_cash.clone()),
        apple_card: read.apple_card.or_else(|| saved.apple_card.clone()),
    }
}

#[cfg(test)]
mod tests;

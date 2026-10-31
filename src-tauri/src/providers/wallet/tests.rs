use super::*;
use chrono::TimeZone;
use plist::{Dictionary, Uid, Value};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 30, 12, 0, 0).unwrap()
}

/// Seconds since Apple's epoch for a UTC moment.
fn apple(year: i32, month: u32, day: u32, hour: u32) -> i64 {
    Utc.with_ymd_and_hms(year, month, day, hour, 0, 0)
        .unwrap()
        .timestamp()
        - APPLE_EPOCH
}

fn wrapped_text(value: &str) -> Vec<u8> {
    let mut bytes = vec![0x32, value.len() as u8];
    bytes.extend_from_slice(value.as_bytes());
    bytes
}

fn wrapped_number(value: u8) -> Vec<u8> {
    vec![0x18, value]
}

/// A CKRecord archive as Wallet stores one: keyed objects holding wrapped values.
fn archive(fields: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut objects = vec![Value::String("$null".into())];
    let mut keys = Vec::new();
    let mut values = Vec::new();
    for (key, bytes) in fields {
        objects.push(Value::String((*key).into()));
        keys.push(Value::Uid(Uid::new(objects.len() as u64 - 1)));
        let mut wrapper = Dictionary::new();
        wrapper.insert("EncryptedData".into(), Value::Data(bytes.clone()));
        objects.push(Value::Dictionary(wrapper));
        values.push(Value::Uid(Uid::new(objects.len() as u64 - 1)));
    }
    let mut map = Dictionary::new();
    map.insert("NS.keys".into(), Value::Array(keys));
    map.insert("NS.objects".into(), Value::Array(values));
    objects.push(Value::Dictionary(map));
    let mut root = Dictionary::new();
    root.insert("$objects".into(), Value::Array(objects));
    let mut bytes = Vec::new();
    Value::Dictionary(root)
        .to_writer_binary(&mut bytes)
        .unwrap();
    bytes
}

struct Store(PathBuf);

impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Wallet's tables with synthetic rows: an Apple Card tap, an online charge, and an Amex charge.
fn store() -> Store {
    let path = std::env::temp_dir().join(format!("wallet-{}.sqlite", uuid::Uuid::new_v4()));
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE payment_transaction (pid INTEGER, service_identifier TEXT, source_pid
               INTEGER, amount INTEGER, currency_code TEXT, transaction_date INTEGER,
               transaction_status_changed_date INTEGER, transaction_status INTEGER,
               transaction_type INTEGER, transaction_source INTEGER, merchant_name TEXT,
               merchant_raw_name TEXT, merchant_industry_code INTEGER, locality TEXT,
               administrative_area TEXT, location_latitude REAL, location_longitude REAL,
               foreign_exchange_information_destination_currency_code TEXT,
               foreign_exchange_information_destination_amount INTEGER,
               foreign_exchange_information_exchange_rate TEXT, maps_merchant_pid INTEGER);
             CREATE TABLE pass (pid INTEGER, organization_name TEXT, transaction_source_pid INTEGER);
             CREATE TABLE payment_application (transaction_source_pid INTEGER, pass_pid INTEGER);
             CREATE TABLE maps_merchant (pid INTEGER, name TEXT, phone_number TEXT, url TEXT,
               location_latitude REAL, location_longitude REAL, postal_address TEXT);
             CREATE TABLE cloud_store_record (record_name TEXT, record_type TEXT, record_data BLOB);
             CREATE TABLE apples (b INTEGER, e INTEGER, g INTEGER, m TEXT);
             CREATE TABLE grapes (pid INTEGER, c INTEGER, d INTEGER);
             CREATE TABLE avocados (a INTEGER, c INTEGER, f TEXT);
             CREATE TABLE oranges (b INTEGER, c INTEGER, d INTEGER);
             CREATE TABLE peanuts (last_updated_date INTEGER);
             CREATE TABLE gooseberry (pid INTEGER);
             CREATE TABLE cloud_store_zone (zone_name TEXT, zone_owner TEXT);
             INSERT INTO pass VALUES (10, 'Apple Card', NULL), (11, 'American Express', 2);
             INSERT INTO payment_application VALUES (1, 10);
             INSERT INTO maps_merchant VALUES (5, 'Corner Market', '+1 555 0100', 'https://corner.test',
               39.75, -105.0,
               '{\"street\":\"100 Main St\",\"city\":\"Denver\",\"state\":\"CO\",\"postalCode\":\"80210\",\"ISOCountryCode\":\"US\"}');
             INSERT INTO oranges VALUES (100000000, 90000000, 10000000);
             INSERT INTO gooseberry VALUES (1), (2);
             INSERT INTO cloud_store_zone VALUES ('card-participant-individual-x', '_someone');",
        )
        .unwrap();
    let insert = "INSERT INTO payment_transaction VALUES
        (?1, ?2, ?3, ?4, 'USD', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, NULL, NULL, ?13, ?14, ?15, ?16, ?17, ?18)";
    connection
        .execute(
            insert,
            rusqlite::params![
                100,
                "tap",
                1,
                487_300,
                apple(2025, 4, 29, 1),
                apple(2025, 4, 30, 2),
                1,
                0,
                1,
                "Corner Mkt",
                "corner mkt #12",
                5411,
                39.7501,
                -105.0001,
                Option::<String>::None,
                Option::<i64>::None,
                Option::<String>::None,
                5
            ],
        )
        .unwrap();
    connection
        .execute(
            insert,
            rusqlite::params![
                101,
                "abroad",
                1,
                223_600,
                apple(2024, 10, 6, 13),
                apple(2024, 10, 7, 13),
                1,
                0,
                1,
                "Harder Kulm",
                "harder kulm",
                4789,
                0.0,
                0.0,
                "CHF",
                190_000,
                "1.18",
                Option::<i64>::None
            ],
        )
        .unwrap();
    connection
        .execute(
            insert,
            rusqlite::params![
                102,
                "live",
                2,
                159_900,
                apple(2026, 10, 29, 21),
                Option::<i64>::None,
                1,
                0,
                0,
                "CORNER MKT",
                Option::<String>::None,
                0,
                0.0,
                0.0,
                Option::<String>::None,
                Option::<i64>::None,
                Option::<String>::None,
                Option::<i64>::None
            ],
        )
        .unwrap();
    connection
        .execute(
            insert,
            rusqlite::params![
                103,
                "gone",
                99,
                100,
                apple(2026, 1, 1, 12),
                Option::<i64>::None,
                1,
                0,
                0,
                "No card",
                Option::<String>::None,
                0,
                0.0,
                0.0,
                Option::<String>::None,
                Option::<i64>::None,
                Option::<String>::None,
                Option::<i64>::None
            ],
        )
        .unwrap();
    let blob = |name: &str, kind: &str, bytes: Vec<u8>| {
        connection
            .execute(
                "INSERT INTO cloud_store_record VALUES (?1, ?2, ?3)",
                rusqlite::params![name, kind, bytes],
            )
            .unwrap();
    };
    blob(
        "tap",
        "Transaction",
        archive(&[
            ("cardType", wrapped_text("digital")),
            ("cardEntryMode", wrapped_text("M")),
            ("secretThing", wrapped_text("never kept")),
            (
                "clearingNetworkData",
                wrapped_text(
                    r#"{"DE041":"T1","DE042":"M1","DE043":"CORNER MKT DENVER","DE022":"M","DE002":"4111"}"#,
                ),
            ),
        ]),
    );
    blob(
        "device-tap",
        "TransactionDeviceData",
        archive(&[
            ("timeZone", wrapped_text("America/Denver")),
            ("transactionSource", wrapped_number(1)),
            ("brandName", wrapped_text("Corner Market")),
        ]),
    );
    blob(
        "abroad",
        "Transaction",
        archive(&[
            ("cardType", wrapped_text("digital")),
            ("merchantCity", wrapped_text("Interlaken")),
            ("merchantState", wrapped_text("CHE")),
            ("merchantRawCountry", wrapped_text("CHE")),
        ]),
    );
    connection
        .execute_batch(
            "INSERT INTO apples VALUES (100, 9700, 20000, 'REWARD_DEFAULT_WALLET');
             INSERT INTO apples VALUES (101, 2200, 10000, 'REWARD_DEFAULT_OTHER');
             INSERT INTO apples VALUES (101, 500, 10000, 'REWARD_RETURNS');",
        )
        .unwrap();
    connection
        .execute_batch(&format!(
            "INSERT INTO grapes VALUES (1, 1, {}), (2, 1, {}), (3, 1, {}), (4, 11, {});
             INSERT INTO avocados VALUES (1, 9700, 'success'), (2, 2200, 'failed'),
               (3, 1000, 'failed'), (4, 9999, 'failed');",
            apple(2025, 5, 1, 12),
            apple(2025, 9, 10, 12),
            apple(2025, 10, 1, 12),
            apple(2025, 11, 1, 12),
        ))
        .unwrap();
    connection
        .execute("INSERT INTO peanuts VALUES (?1)", [apple(2026, 10, 28, 12)])
        .unwrap();
    Store(path)
}

fn row<'a>(data: &'a WalletData, id: &str) -> &'a WalletRow {
    data.rows.iter().find(|row| row.id == id).unwrap()
}

#[test]
fn reads_amounts_times_and_how_a_tap_was_paid() {
    let store = store();
    let data = read_store(&store.0, now()).unwrap();
    let tap = row(&data, "tap");
    assert_eq!(tap.issuer, "Apple Card");
    assert_eq!(tap.amount().round_dp(2).to_string(), "48.73");
    // The evening of the 28th in Denver, though it is the 29th in UTC.
    assert_eq!(tap.date, "2025-04-28");
    assert_eq!(
        tap.occurred_at.as_deref(),
        Some("2025-04-28T19:00:00-06:00")
    );
    assert_eq!(tap.posted_on.as_deref(), Some("2025-04-29"));
    assert_eq!(tap.method, Some(WalletMethod::Tap));
    assert_eq!(tap.in_person, Some(true));
    assert_eq!(tap.merchant.as_deref(), Some("Corner Market"));
    assert_eq!(tap.raw_merchant.as_deref(), Some("corner mkt #12"));
    assert_eq!(tap.merchant_code, Some(5411));
    assert_eq!(tap.reward.as_ref().unwrap().amount, 0.97);
    assert_eq!(tap.reward.as_ref().unwrap().percent, 2.0);
}

#[test]
fn the_mapped_address_leads_and_the_network_record_keeps_only_what_is_named() {
    let store = store();
    let data = read_store(&store.0, now()).unwrap();
    let tap = row(&data, "tap");
    let place = tap.place.as_ref().unwrap();
    assert_eq!(place.street.as_deref(), Some("100 Main St"));
    assert_eq!(place.city.as_deref(), Some("Denver"));
    assert_eq!(place.country.as_deref(), Some("US"));
    assert_eq!(place.lat, Some(39.75));
    let network = tap.network.as_ref().unwrap();
    assert_eq!(network.merchant_id.as_deref(), Some("M1"));
    assert_eq!(network.terminal_id.as_deref(), Some("T1"));
    assert_eq!(network.entry_mode.as_deref(), Some("M"));
    // A card number in the network's record, or a field not listed, is never kept.
    let saved = serde_json::to_string(&data).unwrap();
    assert!(!saved.contains("4111"));
    assert!(!saved.contains("never kept"));
}

#[test]
fn abroad_keeps_the_charged_currency_and_ignores_a_country_printed_as_a_region() {
    let store = store();
    let data = read_store(&store.0, now()).unwrap();
    let abroad = row(&data, "abroad");
    assert_eq!(
        abroad.foreign,
        Some(WalletForeign {
            currency: "CHF".into(),
            amount: 19.0,
            rate: Some(1.18),
        })
    );
    let place = abroad.place.as_ref().unwrap();
    assert_eq!(place.city.as_deref(), Some("Interlaken"));
    assert_eq!(place.region, None);
    assert_eq!(place.country.as_deref(), Some("CH"));
}

#[test]
fn a_live_record_takes_the_macs_time_and_an_old_one_without_a_zone_has_none() {
    let store = store();
    let data = read_store(&store.0, now()).unwrap();
    assert!(row(&data, "live").occurred_at.is_some());
    assert_eq!(row(&data, "live").issuer, "American Express");
    assert_eq!(row(&data, "abroad").occurred_at, None);
    // A record for no card on this Mac isn't kept.
    assert!(data.rows.iter().all(|row| row.id != "gone"));
}

#[test]
fn daily_cash_compares_what_was_earned_with_the_payouts_apple_attempted() {
    let store = store();
    let data = read_store(&store.0, now()).unwrap();
    let cash = data.daily_cash.unwrap();
    assert_eq!(cash.earned, 1.19);
    assert_eq!(cash.reversed, 0.05);
    assert_eq!(cash.paid, 0.97);
    // A payout of another kind (`grapes.c` 11) isn't Daily Cash.
    assert_eq!(cash.failed, 0.32);
    assert_eq!(cash.failed_payouts, 2);
    assert!(cash.last_paid_on.unwrap().starts_with("2025-05"));
    assert!(cash.failing_since.unwrap().starts_with("2025-09"));
}

#[test]
fn the_account_is_read_from_its_shape_and_never_names_the_other_holder() {
    let store = store();
    let data = read_store(&store.0, now()).unwrap();
    let account = data.apple_card.unwrap();
    assert_eq!(account.limit, 10_000.0);
    assert_eq!(account.available, 9_000.0);
    assert_eq!(account.balance, 1_000.0);
    assert_eq!(account.holders, 2);
    assert!(account.shared);
    assert!(account.as_of.unwrap().starts_with("2026-10-28"));
}

#[test]
fn optional_tables_going_leaves_their_figures_absent() {
    let store = store();
    let connection = Connection::open(&store.0).unwrap();
    connection
        .execute_batch(
            "DROP TABLE apples; DROP TABLE avocados; DROP TABLE oranges;
             DROP TABLE cloud_store_record; DROP TABLE maps_merchant;",
        )
        .unwrap();
    drop(connection);
    let data = read_store(&store.0, now()).unwrap();
    assert_eq!(data.rows.len(), 3);
    assert_eq!(data.daily_cash, None);
    assert_eq!(data.apple_card, None);
    assert_eq!(row(&data, "tap").merchant.as_deref(), Some("Corner Mkt"));
}

#[test]
fn a_changed_format_is_reported_and_never_treated_as_no_records() {
    let store = store();
    let connection = Connection::open(&store.0).unwrap();
    connection
        .execute_batch("ALTER TABLE payment_transaction RENAME COLUMN amount TO worth;")
        .unwrap();
    drop(connection);
    assert_eq!(read_store(&store.0, now()).unwrap_err(), UNKNOWN_FORMAT);
}

#[test]
fn a_missing_store_says_how_to_allow_it() {
    let path = std::env::temp_dir().join(format!("absent-{}.sqlite", uuid::Uuid::new_v4()));
    assert_eq!(read_store(&path, now()).unwrap_err(), UNREADABLE);
}

#[test]
fn reading_never_writes_to_wallets_database() {
    let store = store();
    let before = std::fs::read(&store.0).unwrap();
    read_store(&store.0, now()).unwrap();
    assert_eq!(std::fs::read(&store.0).unwrap(), before);
}

#[test]
fn no_query_can_reach_tokens_or_credentials() {
    let queries = [
        TRANSACTIONS,
        PASS_SOURCES,
        APPLICATION_SOURCES,
        MAPS,
        RECORDS,
        REWARDS,
        PAYOUTS,
        BALANCE,
        ACCOUNT_UPDATED,
        HOLDERS,
        SHARED_ZONES,
    ];
    for query in queries {
        let query = query.to_lowercase();
        for forbidden in [
            "*,",
            "select *",
            "token",
            "authentication",
            "notification_service",
            "fpan",
            "dpan",
            "nonce",
            "signing",
            "signature",
            "participant_handle",
            "gooseberry.",
            "serial",
        ] {
            assert!(
                !query.contains(forbidden),
                "{forbidden} appears in a Wallet query"
            );
        }
    }
    // What the second account holder is called is never read, only that they exist.
    assert_eq!(HOLDERS, "SELECT count(*) FROM gooseberry");
}

#[test]
fn a_record_apple_drops_stays_and_a_confirmation_survives_a_read() {
    let store = store();
    let first = read_store(&store.0, now()).unwrap();
    let mut saved = first.clone();
    saved.confirmed.insert("live".into());
    saved.rows.push(WalletRow {
        id: "older".into(),
        date: "2020-01-01".into(),
        ..WalletRow::default()
    });
    let next = merged(Some(&saved), first);
    assert!(next.rows.iter().any(|row| row.id == "older"));
    assert!(next.confirmed.contains("live"));
    assert_eq!(next.rows.first().unwrap().id, "live");
}

#[test]
fn a_live_record_keeps_the_time_it_was_first_seen_with() {
    let store = store();
    let saved = read_store(&store.0, now()).unwrap();
    let later = now() + Duration::days(30);
    let reread = read_store(&store.0, later).unwrap();
    assert_eq!(row(&reread, "live").occurred_at, None);
    let next = merged(Some(&saved), reread);
    assert_eq!(
        row(&next, "live").occurred_at,
        row(&saved, "live").occurred_at
    );
}

#[test]
fn a_later_read_of_the_same_records_is_nothing_new() {
    let store = store();
    let saved = merged(None, read_store(&store.0, now()).unwrap());
    let later = now() + Duration::days(30);
    let reread = read_store(&store.0, later).unwrap();
    let mut next = merged(Some(&saved), reread);
    assert!(saved.same_records(&next));
    next.rows.first_mut().unwrap().amount += 1;
    assert!(!saved.same_records(&next));
}

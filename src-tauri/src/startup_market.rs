//! Disposable last-market observations, separate from committed financial state.
use crate::{finance_contract::MarketProjection, providers::MarketSnapshots};
use chrono::{DateTime, Local, Utc};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

pub struct StartupMarket {
    path: PathBuf,
    written: Mutex<Option<(u64, Instant)>>,
}

fn identity(snapshot: &Value) -> Option<Value> {
    Some(serde_json::json!({
        "calculationVersion": snapshot.get("calculationVersion"),
        "valuation": MarketProjection::deserialize(snapshot).ok()?,
    }))
}

fn valid(market: &MarketSnapshots, saved_at: &str, now: DateTime<Utc>) -> bool {
    let Ok(saved) = DateTime::parse_from_rfc3339(saved_at) else {
        return false;
    };
    if saved > now
        || now.signed_duration_since(saved).num_hours() >= 24
        || saved.with_timezone(&Local).date_naive() != now.with_timezone(&Local).date_naive()
        || saved.date_naive() != now.date_naive()
        || market.snapshots.is_empty()
    {
        return false;
    }
    market.snapshots.iter().all(|(symbol, quote)| {
        symbol == &quote.symbol
            && quote.price.is_finite()
            && quote.price > 0.0
            && quote.previous_close.is_finite()
            && quote.previous_close > 0.0
            && quote.daily_change_pct.is_finite()
            && quote.weekly_change_pct.is_none_or(f64::is_finite)
            && quote
                .weekly_reference_price
                .is_none_or(|price| price.is_finite() && price > 0.0)
            && DateTime::parse_from_rfc3339(&quote.as_of).is_ok_and(|time| time <= now)
    })
}

impl StartupMarket {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            written: Mutex::new(None),
        }
    }
    pub fn load(&self, snapshot: &Value) -> Option<MarketSnapshots> {
        crate::database::secure_file(&self.path).ok()?;
        let db =
            Connection::open_with_flags(&self.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .ok()?;
        let (basis, market, saved): (String, String, String) = db
            .query_row(
                "SELECT basis, market, saved FROM startup WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok()?;
        if serde_json::from_str::<Value>(&basis).ok()? != identity(snapshot)? {
            return None;
        }
        let market = serde_json::from_str(&market).ok()?;
        valid(&market, &saved, Utc::now()).then_some(market)
    }
    pub fn save(&self, snapshot: &Value, market: &MarketSnapshots) {
        let now = Utc::now();
        if !valid(market, &now.to_rfc3339(), now) {
            return;
        }
        let revision = snapshot["revision"].as_u64().unwrap_or(0);
        let Ok(mut written) = self.written.lock() else {
            return;
        };
        if written.as_ref().is_some_and(|(prior, time)| {
            *prior == revision && time.elapsed() < Duration::from_secs(60)
        }) {
            return;
        }
        let result = (|| -> Option<()> {
            let basis = identity(snapshot)?;
            let db = Connection::open(&self.path).ok()?;
            crate::database::secure_file(&self.path).ok()?;
            db.execute_batch("CREATE TABLE IF NOT EXISTS startup (id INTEGER PRIMARY KEY CHECK(id=1), basis TEXT NOT NULL, market TEXT NOT NULL, saved TEXT NOT NULL);").ok()?;
            db.execute(
                "INSERT OR REPLACE INTO startup VALUES (1, ?1, ?2, ?3)",
                params![
                    basis.to_string(),
                    serde_json::to_string(market).ok()?,
                    now.to_rfc3339()
                ],
            )
            .ok()?;
            Some(())
        })();
        if result.is_some() {
            *written = Some((revision, Instant::now()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MarketSnapshot;
    use std::collections::BTreeMap;

    fn market(now: DateTime<Utc>) -> MarketSnapshots {
        MarketSnapshots {
            snapshots: BTreeMap::from([(
                "TEST".into(),
                MarketSnapshot {
                    symbol: "TEST".into(),
                    price: 110.0,
                    previous_close: 100.0,
                    previous_close_as_of: Some((now - chrono::Duration::days(1)).to_rfc3339()),
                    daily_change_pct: 10.0,
                    weekly_change_pct: Some(10.0),
                    weekly_reference_price: Some(100.0),
                    weekly_reference_date: Some(
                        (now - chrono::Duration::days(7)).date_naive().to_string(),
                    ),
                    as_of: now.to_rfc3339(),
                },
            )]),
            session: "Regular market".into(),
            feed: "iex".into(),
            delay_minutes: 0,
            as_of: Some(now.to_rfc3339()),
            next_transition_at: None,
            poll_interval_ms: Some(300000),
            history_feed: "iex".into(),
            history_delay_minutes: 0,
        }
    }

    #[test]
    fn rejects_expired_future_and_invalid_market_observations() {
        let now = DateTime::parse_from_rfc3339("2026-09-18T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut quotes = market(now);
        assert!(valid(&quotes, &now.to_rfc3339(), now));
        assert!(!valid(
            &quotes,
            &(now - chrono::Duration::days(1)).to_rfc3339(),
            now
        ));
        assert!(!valid(
            &quotes,
            &(now + chrono::Duration::seconds(1)).to_rfc3339(),
            now
        ));
        quotes.snapshots.get_mut("TEST").unwrap().price = -1.0;
        assert!(!valid(&quotes, &now.to_rfc3339(), now));
        quotes = market(now + chrono::Duration::seconds(1));
        assert!(!valid(&quotes, &now.to_rfc3339(), now));
    }

    #[test]
    fn cache_is_optional_revision_bound_and_failed_updates_preserve_prior_observations() {
        let directory =
            std::env::temp_dir().join(format!("brief-startup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let cache = StartupMarket::new(directory.join("market.sqlite3"));
        let snapshot: Value =
            serde_json::from_str(include_str!("../../src/data/fixtures/native-finance.json"))
                .unwrap();
        assert!(cache.load(&snapshot).is_none());
        let quotes = market(Utc::now());
        cache.save(&snapshot, &quotes);
        assert_eq!(
            cache.load(&snapshot).unwrap().snapshots["TEST"].price,
            110.0
        );
        let mut changed = snapshot.clone();
        changed["revision"] = (snapshot["revision"].as_u64().unwrap_or(0) + 1).into();
        assert!(cache.load(&changed).is_none());
        changed = snapshot.clone();
        changed["holdings"][0]["shares"] = 999.0.into();
        assert!(cache.load(&changed).is_none());
        let mut invalid = quotes;
        invalid.snapshots.get_mut("TEST").unwrap().price = f64::NAN;
        cache.save(&snapshot, &invalid);
        assert_eq!(
            cache.load(&snapshot).unwrap().snapshots["TEST"].price,
            110.0
        );
        std::fs::write(&cache.path, b"invalid cache").unwrap();
        assert!(cache.load(&snapshot).is_none());
        std::fs::remove_dir_all(directory).unwrap();
    }
}

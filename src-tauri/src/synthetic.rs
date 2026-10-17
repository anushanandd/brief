//! A synthetic person: one generated life, as Plaid, SnapTrade, and Alpaca would report it
//! at each refresh.
//!
//! Over eleven weeks the person is paid twice a month, pays rent, shops and eats on a card,
//! pays the card and saves from checking, takes a trip, subscribes to a service, and buys a
//! fund every month. Card charges arrive pending and post days later, a restaurant's with a
//! tip and a new id, and one hold is dropped without posting, as Plaid reports them. Every
//! name and amount is invented and the same seed gives the same life, so tests can replay it
//! exactly. The replay test (Gate A) runs it through the real refresh and edit paths.
use std::collections::BTreeMap;

pub(crate) mod career;

use chrono::{Duration, NaiveDate, TimeZone, Utc};
use serde_json::{json, Value};

use crate::{
    providers::{PlaidCache, PlaidData, ProviderDataCache, SnapTradeData},
    storage::change::Refresh,
};

/// The first day of the life and how long it runs.
const START: (i32, u32, u32) = (2026, 6, 1);
const DAYS: i64 = 77;
/// Days between refreshes.
const REFRESH_EVERY: i64 = 3;
const HOME: (&str, &str) = ("Columbus", "OH");
const AWAY: (&str, &str) = ("Denver", "CO");

/// splitmix64: small, fast, and the same everywhere.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    /// Whole cents from `low` to `high` dollars.
    fn dollars(&mut self, low: u64, high: u64) -> f64 {
        (low * 100 + self.below((high - low) * 100 + 1)) as f64 / 100.0
    }
}

/// One transaction as the person's bank or card would report it.
#[derive(Clone)]
struct Purchase {
    id: String,
    account: &'static str,
    date: NaiveDate,
    /// Plaid's sign: positive moves money out of the account.
    amount: f64,
    name: String,
    category: (&'static str, &'static str),
    place: Option<(&'static str, &'static str)>,
    online: bool,
    /// The day it posts; before then it is pending.
    posts: NaiveDate,
    /// What posts, when it differs from the pending amount (a tip).
    posted_amount: Option<f64>,
    /// A hold that disappears instead of posting.
    dropped: bool,
}

impl Purchase {
    fn record(&self, today: NaiveDate) -> Option<Value> {
        let pending = today < self.posts;
        if !pending && self.dropped {
            return None;
        }
        let tipped = !pending && self.posted_amount.is_some();
        let mut record = json!({
            "transaction_id": if tipped { format!("{}-posted", self.id) } else { self.id.clone() },
            "account_id": self.account,
            "amount": if tipped { self.posted_amount } else { Some(self.amount) },
            "iso_currency_code": "USD",
            "name": self.name,
            "merchant_name": self.name,
            "pending": pending,
            "date": if pending { self.date } else { self.posts },
            "authorized_date": self.date,
            "payment_channel": if self.online { "online" } else { "in store" },
            "personal_finance_category": {
                "primary": self.category.0,
                "detailed": self.category.1,
                "confidence_level": "HIGH"
            },
        });
        if tipped {
            record["pending_transaction_id"] = json!(self.id);
        }
        if let Some((city, region)) = self.place {
            record["location"] = json!({ "city": city, "region": region, "country": "US" });
        }
        Some(record)
    }

    /// What it has taken from the account's current balance by `today`.
    fn posted(&self, today: NaiveDate) -> f64 {
        if today < self.posts || self.dropped {
            0.0
        } else {
            self.posted_amount.unwrap_or(self.amount)
        }
    }
}

/// The person's whole life: every transaction, fund price, and purchase of the fund.
pub(crate) struct Life {
    start: NaiveDate,
    purchases: Vec<Purchase>,
    /// The fund's close each day.
    prices: Vec<(NaiveDate, f64)>,
    /// Monthly purchases of the fund: (date, units, price).
    buys: Vec<(NaiveDate, f64, f64)>,
}

impl Life {
    pub(crate) fn new(seed: u64) -> Self {
        let mut rng = Rng(seed);
        let start = NaiveDate::from_ymd_opt(START.0, START.1, START.2).unwrap_or_default();
        let mut purchases = Vec::new();
        let mut add = |purchase: Purchase| purchases.push(purchase);
        let mut prices = Vec::new();
        let mut price = 100.0_f64;
        let mut buys = Vec::new();
        let trip = start + Duration::days(40);
        for offset in 0..DAYS {
            let date = start + Duration::days(offset);
            let day = date.format("%d").to_string();
            let id = |kind: &str| format!("{kind}-{}", date.format("%Y%m%d"));
            let base = |id: String, account, amount, name: &str, category| Purchase {
                id,
                account,
                date,
                amount,
                name: name.into(),
                category,
                place: None,
                online: true,
                posts: date,
                posted_amount: None,
                dropped: false,
            };
            if day == "01" || day == "15" {
                add(base(
                    id("pay"),
                    "checking",
                    -2450.0,
                    "Example Employer Payroll",
                    ("INCOME", "INCOME_WAGES"),
                ));
            }
            if day == "01" {
                add(base(
                    id("rent"),
                    "checking",
                    1650.0,
                    "Example Property Rent",
                    ("RENT_AND_UTILITIES", "RENT_AND_UTILITIES_RENT"),
                ));
                add(base(
                    id("save"),
                    "checking",
                    400.0,
                    "Transfer to Savings",
                    ("TRANSFER_OUT", "TRANSFER_OUT_SAVINGS"),
                ));
                add(base(
                    id("saved"),
                    "savings",
                    -400.0,
                    "Transfer from Checking",
                    ("TRANSFER_IN", "TRANSFER_IN_ACCOUNT_TRANSFER"),
                ));
            }
            if day == "20" {
                let payment = rng.dollars(600, 900);
                add(base(
                    id("card-payment"),
                    "checking",
                    payment,
                    "Card Payment",
                    ("LOAN_PAYMENTS", "LOAN_PAYMENTS_CREDIT_CARD_PAYMENT"),
                ));
                add(base(
                    id("card-paid"),
                    "card",
                    -payment,
                    "Payment Thank You",
                    ("LOAN_PAYMENTS", "LOAN_PAYMENTS_CREDIT_CARD_PAYMENT"),
                ));
            }
            if day == "09" {
                add(base(
                    id("stream"),
                    "card",
                    15.99,
                    "Example Streaming",
                    ("ENTERTAINMENT", "ENTERTAINMENT_TV_AND_MOVIES"),
                ));
            }
            let away = date >= trip && date < trip + Duration::days(4);
            let place = Some(if away { AWAY } else { HOME });
            for index in 0..rng.below(3) {
                let (name, category, low, high) = match rng.below(4) {
                    0 => (
                        "Example Grocer",
                        ("FOOD_AND_DRINK", "FOOD_AND_DRINK_GROCERIES"),
                        20,
                        140,
                    ),
                    1 => (
                        "Example Coffee",
                        ("FOOD_AND_DRINK", "FOOD_AND_DRINK_COFFEE"),
                        3,
                        9,
                    ),
                    2 => (
                        "Example Fuel",
                        ("TRANSPORTATION", "TRANSPORTATION_GAS"),
                        25,
                        60,
                    ),
                    _ => (
                        "Example Bistro",
                        ("FOOD_AND_DRINK", "FOOD_AND_DRINK_RESTAURANT"),
                        18,
                        90,
                    ),
                };
                let amount = rng.dollars(low, high);
                let restaurant = category.1 == "FOOD_AND_DRINK_RESTAURANT";
                add(Purchase {
                    place,
                    online: false,
                    posts: date + Duration::days(1 + rng.below(3) as i64),
                    posted_amount: restaurant.then(|| ((amount * 1.2) * 100.0).round() / 100.0),
                    ..base(
                        format!("{}-{index}", id("card")),
                        "card",
                        amount,
                        name,
                        category,
                    )
                });
            }
            if offset == 23 {
                // A hotel's hold, released without posting.
                add(Purchase {
                    place,
                    online: false,
                    posts: date + Duration::days(4),
                    dropped: true,
                    ..base(
                        id("hold"),
                        "card",
                        200.0,
                        "Example Hotel",
                        ("TRAVEL", "TRAVEL_LODGING"),
                    )
                });
            }
            price = (price * (1.0 + (rng.below(301) as f64 - 150.0) / 10_000.0) * 100.0).round()
                / 100.0;
            prices.push((date, price));
            if day == "05" {
                buys.push((date, 3.0, price));
            }
        }
        Self {
            start,
            purchases,
            prices,
            buys,
        }
    }

    /// The days the person refreshes, oldest first.
    pub(crate) fn refresh_days(&self) -> Vec<NaiveDate> {
        (REFRESH_EVERY..DAYS)
            .step_by(REFRESH_EVERY as usize)
            .map(|offset| self.start + Duration::days(offset))
            .collect()
    }

    /// What a refresh at noon UTC on `today` would fetch.
    pub(crate) fn refresh(&self, today: NaiveDate) -> Refresh {
        let committed_at = Utc.from_utc_datetime(&today.and_hms_opt(12, 0, 0).unwrap_or_default());
        let transactions = self
            .purchases
            .iter()
            .filter(|purchase| purchase.date <= today)
            .filter_map(|purchase| purchase.record(today))
            .collect::<Vec<_>>();
        let balance = |account: &str, opening: f64| {
            let moved: f64 = self
                .purchases
                .iter()
                .filter(|purchase| purchase.account == account)
                .map(|purchase| purchase.posted(today))
                .sum();
            ((opening - moved) * 100.0).round() / 100.0
        };
        let fetched = committed_at.to_rfc3339();
        let account = |id: &str, name: &str, kind: &str, subtype: &str, current: f64| {
            json!({
                "account_id": id, "name": name, "institution_name": "Example Bank",
                "type": kind, "subtype": subtype, "mask": "0000",
                "balance_fetched_at": fetched,
                "balances": { "current": current, "iso_currency_code": "USD" }
            })
        };
        let mut card = account("card", "Everyday Card", "credit", "credit card", 0.0);
        // A card's balance is what is owed: its charges less its payments.
        card["balances"]["current"] = json!(-balance("card", 0.0));
        card["balances"]["limit"] = json!(5000);
        let plaid = PlaidData {
            accounts: vec![
                account(
                    "checking",
                    "Everyday Checking",
                    "depository",
                    "checking",
                    balance("checking", 3200.0),
                ),
                account(
                    "savings",
                    "Rainy Day Savings",
                    "depository",
                    "savings",
                    balance("savings", 8000.0),
                ),
                card,
            ],
            transactions: transactions.clone(),
            transaction_history_start: ["checking", "savings", "card"]
                .into_iter()
                .map(|id| (id.to_owned(), self.start.to_string()))
                .collect(),
            ..PlaidData::default()
        };
        let snaptrade = self.brokerage(today, &fetched);
        let prices = self
            .prices
            .iter()
            .filter(|(date, _)| *date <= today)
            .collect::<Vec<_>>();
        let points = |scale: f64| {
            prices
                .iter()
                .map(|(date, price)| json!({ "date": date, "value": (price * scale * 100.0).round() / 100.0 }))
                .collect::<Vec<_>>()
        };
        let plaid_cache = serde_json::from_value::<PlaidCache>(json!({ "items": { "item": {
            "cursor": format!("cursor-{today}"),
            "transactions": transactions,
            "history_complete": true,
        }}}))
        .unwrap_or_default();
        let mut provider_data = ProviderDataCache {
            plaid: Some(plaid),
            snaptrade: Some(snaptrade),
            security_history: BTreeMap::from([("TEST".to_owned(), points(1.0))]),
            security_history_adjustment: Some("split".into()),
            market_history_refreshed_at: Some(fetched.clone()),
            ..ProviderDataCache::default()
        };
        for provider in ["plaid", "snaptrade"] {
            provider_data.sync_status.insert(
                provider.into(),
                crate::finance_contract::ProviderSyncStatus {
                    updated_at: Some(fetched.clone()),
                    error: None,
                },
            );
        }
        Refresh {
            committed_at,
            calendar_date: today,
            balances_fresh: true,
            refreshed_account_ids: vec![
                "plaid:checking".into(),
                "plaid:savings".into(),
                "plaid:card".into(),
                "snaptrade:brokerage".into(),
            ],
            benchmark: Value::Array(points(4.5)),
            warnings: Vec::new(),
            plaid_cache,
            provider_data,
        }
    }

    /// The brokerage as SnapTrade reports it on `today`: the fund bought each month, cash
    /// left from each month's deposit, and a daily balance history.
    fn brokerage(&self, today: NaiveDate, fetched: &str) -> SnapTradeData {
        let bought = self
            .buys
            .iter()
            .filter(|(date, ..)| *date <= today)
            .collect::<Vec<_>>();
        let units = 10.0 + bought.iter().map(|(_, units, _)| units).sum::<f64>();
        let deposits = 500.0 * bought.len() as f64;
        let spent: f64 = bought.iter().map(|(_, units, price)| units * price).sum();
        let cash = ((1000.0 + deposits - spent) * 100.0).round() / 100.0;
        let close = |day: NaiveDate| {
            self.prices
                .iter()
                .rev()
                .find(|(date, _)| *date <= day)
                .map_or(100.0, |(_, price)| *price)
        };
        let price = close(today);
        let mut activities = Vec::new();
        for (date, units, price) in &bought {
            activities.push(json!({
                "id": format!("deposit-{date}"), "type": "CONTRIBUTION", "amount": 500,
                "trade_date": date, "currency": { "code": "USD" }
            }));
            activities.push(json!({
                "id": format!("buy-{date}"), "type": "BUY",
                "amount": (units * price * 100.0).round() / 100.0,
                "trade_date": date, "units": units, "price": price,
                "symbol": { "symbol": "TEST" }, "currency": { "code": "USD" }
            }));
        }
        let history = (0..=(today - self.start).num_days())
            .map(|offset| {
                let day = self.start + Duration::days(offset);
                let held = 10.0
                    + self
                        .buys
                        .iter()
                        .filter(|(date, ..)| *date <= day)
                        .map(|(_, units, _)| units)
                        .sum::<f64>();
                let invested: f64 = self
                    .buys
                    .iter()
                    .filter(|(date, ..)| *date <= day)
                    .map(|(_, units, price)| 500.0 - units * price)
                    .sum();
                json!({
                    "date": day,
                    "total_value": ((held * close(day) + 1000.0 + invested) * 100.0).round() / 100.0
                })
            })
            .collect();
        let total = ((units * price + cash) * 100.0).round() / 100.0;
        SnapTradeData {
            accounts: vec![json!({
                "id": "brokerage", "name": "Example Brokerage", "institution_name": "Example Broker",
                "balance": { "total": { "amount": total, "currency": "USD" } },
                "sync_status": { "holdings": { "last_successful_sync": fetched } }
            })],
            positions: BTreeMap::from([(
                "brokerage".into(),
                vec![json!({
                    "units": units, "price": price, "cost_basis": 95, "currency": "USD",
                    "instrument": { "kind": "etf", "symbol": "TEST", "description": "Example Index Fund" }
                })],
            )]),
            cash_balances: BTreeMap::from([(
                "brokerage".into(),
                vec![json!({ "currency": { "code": "USD" }, "cash": cash })],
            )]),
            positions_as_of: BTreeMap::from([("brokerage".into(), Some(fetched.to_owned()))]),
            activities: BTreeMap::from([("brokerage".into(), activities)]),
            balance_history: BTreeMap::from([("brokerage".into(), history)]),
            history_complete: true,
            activity_complete: true,
            ..SnapTradeData::default()
        }
    }
}

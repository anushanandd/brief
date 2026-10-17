//! A synthetic career: twenty-two months of one invented person's working life, as saved
//! state, for the evidence graph's tests and the assistant's eval set.
//!
//! They work at Example Analytics in Denver from January 2025, paid every other Friday, and
//! take a trip to Chicago in August 2025. They buy furniture that November. Their last day is
//! March 27, 2026; they move to San Francisco and start at Morgan Stanley on April 20, paid
//! twice a month, part into checking and part into a Platinum CashPlus account opened in May.
//! In May they also open the Morgan Stanley Platinum Card, whose $895 annual fee posts on May
//! 22, and a brokerage account. The $895 engagement bonus posts to CashPlus on July 15.
//! Deposits to CashPlus fall short of $5,000 in September. Pay statements and a W-2 back the
//! jobs. Every name and amount is invented, and the life is the same on every run.
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde_json::json;

use super::Rng;
use crate::{
    documents::Document,
    finance_contract::{Account, Transaction},
    storage::FinanceState,
};

/// The day the life is read, as tests pass it.
pub(crate) const TODAY: (i32, u32, u32) = (2026, 10, 17);
const FIRST: (i32, u32, u32) = (2025, 1, 1);
/// Example Analytics' last pay date, and Morgan Stanley's first day.
const LEFT: (i32, u32, u32) = (2026, 3, 27);
const JOINED: (i32, u32, u32) = (2026, 4, 20);
const MOVED: (i32, u32, u32) = (2026, 4, 5);
const CASHPLUS_OPENED: (i32, u32, u32) = (2026, 5, 4);
const CARD_OPENED: (i32, u32, u32) = (2026, 5, 10);
const BROKERAGE_OPENED: (i32, u32, u32) = (2026, 5, 15);
const TRIP: (i32, u32, u32) = (2025, 8, 14);

fn date((year, month, day): (i32, u32, u32)) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("career dates are valid")
}

pub(crate) fn today() -> NaiveDate {
    date(TODAY)
}

struct Builder {
    transactions: Vec<Transaction>,
}

/// What a transaction is, as Plaid would categorize it.
struct Entry<'a> {
    id: String,
    account: &'a str,
    day: NaiveDate,
    /// Contract sign: negative leaves the account.
    amount: f64,
    merchant: &'a str,
    description: Option<&'a str>,
    category: &'a str,
    detail: &'a str,
    place: Option<(&'a str, &'a str)>,
}

impl Builder {
    fn add(&mut self, entry: Entry) {
        let mut transaction: Transaction = serde_json::from_value(json!({
            "id": entry.id,
            "merchant": entry.merchant,
            "category": entry.category,
            "categoryDetail": entry.detail,
            "date": entry.day.to_string(),
            "amount": entry.amount,
            "account": entry.account,
            "accountId": format!("plaid:{}", entry.account),
            "pending": false,
            "description": entry.description,
            "originalDescription": entry.description,
            "paymentChannel": if entry.place.is_some() { "in store" } else { "online" },
            "location": entry.place.map(|(city, region)| json!({ "city": city, "region": region, "country": "US" })),
        }))
        .expect("career transactions are valid");
        transaction.classification = Some(crate::transaction_policy::classify(&transaction));
        self.transactions.push(transaction);
    }
}

fn account(
    id: &str,
    name: &str,
    institution: &str,
    kind: &str,
    value: f64,
    official: Option<&str>,
) -> Account {
    serde_json::from_value(json!({
        "id": format!("plaid:{id}"), "name": name, "institution": institution, "type": kind,
        "value": value, "currency": "USD", "officialName": official,
        "balanceFetchedAt": "2026-10-17T12:00:00Z",
    }))
    .expect("career accounts are valid")
}

fn document(id: &str, kind: &str, extra: serde_json::Value) -> Document {
    let mut value = json!({
        "id": crate::database::sha256(id.as_bytes()),
        "fileName": format!("{id}.pdf"),
        "mediaType": "application/pdf",
        "size": 1_000,
        "importedAt": "2026-10-01T12:00:00Z",
        "kind": kind,
        "reviewed": true,
    });
    if let (Some(value), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
        value.extend(extra.clone());
    }
    serde_json::from_value(value).expect("career documents are valid")
}

fn payslip(
    id: &str,
    issuer: &str,
    period: (NaiveDate, NaiveDate),
    paid: NaiveDate,
    deposits: &[f64],
) -> Document {
    document(
        id,
        "payslip",
        json!({
            "issuer": issuer,
            "taxYear": paid.year(),
            "date": paid.to_string(),
            "periodStart": period.0.to_string(),
            "periodEnd": period.1.to_string(),
            "fields": [{ "key": "net", "label": "Net pay", "amount": deposits.iter().sum::<f64>() }],
            "deposits": deposits.iter().map(|amount| json!({ "amount": amount })).collect::<Vec<_>>(),
        }),
    )
}

fn w2(issuer: &str, year: i32, state: &str) -> Document {
    document(
        &format!("w2-{year}"),
        "taxForm",
        json!({
            "form": "W-2",
            "issuer": issuer,
            "taxYear": year,
            "fields": [
                { "key": "box1", "label": "Wages", "amount": 67_600.0 },
                { "key": "state", "label": "State", "text": state },
            ],
        }),
    )
}

/// The career as saved state: accounts, transactions, and documents, with no edits yet.
pub(crate) fn state() -> FinanceState {
    let mut rng = Rng(0x00C0_FFEE);
    let mut life = Builder {
        transactions: Vec::new(),
    };
    let denver = Some(("Denver", "CO"));
    let san_francisco = Some(("San Francisco", "CA"));
    let (left, joined, moved) = (date(LEFT), date(JOINED), date(MOVED));
    let mut day = date(FIRST);
    while day < today() {
        let stamp = day.format("%Y%m%d");
        let id = |kind: &str| format!("{kind}-{stamp}");
        // Example Analytics pays every other Friday through the last day.
        if day.weekday() == Weekday::Fri
            && day <= left
            && (day - date((2025, 1, 10))).num_days() % 14 == 0
        {
            life.add(Entry {
                id: id("pay"),
                account: "checking",
                day,
                amount: 2_600.0,
                merchant: "Example Analytics",
                description: Some("EXAMPLE ANALYTICS PAYROLL PPD ID: 4455"),
                category: "Income",
                detail: "INCOME_WAGES",
                place: None,
            });
        }
        // Morgan Stanley pays on the 15th and the month's last day, split two ways.
        let last_day = (day + Duration::days(1)).day() == 1;
        if day >= joined && (day.day() == 15 || last_day) {
            let september = day.year() == 2026 && day.month() == 9;
            let to_cashplus = if day < date(CASHPLUS_OPENED) {
                0.0
            } else if september {
                2_400.0
            } else {
                2_600.0
            };
            life.add(Entry {
                id: id("ms-pay"),
                account: "checking",
                day,
                amount: 4_100.0 - to_cashplus,
                merchant: "Morgan Stanley",
                description: Some("MORGAN STANLEY SERV PAYROLL PPD ID: 9001"),
                category: "Income",
                detail: "INCOME_WAGES",
                place: None,
            });
            if to_cashplus > 0.0 {
                life.add(Entry {
                    id: id("ms-pay-cashplus"),
                    account: "cashplus",
                    day,
                    amount: to_cashplus,
                    merchant: "Morgan Stanley",
                    description: Some("MORGAN STANLEY SERV PAYROLL PPD ID: 9001"),
                    category: "Income",
                    detail: "INCOME_WAGES",
                    place: None,
                });
            }
        }
        if day.day() == 1 {
            let (merchant, amount) = if day < moved {
                ("Example Apartments", 1_500.0)
            } else {
                ("Example Residences", 2_400.0)
            };
            life.add(Entry {
                id: id("rent"),
                account: "checking",
                day,
                amount: -amount,
                merchant,
                description: None,
                category: "Rent And Utilities",
                detail: "RENT_AND_UTILITIES_RENT",
                place: None,
            });
        }
        let card = if day >= date(CARD_OPENED) {
            "msplatinum"
        } else {
            "card"
        };
        if day.day() == 20 {
            for (account, amount) in [("checking", -900.0), (card, 900.0)] {
                life.add(Entry {
                    id: format!("{}-{account}", id("card-payment")),
                    account,
                    day,
                    amount,
                    merchant: "Card Payment",
                    description: None,
                    category: "Loan Payments",
                    detail: "LOAN_PAYMENTS_CREDIT_CARD_PAYMENT",
                    place: None,
                });
            }
        }
        let trip = date(TRIP);
        let away = day >= trip && day < trip + Duration::days(4);
        // Moving days have no purchases at home.
        let home = if day < left + Duration::days(5) {
            denver
        } else if day >= moved {
            san_francisco
        } else {
            None
        };
        let place = if away { Some(("Chicago", "IL")) } else { home };
        if place.is_some() {
            for index in 0..rng.below(3) {
                let (merchant, category, detail, low, high) = match rng.below(4) {
                    0 => (
                        "Example Grocer",
                        "Food And Drink",
                        "FOOD_AND_DRINK_GROCERIES",
                        20,
                        140,
                    ),
                    1 => (
                        "Example Coffee",
                        "Food And Drink",
                        "FOOD_AND_DRINK_COFFEE",
                        3,
                        9,
                    ),
                    2 => (
                        "Example Fuel",
                        "Transportation",
                        "TRANSPORTATION_GAS",
                        25,
                        60,
                    ),
                    _ => (
                        "Example Bistro",
                        "Food And Drink",
                        "FOOD_AND_DRINK_RESTAURANT",
                        18,
                        90,
                    ),
                };
                life.add(Entry {
                    id: format!("{}-{index}", id("buy")),
                    account: card,
                    day,
                    amount: -rng.dollars(low, high),
                    merchant,
                    description: None,
                    category,
                    detail,
                    place,
                });
            }
        }
        day += Duration::days(1);
    }
    let one = |life: &mut Builder, entry: Entry| life.add(entry);
    one(
        &mut life,
        Entry {
            id: "flight-chicago".into(),
            account: "card",
            day: date((2025, 7, 28)),
            amount: -342.0,
            merchant: "Example Airlines",
            description: None,
            category: "Travel",
            detail: "TRAVEL_FLIGHTS",
            place: None,
        },
    );
    one(
        &mut life,
        Entry {
            id: "furniture".into(),
            account: "card",
            day: date((2025, 11, 3)),
            amount: -3_200.0,
            merchant: "Example Furniture",
            description: None,
            category: "Home Improvement",
            detail: "HOME_IMPROVEMENT_FURNITURE",
            place: denver,
        },
    );
    one(
        &mut life,
        Entry {
            id: "membership-fee".into(),
            account: "msplatinum",
            day: date((2026, 5, 22)),
            amount: -895.0,
            merchant: "Annual Membership Fee",
            description: Some("ANNUAL MEMBERSHIP FEE"),
            category: "Bank Fees",
            detail: "BANK_FEES_OTHER_BANK_FEES",
            place: None,
        },
    );
    one(
        &mut life,
        Entry {
            id: "engagement-bonus".into(),
            account: "cashplus",
            day: date((2026, 7, 15)),
            amount: 895.0,
            merchant: "Engagement Bonus",
            description: Some("PLATINUM CASHPLUS ENGAGEMENT BONUS"),
            category: "Income",
            detail: "INCOME_OTHER_INCOME",
            place: None,
        },
    );
    for (account, amount) in [("checking", -1_000.0), ("brokerage", 1_000.0)] {
        one(
            &mut life,
            Entry {
                id: format!("brokerage-open-{account}"),
                account,
                day: date(BROKERAGE_OPENED),
                amount,
                merchant: "Transfer to Example Brokerage",
                description: None,
                category: "Transfer",
                detail: if amount < 0.0 {
                    "TRANSFER_OUT_INVESTMENT_AND_RETIREMENT_FUNDS"
                } else {
                    "TRANSFER_IN_INVESTMENT_AND_RETIREMENT_FUNDS"
                },
                place: None,
            },
        );
    }
    let mut transactions = life.transactions;
    transactions.sort_by(|left, right| right.date.cmp(&left.date).then(left.id.cmp(&right.id)));

    let analytics = "Example Analytics LLC";
    let morgan = "Morgan Stanley Services Group Inc.";
    let documents = vec![
        payslip(
            "analytics-first",
            analytics,
            (date((2024, 12, 23)), date((2025, 1, 5))),
            date((2025, 1, 10)),
            &[2_600.0],
        ),
        payslip(
            "analytics-june",
            analytics,
            (date((2025, 5, 26)), date((2025, 6, 8))),
            date((2025, 6, 13)),
            &[2_600.0],
        ),
        payslip(
            "analytics-last",
            analytics,
            (date((2026, 3, 9)), date((2026, 3, 22))),
            date(LEFT),
            &[2_600.0],
        ),
        w2(analytics, 2025, "CO"),
        payslip(
            "ms-first",
            morgan,
            (date(JOINED), date((2026, 4, 30))),
            date((2026, 4, 30)),
            &[4_100.0],
        ),
        payslip(
            "ms-july",
            morgan,
            (date((2026, 7, 1)), date((2026, 7, 15))),
            date((2026, 7, 15)),
            &[1_500.0, 2_600.0],
        ),
        payslip(
            "ms-september",
            morgan,
            (date((2026, 9, 16)), date((2026, 9, 30))),
            date((2026, 9, 30)),
            &[1_700.0, 2_400.0],
        ),
    ];
    let mut state = crate::storage::empty_state().expect("the empty state is valid");
    state.snapshot.accounts = vec![
        account(
            "checking",
            "Everyday Checking",
            "Example Bank",
            "cash",
            6_200.0,
            Some("Everyday Checking"),
        ),
        account(
            "card",
            "Everyday Card",
            "Example Bank",
            "credit",
            -310.0,
            Some("Example Cash Back Card"),
        ),
        account(
            "cashplus",
            "Platinum CashPlus - 1234",
            "Morgan Stanley",
            "cash",
            18_400.0,
            None,
        ),
        account(
            "msplatinum",
            "Platinum Card",
            "American Express",
            "credit",
            -1_240.0,
            Some("Morgan Stanley Platinum Card®"),
        ),
        account(
            "brokerage",
            "Example Brokerage",
            "Example Broker",
            "investment",
            1_000.0,
            None,
        ),
    ];
    state.snapshot.transactions = transactions;
    state.snapshot.updated_at = "2026-10-17T12:00:00Z".into();
    state.documents = documents;
    state
}

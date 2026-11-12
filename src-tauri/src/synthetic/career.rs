//! A fixed synthetic career, as saved state, for the evidence tests and the assistant's eval
//! set: two jobs with a move between them, pay split across two accounts, a fee waiver that
//! lapses when deposits fall short, a fee repaid by a bonus, and one trip. Dates and amounts
//! never change, so tests can assert exact results.
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
/// The first job's last pay date, and the second job's first day.
const LEFT: (i32, u32, u32) = (2026, 2, 6);
const JOINED: (i32, u32, u32) = (2026, 3, 2);
const MOVED: (i32, u32, u32) = (2026, 2, 15);
const PREMIER_OPENED: (i32, u32, u32) = (2026, 4, 6);
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
        transaction.classification = Some(crate::transaction_policy::classify(
            &transaction,
            &crate::insights::benefits::Catalog::default(),
        ));
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
    let austin = Some(("Austin", "TX"));
    let seattle = Some(("Seattle", "WA"));
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
        // The second job pays twice a month, split across two accounts.
        let last_day = (day + Duration::days(1)).day() == 1;
        if day >= joined && (day.day() == 15 || last_day) {
            let september = day.year() == 2026 && day.month() == 9;
            let to_premier = if day < date(PREMIER_OPENED) {
                0.0
            } else if september {
                2_400.0
            } else {
                2_600.0
            };
            life.add(Entry {
                id: id("corp-pay"),
                account: "checking",
                day,
                amount: 4_100.0 - to_premier,
                merchant: "Example Corp",
                description: Some("EXAMPLE CORP SERV PAYROLL PPD ID: 9001"),
                category: "Income",
                detail: "INCOME_WAGES",
                place: None,
            });
            if to_premier > 0.0 {
                life.add(Entry {
                    id: id("corp-pay-premier"),
                    account: "premier",
                    day,
                    amount: to_premier,
                    merchant: "Example Corp",
                    description: Some("EXAMPLE CORP SERV PAYROLL PPD ID: 9001"),
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
            "rewards"
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
            austin
        } else if day >= moved {
            seattle
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
            place: austin,
        },
    );
    one(
        &mut life,
        Entry {
            id: "account-fee".into(),
            account: "checking",
            day: date((2026, 5, 22)),
            amount: -100.0,
            merchant: "Annual Account Fee",
            description: Some("ANNUAL ACCOUNT FEE"),
            category: "Bank Fees",
            detail: "BANK_FEES_OTHER_BANK_FEES",
            place: None,
        },
    );
    one(
        &mut life,
        Entry {
            id: "loyalty-bonus".into(),
            account: "checking",
            day: date((2026, 7, 15)),
            amount: 100.0,
            merchant: "Loyalty Bonus",
            description: Some("EXAMPLE BANK LOYALTY BONUS"),
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
    let corp = "Example Corp Services Group Inc.";
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
            (date((2026, 1, 19)), date((2026, 2, 1))),
            date(LEFT),
            &[2_600.0],
        ),
        w2(analytics, 2025, "TX"),
        payslip(
            "corp-first",
            corp,
            (date(JOINED), date((2026, 3, 15))),
            date((2026, 3, 15)),
            &[4_100.0],
        ),
        payslip(
            "corp-july",
            corp,
            (date((2026, 7, 1)), date((2026, 7, 15))),
            date((2026, 7, 15)),
            &[1_500.0, 2_600.0],
        ),
        payslip(
            "corp-september",
            corp,
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
            "premier",
            "Premier Checking - 5678",
            "Example Bank",
            "cash",
            18_400.0,
            Some("Premier Checking"),
        ),
        account(
            "rewards",
            "Example Card",
            "Example Issuer",
            "credit",
            -1_240.0,
            Some("The Example Card®"),
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
    // The brokerage holds one invented fund, its whole balance.
    state.snapshot.holdings = vec![serde_json::from_value(json!({
        "accountId": "plaid:brokerage",
        "ticker": "EXTM",
        "name": "Example Total Market ETF",
        "instrumentSubtype": "etf",
        "shares": 10,
        "price": 100.0,
        "value": 1_000.0,
        "costBasis": 950.0,
        "unrealizedGain": 50.0,
        "totalChangePct": 5.26,
        "color": "#272727",
    }))
    .expect("the career's holding is valid")];
    state.snapshot.transactions = transactions;
    state.snapshot.updated_at = "2026-10-17T12:00:00Z".into();
    state.documents = documents;
    state.settings.programs = vec![employee_waiver(), loyalty_bonus()];
    state.settings.card_rules = vec![crate::insights::benefits::example_rules()];
    state
}

/// An invented employer's fee waiver, as a rule the user approved from research: it exercises
/// employment, monthly deposits, a condition no record shows, a watch, and a job's end.
pub(crate) fn employee_waiver() -> crate::evidence::programs::Program {
    serde_json::from_value(json!({
        "id": "example-premier-employee-fee-waiver",
        "name": "Example Bank employee waiver",
        "organization": "Example Bank",
        "summary": "For Example Corp employees, the Premier Checking $25 monthly fee is avoided with $5,000 in monthly deposits and enrollment in online banking.",
        "source": { "url": "https://bank.example/employees", "title": "Employee banking | Example Bank" },
        "reviewed": "2026-10-17",
        "terms": [{ "from": "2025-01-01", "values": { "deposits": [5000], "fee": [25] } }],
        "conditions": [
            {
                "kind": "account",
                "as": "premier",
                "says": "A Premier Checking account at Example Bank",
                "name": "premier checking",
                "institution": "example bank"
            },
            {
                "kind": "employment",
                "as": "job",
                "says": "You work at Example Corp, by your pay statements or payroll",
                "organization": "example corp"
            },
            {
                "kind": "monthlyDeposits",
                "as": "months",
                "says": "At least the terms' deposits reached the account in the month, while you worked there",
                "account": "premier",
                "atLeastTerm": "deposits",
                "during": "job"
            },
            {
                "kind": "unobservable",
                "says": "Enrollment in Example Bank online banking, which no record Brief reads shows"
            }
        ],
        "conclusion": {
            "subject": "premier",
            "predicate": "feeWaived",
            "title": "Monthly fee waived",
            "says": "The $25 monthly fee is waived under the employee criteria",
            "spans": "months"
        },
        "watch": {
            "condition": "months",
            "says": "Deposits to Premier Checking were under {deposits} in {month}; the $25 monthly fee may apply."
        },
        "ends": {
            "with": "job",
            "says": "When work at Example Corp ends, the employee waiver lapses: the standard criteria apply, or the $25 monthly fee."
        }
    }))
    .expect("the career's waiver is a valid program")
}

/// An invented bank bonus that repays an account's annual fee: it exercises an account, two
/// dated transactions, a term amount, an offset, and an expected bonus.
pub(crate) fn loyalty_bonus() -> crate::evidence::programs::Program {
    serde_json::from_value(json!({
        "id": "example-checking-loyalty-bonus",
        "name": "Example Bank loyalty bonus",
        "organization": "Example Bank",
        "summary": "Example Bank pays a loyalty bonus into Everyday Checking after its annual fee.",
        "source": { "url": "https://bank.example/loyalty", "title": "Loyalty bonus | Example Bank" },
        "reviewed": "2026-10-17",
        "terms": [{ "from": "2026-01-01", "values": { "bonus": [100] } }],
        "conditions": [
            {
                "kind": "account",
                "as": "checking",
                "says": "An Everyday Checking account at Example Bank",
                "name": "everyday checking",
                "institution": "example bank"
            },
            {
                "kind": "transaction",
                "as": "bonus",
                "says": "A deposit to that account named a loyalty bonus, of the amount the terms give",
                "account": "checking",
                "description": "loyalty bonus",
                "direction": "in",
                "amountTerm": "bonus"
            },
            {
                "kind": "transaction",
                "as": "fee",
                "says": "The account's annual fee, charged in the 13 months before the bonus",
                "account": "checking",
                "description": "annual account fee|annual fee",
                "direction": "out",
                "before": { "of": "bonus", "days": 396 }
            }
        ],
        "conclusion": {
            "subject": "bonus",
            "predicate": "offsets",
            "object": "fee",
            "title": "Account fee offset",
            "says": "The {bonus} loyalty bonus offsets the account's {fee} annual fee"
        },
        "expect": {
            "missing": "bonus",
            "after": "fee",
            "days": 60,
            "says": "The loyalty bonus usually posts within two months of the annual fee; none has since the fee on {fee.date}."
        }
    }))
    .expect("the career's bonus is a valid program")
}

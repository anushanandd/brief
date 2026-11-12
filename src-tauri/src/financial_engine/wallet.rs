//! Apple Wallet's records in the snapshot.
//!
//! A card no provider reports (Apple Card) is Wallet's alone: its purchases take the shape of
//! Plaid records, so categories, the transaction policy, and every insight treat them like any
//! other. It joins as a spending-only account with no value: Wallet's balance is the whole
//! account's, and no payment to the card reaches Brief, so it can't be read as a liability. Its
//! purchases are spending, but never in net worth, debt, net cash flow, or the forecast.
//!
//! A card a provider also reports stays the provider's. Each Wallet record is matched to the
//! transaction it describes, which gains Wallet's exact time. The network's merchant code sets
//! the category only when Plaid is unsure, since a confident Plaid category is usually finer. Until the provider reports a
//! charge, its record stands in as pending; past that window it joins history only once the
//! user confirms it, and only while the provider still hasn't reported it, so a late posting is
//! never counted twice.
use std::sync::LazyLock;

use super::*;
use crate::{
    finance_contract::{
        WalletCard, WalletDeclined, WalletDetail, WalletReviewRow, WalletSummary,
        SPENDING_ONLY_PREFIX,
    },
    merchants::{same_merchant, Presence},
    places::{self, Place, PlacePrecision, PlaceSource},
    providers::wallet::{WalletData, WalletKind, WalletPlace, WalletRow, WalletStatus},
};

const APPLE_CARD: &str = "Apple Card";
/// What keeps Apple Card's purchases together; `SPENDING_ONLY_PREFIX` marks them.
const APPLE_CARD_KEY: &str = "apple-card";
/// A provider can take this long to report a charge; a record stands in as pending until then.
const PENDING_DAYS: i64 = 5;
/// A record and a provider's transaction of one amount are the same charge this close together.
const MATCH_DAYS: i64 = 5;
/// Of different amounts, they must be this close and name the same merchant: a tip, a hold.
const AMOUNT_DIFFERS_DAYS: i64 = 3;
/// How far back a refund or dispute credit finds the purchase it repays.
const REFUND_DAYS: i64 = 180;

/// Merchant category codes to Plaid's detailed codes, so one mapping decides every category.
static MERCHANT_CODES: LazyLock<BTreeMap<u32, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("merchant_codes.json")).expect("merchant codes are valid")
});

fn plaid_code(code: u32) -> Option<&'static str> {
    match code {
        // Networks give every airline, car rental chain, and hotel chain its own code.
        3000..=3299 => Some("TRAVEL_FLIGHTS"),
        3351..=3441 => Some("TRAVEL_RENTAL_CARS"),
        3501..=3999 => Some("TRAVEL_LODGING"),
        _ => MERCHANT_CODES.get(&code).map(String::as_str),
    }
}

fn category(code: &str) -> PlaidCategory {
    PlaidCategory {
        primary: "OTHER".into(),
        detailed: Some(code.into()),
        confidence_level: None,
    }
}

/// The record's amount in cents, positive for money out as Plaid's are.
fn outflow(row: &WalletRow) -> Decimal {
    row.amount().round_dp(2)
}

fn day(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn name(row: &WalletRow) -> &str {
    row.merchant
        .as_deref()
        .or(row.raw_merchant.as_deref())
        .unwrap_or("Transaction")
}

/// A name an issuer's feed prints in capitals, as a reader expects it.
fn readable(name: &str) -> String {
    if name.chars().any(char::is_lowercase) {
        name.to_owned()
    } else {
        categories::title_case(&name.replace(' ', "_"))
    }
}

fn place(saved: &WalletPlace) -> Option<Place> {
    let country = saved.country.clone()?;
    let town = saved
        .city
        .as_deref()
        .and_then(|city| places::lookup(city, &country, saved.region.as_deref()));
    let spot = saved
        .lat
        .zip(saved.lon)
        .or_else(|| town.map(|town| (town.lat, town.lon)));
    let exact = saved.lat.is_some() && saved.street.is_some();
    Some(Place {
        address: saved.street.clone(),
        postal_code: saved.postal_code.clone(),
        city: saved.city.clone(),
        region: saved.region.clone(),
        country,
        lat: spot.map(|(lat, _)| lat),
        lon: spot.map(|(_, lon)| lon),
        source: PlaceSource::Wallet,
        precision: if exact {
            PlacePrecision::Exact
        } else if saved.street.is_some() {
            PlacePrecision::Street
        } else if saved.city.is_some() {
            PlacePrecision::City
        } else {
            PlacePrecision::Region
        },
    })
}

fn detail(row: &WalletRow) -> WalletDetail {
    WalletDetail {
        record_id: row.id.clone(),
        method: row.method,
        foreign: row.foreign.clone(),
        reward: row.reward.clone(),
        phone: row.phone.clone(),
        network: row.network.clone(),
        presence: row.in_person.map(|there| {
            if there {
                Presence::InPerson
            } else {
                Presence::Remote
            }
        }),
        place: row.place.as_ref().and_then(place),
        refund_of: None,
        terminal_amount: None,
        merchant: row.merchant.as_deref().map(readable),
        categorized: false,
    }
}

/// A record in the shape of a Plaid transaction, so it is categorized and shown like one.
fn record(
    row: &WalletRow,
    account_id: &str,
    merchant: &str,
    filed: Option<PlaidCategory>,
    pending: bool,
) -> PlaidTransaction {
    PlaidTransaction {
        transaction_id: format!("{SPENDING_ONLY_PREFIX}{}", row.id),
        account_id: account_id.to_owned(),
        amount: outflow(row),
        name: row
            .raw_merchant
            .clone()
            .or_else(|| Some(merchant.to_owned())),
        merchant_name: Some(merchant.to_owned()),
        logo_url: None,
        website: row.website.clone(),
        pending,
        date: Some(row.posted_on.clone().unwrap_or_else(|| row.date.clone())),
        datetime: None,
        authorized_date: Some(row.date.clone()),
        authorized_datetime: None,
        location: None,
        payment_channel: row
            .in_person
            .map(|there| if there { "in store" } else { "online" }.to_owned()),
        category: Some(vec!["Other".into()]),
        personal_finance_category: filed,
        counterparties: None,
        transaction_code: None,
        pending_transaction_id: None,
        merchant_entity_id: None,
        original_description: None,
        check_number: None,
        account_owner: None,
        iso_currency_code: Some("USD".into()),
        unofficial_currency_code: None,
    }
}

/// The transaction a record stands for, on the named account.
fn shown(
    row: &WalletRow,
    account: (&str, &str),
    merchant: &str,
    filed: Option<PlaidCategory>,
    pending: bool,
) -> Option<Transaction> {
    let (account_id, account_name) = account;
    let names = BTreeMap::from([(account_id, account_name.to_owned())]);
    let transaction = plaid_transaction(record(row, account_id, merchant, filed, pending), &names)?;
    Some(Transaction {
        account_id: Some(account_id.to_owned()),
        occurred_at: row.occurred_at.clone(),
        wallet: Some(detail(row)),
        ..transaction
    })
}

/// An Apple Card record as a transaction. A record that moves no money of the user's, such as
/// Daily Cash taken back after a return, has none.
fn apple_card_transaction(row: &WalletRow, account_id: &str) -> Option<Transaction> {
    let pending = row.status == WalletStatus::Pending;
    let (merchant, filed) = match row.kind {
        WalletKind::Purchase | WalletKind::Refund => (
            name(row).to_owned(),
            row.merchant_code.and_then(plaid_code).map(category),
        ),
        WalletKind::Adjustment => match row.adjustment.as_deref() {
            Some("provisionalDispute") => ("Dispute credit".to_owned(), None),
            Some("provisionalDisputeReversal") => ("Dispute reversal".to_owned(), None),
            _ => return None,
        },
        WalletKind::Other => return None,
    };
    shown(row, (account_id, APPLE_CARD), &merchant, filed, pending)
}

/// Whether `purchase` is what `refund` repays. A dispute names no merchant, so its amount has
/// to be the purchase's.
fn repays(refund: &Transaction, purchase: &Transaction) -> bool {
    if purchase.amount >= 0.0 || refund.amount <= 0.0 {
        return false;
    }
    let days = day(&refund.date)
        .zip(day(&purchase.date))
        .map(|(refund, purchase)| (refund - purchase).num_days());
    if !days.is_some_and(|days| (0..=REFUND_DAYS).contains(&days)) {
        return false;
    }
    let (paid, repaid) = (
        crate::numeric::minor_units(-purchase.amount),
        crate::numeric::minor_units(refund.amount),
    );
    if refund.merchant == "Dispute credit" {
        paid == repaid
    } else {
        purchase.merchant == refund.merchant && paid >= repaid
    }
}

/// Points each refund and dispute credit at the latest purchase it repays, each once.
fn link_refunds(transactions: &mut [Transaction]) {
    let mut taken = BTreeSet::new();
    for index in 0..transactions.len() {
        let refund = &transactions[index];
        let purchase = transactions
            .iter()
            .filter(|purchase| repays(refund, purchase) && !taken.contains(&purchase.id))
            .max_by(|left, right| left.date.cmp(&right.date))
            .map(|purchase| purchase.id.clone());
        if let Some(purchase) = purchase {
            taken.insert(purchase.clone());
            if let Some(wallet) = transactions[index].wallet.as_mut() {
                wallet.refund_of = Some(purchase);
            }
        }
    }
}

/// How many days apart a record and a provider's transaction are, by the nearer of its dates.
fn days_apart(row: NaiveDate, record: &PlaidTransaction) -> Option<i64> {
    [
        plaid_posted_date(record),
        iso_date(record.authorized_date.as_deref()),
    ]
    .into_iter()
    .flatten()
    .filter_map(|date| day(&date))
    .map(|date| (date - row).num_days().abs())
    .min()
}

fn names(record: &PlaidTransaction) -> impl Iterator<Item = &str> {
    [
        record.merchant_name.as_deref(),
        record.name.as_deref(),
        record.original_description.as_deref(),
    ]
    .into_iter()
    .flatten()
}

fn description(record: &PlaidTransaction) -> String {
    names(record).collect::<Vec<_>>().join(" ")
}

/// Whether the record names the provider's merchant. Each of the provider's names is compared
/// on its own: run together, they hide a name ("EXAMPLE PBC" isn't in "Example EXAMPLE* PRO").
fn names_merchant(row: &WalletRow, record: &PlaidTransaction) -> bool {
    names(record).any(|other| same_merchant(name(row), other))
}

/// What a matched record adds to a provider's transaction.
struct Matched {
    occurred_at: Option<String>,
    detail: WalletDetail,
    pending_id: String,
    /// The network's merchant code as Plaid's detailed category.
    code: Option<&'static str>,
}

/// The provider's transaction each record describes: first by amount, then, for what's left, by
/// merchant at another amount. Each transaction answers for one record.
fn matches<'a>(
    rows: &[&'a WalletRow],
    candidates: &[&'a PlaidTransaction],
) -> BTreeMap<&'a str, &'a PlaidTransaction> {
    let mut found = BTreeMap::<&str, &PlaidTransaction>::new();
    let mut taken = BTreeSet::<&str>::new();
    for row in rows {
        let Some(date) = day(&row.date) else { continue };
        let best = candidates
            .iter()
            .filter(|candidate| {
                candidate.amount == outflow(row)
                    && !taken.contains(candidate.transaction_id.as_str())
            })
            .filter_map(|candidate| {
                let days = days_apart(date, candidate).filter(|days| *days <= MATCH_DAYS)?;
                let alike = crate::statements::similarity(name(row), &description(candidate));
                Some((days, std::cmp::Reverse(alike), *candidate))
            })
            .min_by_key(|(days, alike, _)| (*days, *alike));
        if let Some((.., candidate)) = best {
            taken.insert(candidate.transaction_id.as_str());
            found.insert(row.id.as_str(), candidate);
        }
    }
    for row in rows {
        if found.contains_key(row.id.as_str()) {
            continue;
        }
        let Some(date) = day(&row.date) else { continue };
        let best = candidates
            .iter()
            .filter(|candidate| {
                !taken.contains(candidate.transaction_id.as_str()) && names_merchant(row, candidate)
            })
            .filter_map(|candidate| {
                let days = days_apart(date, candidate).filter(|d| *d <= AMOUNT_DIFFERS_DAYS)?;
                Some((days, *candidate))
            })
            .min_by_key(|(days, _)| *days);
        if let Some((_, candidate)) = best {
            taken.insert(candidate.transaction_id.as_str());
            found.insert(row.id.as_str(), candidate);
        }
    }
    found
}

/// The one credit card a provider reports for this issuer. With several, nothing says which a
/// record belongs to, so they stay unplaced.
fn provider_card<'a>(accounts: &'a [Account], issuer: &str) -> Option<&'a Account> {
    let issuer = issuer.to_ascii_lowercase();
    let mut cards = accounts.iter().filter(|account| {
        account.r#type == "credit"
            && account.id.starts_with("plaid:")
            && account.institution.to_ascii_lowercase().contains(&issuer)
    });
    let card = cards.next()?;
    cards.next().is_none().then_some(card)
}

#[derive(Default)]
pub(super) struct Projected {
    /// Records that are transactions of their own.
    pub(super) transactions: Vec<Transaction>,
    /// By provider transaction id.
    matched: BTreeMap<String, Matched>,
    pub(super) summary: Option<WalletSummary>,
}

impl Projected {
    /// Files a provider's transaction under the network's merchant code when Plaid isn't sure
    /// of its own category: the code is what the merchant registered, Plaid's is a guess. A sure
    /// Plaid category stays, since it's often finer (coffee, not restaurants).
    pub(super) fn categorize(&mut self, transactions: &mut [PlaidTransaction]) {
        for transaction in transactions {
            let Some(matched) = self.matched.get_mut(&transaction.transaction_id) else {
                continue;
            };
            let sure = transaction
                .personal_finance_category
                .as_ref()
                .is_some_and(|filed| {
                    matches!(
                        filed.confidence_level.as_deref(),
                        Some("VERY_HIGH" | "HIGH")
                    )
                });
            if let Some(code) = matched.code.filter(|_| !sure) {
                transaction.personal_finance_category = Some(category(code));
                matched.detail.categorized = true;
            }
        }
    }

    /// A provider's transaction with what Wallet recorded about it.
    pub(super) fn enrich(&mut self, transaction: Transaction) -> Transaction {
        let Some(matched) = self.matched.remove(&transaction.id) else {
            return transaction;
        };
        let terminal_amount = matched.detail.terminal_amount;
        Transaction {
            occurred_at: matched.occurred_at,
            wallet: Some(WalletDetail {
                terminal_amount: terminal_amount.filter(|amount| {
                    crate::numeric::minor_units(*amount)
                        != crate::numeric::minor_units(transaction.amount)
                }),
                ..matched.detail
            }),
            // Edits made while the record stood in carry over.
            pending_transaction_id: transaction
                .pending_transaction_id
                .clone()
                .or(Some(matched.pending_id)),
            ..transaction
        }
    }
}

/// A category for a record that has none: what the card's own history files that merchant under.
fn filed_like(row: &WalletRow, history: &[&PlaidTransaction]) -> Option<PlaidCategory> {
    history
        .iter()
        .find(|record| names_merchant(row, record))
        .and_then(|record| record.personal_finance_category.as_ref())
        .map(|filed| PlaidCategory {
            primary: filed.primary.clone(),
            detailed: filed.detailed.clone(),
            confidence_level: filed.confidence_level.clone(),
        })
}

pub(super) fn project_wallet(
    data: Option<&WalletData>,
    accounts: &[Account],
    plaid: &[PlaidTransaction],
    today: NaiveDate,
) -> Projected {
    let Some(data) = data else {
        return Projected::default();
    };
    let mut projected = Projected::default();
    let mut cards = BTreeMap::<&str, WalletCard>::new();
    let mut review = Vec::new();
    let mut declined = Vec::new();
    let mut by_issuer = BTreeMap::<&str, Vec<&WalletRow>>::new();
    for row in &data.rows {
        let card = cards.entry(&row.issuer).or_insert_with(|| WalletCard {
            issuer: row.issuer.clone(),
            ..WalletCard::default()
        });
        match row.status {
            WalletStatus::Declined => {
                declined.push(WalletDeclined {
                    date: row.date.clone(),
                    amount: money(-outflow(row)),
                    merchant: name(row).to_owned(),
                });
                continue;
            }
            WalletStatus::Other => continue,
            WalletStatus::Approved | WalletStatus::Pending => {}
        }
        card.records += 1;
        by_issuer.entry(&row.issuer).or_default().push(row);
    }
    let apple_account = format!("{SPENDING_ONLY_PREFIX}{APPLE_CARD_KEY}");
    for (issuer, mut rows) in by_issuer {
        // Oldest first, so an earlier charge takes the earlier of two equal transactions.
        rows.sort_by(|left, right| left.date.cmp(&right.date));
        if issuer == APPLE_CARD {
            let mut transactions = rows
                .iter()
                .filter_map(|row| apple_card_transaction(row, &apple_account))
                .collect::<Vec<_>>();
            link_refunds(&mut transactions);
            projected.transactions.extend(transactions);
            continue;
        }
        let Some(account) = provider_card(accounts, issuer) else {
            continue;
        };
        let raw = account.id.trim_start_matches("plaid:");
        let history = plaid
            .iter()
            .filter(|record| record.account_id == raw)
            .collect::<Vec<_>>();
        let found = matches(&rows, &history);
        let card = cards.entry(issuer).or_default();
        for row in rows {
            if let Some(candidate) = found.get(row.id.as_str()) {
                card.matched += 1;
                projected.matched.insert(
                    candidate.transaction_id.clone(),
                    Matched {
                        occurred_at: row.occurred_at.clone(),
                        detail: WalletDetail {
                            terminal_amount: Some(money(-outflow(row))),
                            ..detail(row)
                        },
                        pending_id: format!("{SPENDING_ONLY_PREFIX}{}", row.id),
                        code: row.merchant_code.and_then(plaid_code),
                    },
                );
                continue;
            }
            let age = day(&row.date).map(|date| (today - date).num_days());
            let waiting = age.is_some_and(|age| age <= PENDING_DAYS);
            let confirmed = data.confirmed.contains(&row.id);
            if !waiting {
                card.review += 1;
                review.push(WalletReviewRow {
                    id: row.id.clone(),
                    account: account.name.clone(),
                    date: row.date.clone(),
                    amount: money(-outflow(row)),
                    merchant: readable(name(row)),
                    confirmed,
                });
            } else {
                card.waiting += 1;
            }
            if waiting || confirmed {
                projected.transactions.extend(shown(
                    row,
                    (&account.id, &account.name),
                    &readable(name(row)),
                    filed_like(row, &history),
                    waiting,
                ));
            }
        }
    }
    projected.summary = Some(WalletSummary {
        read_at: data.read_at.clone(),
        history_from: data
            .rows
            .iter()
            .filter(|row| row.status == WalletStatus::Approved)
            .map(|row| row.date.clone())
            .min(),
        cards: cards.into_values().collect(),
        review,
        declined,
        daily_cash: data.daily_cash.clone(),
        apple_card: data.apple_card.clone(),
    });
    projected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finance_contract::WalletMethod;
    use serde_json::json;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 30).unwrap()
    }

    fn apple(id: &str, date: &str, amount: i64, merchant: &str) -> WalletRow {
        WalletRow {
            id: id.into(),
            issuer: APPLE_CARD.into(),
            amount: amount * 100,
            date: date.into(),
            merchant: Some(merchant.into()),
            merchant_code: Some(5411),
            method: Some(WalletMethod::Tap),
            in_person: Some(true),
            place: Some(WalletPlace {
                street: Some("100 Main St".into()),
                city: Some("Austin".into()),
                region: Some("TX".into()),
                country: Some("US".into()),
                lat: Some(30.27),
                lon: Some(-97.74),
                ..WalletPlace::default()
            }),
            ..WalletRow::default()
        }
    }

    fn issued(id: &str, date: &str, cents: i64, merchant: &str) -> WalletRow {
        WalletRow {
            id: id.into(),
            issuer: "Example Bank".into(),
            amount: cents * 100,
            date: date.into(),
            merchant: Some(merchant.into()),
            occurred_at: Some(format!("{date}T14:08:53-07:00")),
            ..WalletRow::default()
        }
    }

    fn accounts() -> Vec<Account> {
        vec![Account {
            id: "plaid:issued".into(),
            name: "Rewards".into(),
            institution: "Example Bank".into(),
            r#type: "credit".into(),
            ..Account::default()
        }]
    }

    fn plaid(id: &str, date: &str, amount: f64, name: &str, pending: bool) -> PlaidTransaction {
        serde_json::from_value(json!({
            "transaction_id": id, "account_id": "issued", "amount": amount, "pending": pending,
            "date": date, "name": name, "merchant_name": name,
            "personal_finance_category": {
                "primary": "FOOD_AND_DRINK", "detailed": "FOOD_AND_DRINK_GROCERIES"
            }
        }))
        .unwrap()
    }

    fn data(rows: Vec<WalletRow>) -> WalletData {
        WalletData {
            read_at: "2026-10-30T12:00:00Z".into(),
            rows,
            ..WalletData::default()
        }
    }

    #[test]
    fn every_merchant_code_names_a_category_that_exists() {
        for (code, plaid) in MERCHANT_CODES.iter() {
            assert!(
                categories::kind_for(plaid).is_some() && plaid != "OTHER_OTHER",
                "{code} maps to {plaid}"
            );
        }
        let groceries = categories::categorize("OTHER", plaid_code(5411), None);
        assert_eq!(groceries.category, "Groceries");
        assert_eq!(plaid_code(3132), Some("TRAVEL_FLIGHTS"));
    }

    #[test]
    fn apple_card_purchases_are_spending_on_their_own_account_with_wallets_place() {
        let projected = project_wallet(
            Some(&data(vec![apple(
                "one",
                "2025-04-28",
                4873,
                "Corner Market",
            )])),
            &[],
            &[],
            today(),
        );
        let [transaction] = projected.transactions.as_slice() else {
            panic!("one transaction");
        };
        assert_eq!(transaction.id, "wallet:one");
        assert_eq!(transaction.account, APPLE_CARD);
        assert_eq!(transaction.account_id.as_deref(), Some("wallet:apple-card"));
        assert_eq!(transaction.amount, -48.73);
        assert_eq!(transaction.category, "Groceries");
        assert_eq!(transaction.merchant, "Corner Market");
        assert!(transaction.paid_by_someone_else());
        let wallet = transaction.wallet.as_ref().unwrap();
        assert_eq!(wallet.presence, Some(Presence::InPerson));
        let place = wallet.place.as_ref().unwrap();
        assert_eq!(place.source, PlaceSource::Wallet);
        assert_eq!(place.precision, PlacePrecision::Exact);
        assert_eq!(place.address.as_deref(), Some("100 Main St"));
    }

    #[test]
    fn a_refund_repays_the_purchase_it_followed_and_a_dispute_matches_by_amount() {
        let mut refund = apple("back", "2025-05-02", -2000, "Corner Market");
        refund.kind = WalletKind::Refund;
        let mut dispute = apple("claim", "2025-05-03", -999, "ignored");
        dispute.kind = WalletKind::Adjustment;
        dispute.adjustment = Some("provisionalDispute".into());
        let mut clawback = apple("clawback", "2025-05-03", 5, "ignored");
        clawback.kind = WalletKind::Adjustment;
        clawback.adjustment = Some("other".into());
        let projected = project_wallet(
            Some(&data(vec![
                apple("bought", "2025-04-28", 2000, "Corner Market"),
                apple("other", "2025-04-29", 999, "Elsewhere"),
                refund,
                dispute,
                clawback,
            ])),
            &[],
            &[],
            today(),
        );
        // Daily Cash taken back moves none of the user's money.
        assert_eq!(projected.transactions.len(), 4);
        let of = |id: &str| {
            let transaction = projected.transactions.iter().find(|t| t.id == id).unwrap();
            transaction.wallet.as_ref().unwrap().refund_of.clone()
        };
        assert_eq!(of("wallet:back").as_deref(), Some("wallet:bought"));
        assert_eq!(of("wallet:claim").as_deref(), Some("wallet:other"));
    }

    #[test]
    fn declined_charges_never_become_transactions() {
        let mut row = apple("no", "2025-11-28", 6999, "Corner Market");
        row.status = WalletStatus::Declined;
        let projected = project_wallet(Some(&data(vec![row])), &[], &[], today());
        assert!(projected.transactions.is_empty());
        let summary = projected.summary.unwrap();
        assert_eq!(summary.declined[0].amount, -69.99);
        assert_eq!(summary.cards[0].records, 0);
    }

    #[test]
    fn a_matched_record_adds_its_time_to_the_providers_transaction_and_counts_once() {
        let records = [plaid("p1", "2026-10-13", 27.65, "Corner Market", false)];
        let mut projected = project_wallet(
            Some(&data(vec![issued(
                "one",
                "2026-10-13",
                2765,
                "CORNER MARKET",
            )])),
            &accounts(),
            &records,
            today(),
        );
        assert!(projected.transactions.is_empty());
        let summary = projected.summary.take().unwrap();
        assert_eq!(summary.cards[0].matched, 1);
        let enriched = projected.enrich(Transaction {
            id: "p1".into(),
            amount: -27.65,
            ..Transaction::default()
        });
        assert_eq!(
            enriched.occurred_at.as_deref(),
            Some("2026-10-13T14:08:53-07:00")
        );
        assert_eq!(
            enriched.pending_transaction_id.as_deref(),
            Some("wallet:one")
        );
        assert_eq!(enriched.wallet.unwrap().terminal_amount, None);
    }

    #[test]
    fn a_tip_keeps_the_providers_amount_and_remembers_the_terminals() {
        let records = [plaid(
            "p1",
            "2026-10-14",
            36.0,
            "Chili's Grill & Bar",
            false,
        )];
        let mut projected = project_wallet(
            Some(&data(vec![issued("one", "2026-10-13", 3000, "CHILIS")])),
            &accounts(),
            &records,
            today(),
        );
        let enriched = projected.enrich(Transaction {
            id: "p1".into(),
            amount: -36.0,
            ..Transaction::default()
        });
        assert_eq!(enriched.amount, -36.0);
        assert_eq!(enriched.wallet.unwrap().terminal_amount, Some(-30.0));
    }

    #[test]
    fn the_merchant_code_files_a_charge_only_when_plaid_is_unsure() {
        let sure = |level: &str| {
            let mut record = plaid(level, "2026-10-13", 27.65, "Corner Market", false);
            record
                .personal_finance_category
                .as_mut()
                .unwrap()
                .confidence_level = Some(level.into());
            record
        };
        let mut records = [sure("VERY_HIGH"), sure("LOW")];
        let dining = |id: &str| WalletRow {
            merchant_code: Some(5812),
            ..issued(id, "2026-10-13", 2765, "Corner Market")
        };
        let mut projected = project_wallet(
            Some(&data(vec![dining("one"), dining("two")])),
            &accounts(),
            &records,
            today(),
        );
        projected.categorize(&mut records);
        let detailed = |record: &PlaidTransaction| {
            record
                .personal_finance_category
                .as_ref()
                .unwrap()
                .detailed
                .clone()
        };
        assert_eq!(
            detailed(&records[0]).as_deref(),
            Some("FOOD_AND_DRINK_GROCERIES")
        );
        assert_eq!(detailed(&records[1]).as_deref(), plaid_code(5812));
        let mut categorized = |id: &str| {
            let enriched = projected.enrich(Transaction {
                id: id.into(),
                amount: -27.65,
                ..Transaction::default()
            });
            enriched.wallet.unwrap().categorized
        };
        assert!(!categorized("VERY_HIGH"));
        assert!(categorized("LOW"));
    }

    #[test]
    fn a_name_with_a_suffix_matches_a_charge_whose_descriptor_runs_on() {
        let mut record = plaid("p1", "2026-10-13", 95.89, "Example Labs", false);
        record.name = Some("EXAMPLE LABS* PRO PORTLAND".into());
        record.original_description = Some("EXAMPLE LABS* PRO PORTLAND OR".into());
        let projected = project_wallet(
            Some(&data(vec![issued(
                "one",
                "2026-10-13",
                9567,
                "EXAMPLE LABS PBC",
            )])),
            &accounts(),
            &[record],
            today(),
        );
        assert!(projected.transactions.is_empty());
        assert_eq!(projected.summary.unwrap().cards[0].matched, 1);
    }

    #[test]
    fn two_equal_charges_each_take_their_own_transaction() {
        let records = [
            plaid("first", "2026-10-10", 5.0, "Corner Market", false),
            plaid("second", "2026-10-12", 5.0, "Corner Market", false),
        ];
        let projected = project_wallet(
            Some(&data(vec![
                issued("a", "2026-10-10", 500, "CORNER MARKET"),
                issued("b", "2026-10-12", 500, "CORNER MARKET"),
            ])),
            &accounts(),
            &records,
            today(),
        );
        assert!(projected.matched.contains_key("first"));
        assert!(projected.matched.contains_key("second"));
        assert_eq!(projected.summary.unwrap().cards[0].matched, 2);
    }

    #[test]
    fn a_charge_the_provider_hasnt_reported_stands_in_as_pending_filed_like_its_history() {
        let records = [plaid("p1", "2026-10-12", 30.42, "Safeway", false)];
        let projected = project_wallet(
            Some(&data(vec![issued("new", "2026-10-29", 1599, "SAFEWAY")])),
            &accounts(),
            &records,
            today(),
        );
        let [transaction] = projected.transactions.as_slice() else {
            panic!("one transaction");
        };
        assert!(transaction.pending);
        assert!(transaction.wallet_only());
        assert!(!transaction.paid_by_someone_else());
        assert_eq!(transaction.account_id.as_deref(), Some("plaid:issued"));
        assert_eq!(transaction.merchant, "Safeway");
        assert_eq!(transaction.category, "Groceries");
        assert_eq!(projected.summary.unwrap().cards[0].waiting, 1);
    }

    #[test]
    fn past_the_window_a_record_leaves_totals_until_the_user_confirms_it() {
        let row = issued("late", "2026-10-20", 1599, "SAFEWAY");
        let unconfirmed = project_wallet(Some(&data(vec![row.clone()])), &accounts(), &[], today());
        assert!(unconfirmed.transactions.is_empty());
        let summary = unconfirmed.summary.unwrap();
        assert_eq!(summary.review[0].merchant, "Safeway");
        assert!(!summary.review[0].confirmed);

        let mut saved = data(vec![row]);
        saved.confirmed.insert("late".into());
        let confirmed = project_wallet(Some(&saved), &accounts(), &[], today());
        assert!(!confirmed.transactions[0].pending);
        assert!(confirmed.summary.unwrap().review[0].confirmed);
    }

    #[test]
    fn a_late_posting_replaces_a_confirmed_record_rather_than_joining_it() {
        let mut saved = data(vec![issued("late", "2026-10-20", 1599, "SAFEWAY")]);
        saved.confirmed.insert("late".into());
        let records = [plaid("p1", "2026-10-22", 15.99, "Safeway", false)];
        let projected = project_wallet(Some(&saved), &accounts(), &records, today());
        assert!(projected.transactions.is_empty());
        assert!(projected.summary.unwrap().review.is_empty());
    }

    #[test]
    fn a_card_no_single_provider_account_answers_for_stays_unplaced() {
        let mut two = accounts();
        two.push(Account {
            id: "plaid:issued-two".into(),
            ..two[0].clone()
        });
        let projected = project_wallet(
            Some(&data(vec![issued("new", "2026-10-29", 1599, "SAFEWAY")])),
            &two,
            &[],
            today(),
        );
        assert!(projected.transactions.is_empty());
        assert_eq!(projected.summary.unwrap().cards[0].records, 1);
    }
}

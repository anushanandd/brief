//! The dated things Brief knows about, one list for the Calendar page and the Brief calendar:
//! card payments due and statements closing, the forecast's bills, subscriptions, and income,
//! card credits running out, announced dividends, lots turning long-term, wash-sale windows
//! closing, and trips. Each item reuses a result derived elsewhere; nothing here estimates.
use chrono::{Duration, NaiveDate};
use serde::Serialize;
use ts_rs::TS;

use super::attention::payment_owed;
use super::benefits::BenefitWindow;
use super::credit::Credit;
use super::dividends::UpcomingDividend;
use super::forecast::{EventKind, EventState, ExpectedEvent};
use super::lots::LongTermSoon;
use super::tax::WashWindow;
use super::trips::Trip;
use crate::finance_contract::Account;

/// How far ahead dated items are listed: the cash forecast's horizon.
pub const HORIZON_DAYS: i64 = super::cashline::HORIZON_DAYS;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, TS, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum AgendaKind {
    PaymentDue,
    StatementCloses,
    CreditExpiring,
    Bill,
    Subscription,
    Income,
    Dividend,
    LongTerm,
    WashSaleEnds,
    Trip,
}

#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgendaItem {
    /// Its kind, subject, and date, so it stays the same across refreshes while it means the
    /// same thing.
    pub id: String,
    pub kind: AgendaKind,
    pub date: NaiveDate,
    /// The last day of an item that spans days: a trip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end: Option<NaiveDate>,
    /// The series, benefit, or destination it concerns; accounts and tickers are named by the
    /// renderer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ticker: Option<String>,
    /// As cash moves: negative for money out. Absent when unknown, never zero instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub amount: Option<f64>,
    /// Brief inferred the date or amount; the institution didn't report it.
    pub estimated: bool,
    /// How often a forecast series recurs: `monthly`, `biweekly`, and so on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frequency: Option<&'static str>,
    /// The range a forecast amount has posted in, when it isn't one amount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub amount_low: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub amount_high: Option<f64>,
    /// A payment due's minimum, as the issuer reported it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub minimum_payment: Option<f64>,
}

impl AgendaItem {
    fn new(kind: AgendaKind, key: &str, date: NaiveDate) -> Self {
        Self {
            id: format!("{}:{key}:{date}", kind_key(kind)),
            kind,
            date,
            end: None,
            title: None,
            account_id: None,
            ticker: None,
            amount: None,
            estimated: false,
            frequency: None,
            amount_low: None,
            amount_high: None,
            minimum_payment: None,
        }
    }
}

fn kind_key(kind: AgendaKind) -> &'static str {
    match kind {
        AgendaKind::PaymentDue => "due",
        AgendaKind::StatementCloses => "close",
        AgendaKind::CreditExpiring => "credit",
        AgendaKind::Bill => "bill",
        AgendaKind::Subscription => "subscription",
        AgendaKind::Income => "income",
        AgendaKind::Dividend => "dividend",
        AgendaKind::LongTerm => "long-term",
        AgendaKind::WashSaleEnds => "wash",
        AgendaKind::Trip => "trip",
    }
}

pub struct Inputs<'a> {
    pub today: NaiveDate,
    pub accounts: &'a [Account],
    /// The forecast over `HORIZON_DAYS`, with each series' later occurrences.
    pub forecast: &'a [ExpectedEvent],
    pub credit: &'a Credit,
    /// Each tracked card's benefit windows, by account.
    pub benefits: Vec<(&'a str, &'a [BenefitWindow])>,
    pub hidden_benefit_ids: &'a [String],
    pub dividends: &'a [UpcomingDividend],
    pub long_term_soon: &'a [LongTermSoon],
    pub wash_windows: &'a [WashWindow],
    pub trips: &'a [Trip],
}

/// Every trip, and the other items from today through the horizon, by date.
pub fn agenda(inputs: Inputs<'_>) -> Vec<AgendaItem> {
    let today = inputs.today;
    let last = today + Duration::days(HORIZON_DAYS);
    let ahead = |date: NaiveDate| (today..=last).contains(&date);
    let mut items = Vec::new();

    for account in inputs
        .accounts
        .iter()
        .filter(|account| account.r#type == "credit")
    {
        let Some(liability) = &account.liability else {
            continue;
        };
        let due = liability
            .next_payment_due_date
            .as_deref()
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok());
        if let Some(due) = due.filter(|due| ahead(*due) && payment_owed(liability)) {
            let mut item = AgendaItem::new(AgendaKind::PaymentDue, &account.id, due);
            item.account_id = Some(account.id.clone());
            item.amount = liability.last_statement_balance.map(|balance| -balance);
            item.minimum_payment = liability.minimum_payment;
            items.push(item);
        }
    }
    for card in &inputs.credit.cards {
        if let Some(closes) = card.closes_on.filter(|date| ahead(*date)) {
            let mut item = AgendaItem::new(AgendaKind::StatementCloses, &card.account_id, closes);
            item.account_id = Some(card.account_id.clone());
            item.estimated = card.closes_estimated;
            items.push(item);
        }
    }
    for (account_id, windows) in &inputs.benefits {
        for window in windows.iter() {
            let left = window
                .remaining_range
                .as_ref()
                .map(|range| range.high)
                .or(window.remaining_amount);
            // A subscription spends its credit on its own; only deadlines that need action.
            if window.recurring
                || !window.cadence.is_calendar()
                || !ahead(window.window_end)
                || inputs.hidden_benefit_ids.contains(&window.id)
            {
                continue;
            }
            let Some(left) = left.filter(|left| *left > 0.0) else {
                continue;
            };
            let key = format!("{account_id}:{}", window.id);
            let mut item = AgendaItem::new(AgendaKind::CreditExpiring, &key, window.window_end);
            item.title = Some(window.name.clone());
            item.account_id = Some((*account_id).to_owned());
            item.amount = Some(left);
            item.estimated = window.remaining_amount.is_none();
            items.push(item);
        }
    }
    for event in inputs.forecast {
        let kind = match event.kind {
            EventKind::Payment => AgendaKind::Bill,
            EventKind::Subscription => AgendaKind::Subscription,
            EventKind::Income => AgendaKind::Income,
            // Card credits post on their own after a purchase; the benefit is the deadline.
            EventKind::Credit => continue,
        };
        if event.state == EventState::Missed {
            continue;
        }
        for date in std::iter::once(event.date)
            .chain(event.later.iter().copied())
            .filter(|date| ahead(*date))
        {
            let mut item = AgendaItem::new(kind, &event.recurring_id, date);
            item.title = Some(event.title.clone());
            item.account_id = event.account_id.clone();
            item.amount = Some(event.amount);
            item.estimated = true;
            item.frequency = Some(event.frequency);
            if event.amount_low != event.amount_high {
                item.amount_low = Some(event.amount_low);
                item.amount_high = Some(event.amount_high);
            }
            items.push(item);
        }
    }
    for upcoming in inputs.dividends {
        let Some(paid) = upcoming.dividend.payable_date.filter(|date| ahead(*date)) else {
            continue;
        };
        let mut item = AgendaItem::new(AgendaKind::Dividend, &upcoming.ticker, paid);
        item.ticker = Some(upcoming.ticker.clone());
        item.amount = Some(upcoming.dividend.amount);
        items.push(item);
    }
    for lot in inputs.long_term_soon.iter().filter(|lot| ahead(lot.date)) {
        let key = format!("{}:{}", lot.account_id, lot.ticker);
        let mut item = AgendaItem::new(AgendaKind::LongTerm, &key, lot.date);
        item.ticker = Some(lot.ticker.clone());
        item.account_id = Some(lot.account_id.clone());
        items.push(item);
    }
    for window in inputs
        .wash_windows
        .iter()
        .filter(|window| ahead(window.until))
    {
        let mut item = AgendaItem::new(AgendaKind::WashSaleEnds, &window.sale_id, window.until);
        item.ticker = Some(window.ticker.clone());
        item.account_id = Some(window.account_id.clone());
        items.push(item);
    }
    for trip in inputs.trips {
        let mut item = AgendaItem::new(AgendaKind::Trip, &trip.id, trip.start);
        item.end = Some(trip.end);
        item.title = Some(trip.destination.clone());
        item.amount = Some(trip.total);
        items.push(item);
    }
    items.sort_by(|left, right| {
        (left.date, left.kind, &left.id).cmp(&(right.date, right.kind, &right.id))
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::forecast::EventState;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn event(kind: EventKind, date: &str, later: &[&str], state: EventState) -> ExpectedEvent {
        ExpectedEvent {
            id: "event".into(),
            recurring_id: "series".into(),
            title: "Rent".into(),
            kind,
            date: day(date),
            amount: -1_200.0,
            amount_low: -1_200.0,
            amount_high: -1_200.0,
            frequency: "monthly",
            observations: 6,
            last_seen: day("2026-02-01"),
            state,
            account_id: Some("checking".into()),
            matched_transaction_id: None,
            source_transaction_id: "t1".into(),
            later: later.iter().map(|date| day(date)).collect(),
            per_year: 12,
        }
    }

    fn inputs<'a>(forecast: &'a [ExpectedEvent], credit: &'a Credit) -> Inputs<'a> {
        Inputs {
            today: day("2026-03-10"),
            accounts: &[],
            forecast,
            credit,
            benefits: Vec::new(),
            hidden_benefit_ids: &[],
            dividends: &[],
            long_term_soon: &[],
            wash_windows: &[],
            trips: &[],
        }
    }

    #[test]
    fn forecast_occurrences_within_the_horizon_keep_stable_ids() {
        let credit = Credit::default();
        let forecast = [
            event(
                EventKind::Payment,
                "2026-04-01",
                &["2026-05-01", "2026-06-01", "2026-07-01"],
                EventState::Expected,
            ),
            event(EventKind::Income, "2026-02-20", &[], EventState::Missed),
            event(EventKind::Credit, "2026-03-20", &[], EventState::Expected),
        ];
        let items = agenda(inputs(&forecast, &credit));
        let ids = items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        // July is past the 90-day horizon; a missed deposit and a card credit aren't dated items.
        assert_eq!(
            ids,
            [
                "bill:series:2026-04-01",
                "bill:series:2026-05-01",
                "bill:series:2026-06-01"
            ]
        );
        assert!(items
            .iter()
            .all(|item| item.amount == Some(-1_200.0) && item.estimated));
    }

    #[test]
    fn statement_closes_carry_whether_the_date_was_estimated() {
        let credit = Credit {
            cards: vec![crate::insights::credit::CardCredit {
                account_id: "card".into(),
                balance: 100.0,
                limit: None,
                available: None,
                utilization: None,
                statement_utilization: None,
                closes_on: Some(day("2026-03-25")),
                closes_estimated: true,
                interest_charged: None,
            }],
            ..Credit::default()
        };
        let items = agenda(inputs(&[], &credit));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "close:card:2026-03-25");
        assert_eq!(items[0].kind, AgendaKind::StatementCloses);
        assert!(items[0].estimated);
        assert_eq!(items[0].amount, None);
    }
}

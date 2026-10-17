//! macOS Calendar, with full access through EventKit. Brief keeps one calendar of its own,
//! "Brief", matched to the agenda, and reads the user's other calendars for the Calendar page
//! and trips. It writes only to its own calendar, and only events whose link it made. What it
//! reads is never saved or logged, and never reaches insights.
use std::sync::Mutex;

use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use ts_rs::TS;

use crate::insights::agenda::{AgendaItem, AgendaKind};
use crate::insights::View;
use crate::providers::EarningsEvent;
use crate::AppState;

/// Whether Brief may use Calendar: `notAsked`, `restricted`, `denied`, `allowed` (full
/// access), `writeOnly` (not enough to keep a calendar), or `unavailable` off macOS.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "CalendarAccess")]
pub enum Access {
    NotAsked,
    Restricted,
    Denied,
    Allowed,
    WriteOnly,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Unavailable,
}

/// A calendar account a calendar can be made in.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "CalendarSource")]
pub struct Source {
    pub id: String,
    pub title: String,
    /// `icloud`, `local`, `caldav`, `exchange`, or `other`.
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarInfo {
    pub id: String,
    pub title: String,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Attendee {
    pub name: String,
    /// The Contacts card this attendee is, when Contacts can be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub contact_id: Option<String>,
}

/// An event as EventKit reports it. All-day events carry dates; others, local instants.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "CalendarEvent")]
pub struct Event {
    pub id: String,
    pub calendar: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub color: Option<String>,
    #[serde(default, skip_serializing)]
    #[ts(skip)]
    pub url: Option<String>,
    #[serde(default, skip_serializing)]
    #[ts(skip)]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attendees: Vec<Attendee>,
}

impl Event {
    fn day(value: &str) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(value.get(..10)?, "%Y-%m-%d").ok()
    }

    /// The reference of the item a Brief event was made for: from its link, or from the last
    /// line of its notes, which every calendar service keeps (Google may drop the link).
    fn reference(&self) -> Option<String> {
        let notes = self.notes.as_deref().map(plain_text).unwrap_or_default();
        if let Some(found) = notes.rsplit_once(REFERENCE_LABEL) {
            return found.1.split_whitespace().next().map(str::to_owned);
        }
        // Events written before references carried the whole item in a link.
        let legacy = notes
            .find("brief://calendar?")
            .and_then(|start| notes[start..].split_whitespace().next());
        let url = tauri::Url::parse(self.url.as_deref().or(legacy)?).ok()?;
        if url.scheme() != "brief" || url.host_str() != Some("calendar") {
            return None;
        }
        let pairs = url.query_pairs().collect::<Vec<_>>();
        let value = |name: &str| pairs.iter().find(|(key, _)| key == name).map(|(_, v)| v);
        value("ref")
            .map(|found| found.to_string())
            .or_else(|| value("item").map(|item| reference(item)))
    }
}

/// Notes as text: services that keep notes as HTML break lines with tags and escape symbols.
fn plain_text(notes: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for character in notes.replace("<br>", "\n").replace("<br/>", "\n").chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

const REFERENCE_LABEL: &str = "Brief ref ";

/// A short, stable reference for an agenda item: its kind, then a hash of its full id, which
/// can carry an account id and a merchant.
fn reference(item: &str) -> String {
    let kind = item.split(':').next().unwrap_or("item");
    // FNV-1a: stable across builds and platforms, unlike the standard library's hasher.
    let hash = item.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{kind}-{hash:016x}")
}

/// An event the Brief calendar should hold.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Desired {
    /// The agenda item's reference, which matches it to its event.
    #[serde(skip)]
    pub reference: String,
    /// Opens the day in Brief.
    pub url: String,
    pub title: String,
    pub notes: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
}

impl Desired {
    /// The event for `item`, its notes the `details` and then its reference.
    fn new(
        item: &str,
        title: String,
        details: Vec<String>,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Self {
        let reference = reference(item);
        let url = tauri::Url::parse_with_params(
            "brief://calendar",
            [
                ("day", start.to_string().as_str()),
                ("ref", reference.as_str()),
            ],
        )
        .map(String::from)
        .unwrap_or_default();
        Self {
            notes: details
                .into_iter()
                .chain([format!("{REFERENCE_LABEL}{reference}")])
                .collect::<Vec<_>>()
                .join("\n"),
            reference,
            url,
            title,
            start,
            end,
        }
    }
}

#[derive(Debug, Default, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    pub add: Vec<Desired>,
    pub update: Vec<Update>,
    pub remove: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub id: String,
    #[serde(flatten)]
    pub event: Desired,
}

/// How far back the Brief calendar is kept matched: a trip found after it ended, or a date
/// that moved, is still corrected. Older events are left as they were written.
const KEPT_DAYS: i64 = 30;
/// The agenda's kinds the Brief calendar holds, besides earnings. Subscriptions and
/// dividends take care of themselves.
const WRITTEN: [AgendaKind; 8] = [
    AgendaKind::PaymentDue,
    AgendaKind::StatementCloses,
    AgendaKind::CreditExpiring,
    AgendaKind::Bill,
    AgendaKind::Income,
    AgendaKind::LongTerm,
    AgendaKind::WashSaleEnds,
    AgendaKind::Trip,
];
/// Dollars to the cent with thousands separators, unsigned: `$1,234.56`.
fn money(value: f64) -> String {
    let cents = (value.abs() * 100.0).round() as i64;
    let dollars = (cents / 100).to_string();
    let mut grouped = String::new();
    for (index, digit) in dollars.chars().enumerate() {
        if index > 0 && (dollars.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("${grouped}.{:02}", cents % 100)
}

fn long_date(date: NaiveDate) -> String {
    date.format("%b %-d, %Y").to_string()
}

/// A bank descriptor in capitals reads as words, as the renderer's `displayCase` shows it:
/// `ACH RENT PAYMENT` becomes `ACH Rent Payment`. Short words and codes keep their capitals.
fn display_case(value: &str) -> String {
    let letters = value
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect::<Vec<_>>();
    let capitals = letters.iter().filter(|c| c.is_uppercase()).count();
    if letters.len() < 3 || capitals * 5 < letters.len() * 4 {
        return value.to_owned();
    }
    value
        .split(' ')
        .map(|word| {
            let vowel = word.chars().skip(1).any(|c| "AEIOUY".contains(c));
            if word.chars().count() <= 3 || !vowel || !word.chars().all(char::is_alphabetic) {
                return word.to_owned();
            }
            let mut chars = word.chars();
            chars.next().into_iter().collect::<String>() + &chars.as_str().to_lowercase()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What the event says at a glance: the item, and its amount where it has one.
fn title(view: &View, item: &AgendaItem) -> String {
    let account = || {
        item.account_id
            .as_deref()
            .and_then(|id| view.account_name(id))
            .unwrap_or("Card")
    };
    let subject = display_case(item.title.as_deref().unwrap_or_default());
    let ticker = item.ticker.as_deref().unwrap_or_default();
    let label = match item.kind {
        AgendaKind::PaymentDue => format!("{} payment due", account()),
        AgendaKind::StatementCloses => format!("{} statement closes", account()),
        AgendaKind::CreditExpiring => format!("{subject} credit ends"),
        AgendaKind::Bill => format!("{subject} due"),
        AgendaKind::Subscription => format!("{subject} renews"),
        AgendaKind::Income => format!("{subject} expected"),
        AgendaKind::Dividend => format!("{ticker} dividend"),
        AgendaKind::LongTerm => format!("{ticker} shares turn long-term"),
        AgendaKind::WashSaleEnds => format!("{ticker} wash-sale window ends"),
        AgendaKind::Trip => return format!("Trip: {subject}"),
    };
    match item.amount {
        Some(amount) if item.kind == AgendaKind::CreditExpiring => {
            format!("{label} · {} left", money(amount))
        }
        Some(amount) => format!("{label} · {}", money(amount)),
        None => label,
    }
}

/// Everything the item knows, a fact a line, for the event's notes.
fn details(view: &View, item: &AgendaItem) -> Vec<String> {
    let Some(insights) = &view.insights else {
        return Vec::new();
    };
    let account = item
        .account_id
        .as_deref()
        .and_then(|id| view.account_name(id))
        .map(str::to_owned);
    let mut lines = Vec::new();
    let mut push = |label: &str, value: Option<String>| {
        if let Some(value) = value {
            lines.push(format!("{label}: {value}"));
        }
    };
    match item.kind {
        AgendaKind::PaymentDue => {
            push("Statement balance", item.amount.map(money));
            push("Minimum payment", item.minimum_payment.map(money));
            push("Card", account);
        }
        AgendaKind::StatementCloses => {
            let card = insights
                .credit
                .cards
                .iter()
                .find(|card| Some(&card.account_id) == item.account_id.as_ref());
            push("Balance now", card.map(|card| money(card.balance)));
            push(
                "Limit used",
                card.and_then(|card| card.utilization)
                    .map(|share| format!("{share:.1}%")),
            );
            push("Card", account);
            if item.estimated {
                lines.push("Closing date estimated from past statements.".into());
            }
        }
        AgendaKind::CreditExpiring => {
            push("Left to use", item.amount.map(money));
            push("Card", account);
        }
        AgendaKind::Bill | AgendaKind::Subscription | AgendaKind::Income => {
            let amount = match (item.amount_low, item.amount_high) {
                (Some(low), Some(high)) => Some(format!(
                    "{} (between {} and {})",
                    item.amount.map(money).unwrap_or_default(),
                    money(low.abs().min(high.abs())),
                    money(low.abs().max(high.abs()))
                )),
                _ => item.amount.map(money),
            };
            push("Amount", amount);
            push("Account", account);
            if let Some(frequency) = item.frequency {
                lines.push(format!("Repeats {}", frequency.to_lowercase()));
            }
            lines.push("Expected from past activity.".into());
        }
        AgendaKind::Dividend => {
            push("Amount at shares held", item.amount.map(money));
            push("Account", account);
        }
        AgendaKind::LongTerm => {
            let lot = insights.long_term_soon.iter().find(|lot| {
                Some(&lot.account_id) == item.account_id.as_ref()
                    && Some(&lot.ticker) == item.ticker.as_ref()
                    && lot.date == item.date
            });
            push("Shares", lot.map(|lot| format!("{}", lot.units)));
            push("Gain now", lot.and_then(|lot| lot.gain).map(money));
            push("Account", account);
            lines.push("Sold from today, these shares are taxed at the long-term rate.".into());
        }
        AgendaKind::WashSaleEnds => {
            let window = insights
                .taxes
                .wash_windows
                .iter()
                .find(|window| item.id == format!("wash:{}:{}", window.sale_id, window.until));
            push("Loss", window.map(|window| money(window.loss)));
            push("Sold", window.map(|window| long_date(window.sold_on)));
            push("Account", account);
            lines.push(format!(
                "Buying {} again through this day disallows the loss.",
                item.ticker.as_deref().unwrap_or("it")
            ));
        }
        AgendaKind::Trip => {
            let trip = insights
                .trips
                .trips
                .iter()
                .find(|trip| item.id == format!("trip:{}:{}", trip.id, trip.start));
            push("Spent", item.amount.map(money));
            push(
                "Purchases",
                trip.map(|trip| trip.transaction_ids.len().to_string()),
            );
            push(
                "Places",
                trip.map(|trip| {
                    trip.stops
                        .iter()
                        .filter_map(|stop| stop.city.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .filter(|places| !places.is_empty()),
            );
        }
    }
    lines
}

/// The Brief calendar's events from the agenda and the earnings of the holdings that matter,
/// from `today - KEPT_DAYS` on.
pub fn desired(view: &View, earnings: &[EarningsEvent], today: NaiveDate) -> Vec<Desired> {
    let Some(insights) = &view.insights else {
        return Vec::new();
    };
    let floor = today - Duration::days(KEPT_DAYS);
    let mut events = insights
        .agenda
        .iter()
        .filter(|item| WRITTEN.contains(&item.kind) && item.end.unwrap_or(item.date) >= floor)
        .map(|item| {
            Desired::new(
                &item.id,
                title(view, item),
                details(view, item),
                item.date,
                item.end.unwrap_or(item.date),
            )
        })
        .collect::<Vec<_>>();
    let watch = &insights.investment_attention.earnings_watch;
    let last = today + Duration::days(crate::insights::agenda::HORIZON_DAYS);
    for event in earnings
        .iter()
        .filter(|event| watch.contains(&event.symbol))
    {
        let Ok(date) = NaiveDate::parse_from_str(&event.report_date, "%Y-%m-%d") else {
            continue;
        };
        if (today..=last).contains(&date) {
            let estimate = event
                .estimate
                .map(|estimate| format!("Estimated EPS: ${estimate:.2}"));
            let quarter = event
                .fiscal_date_ending
                .as_deref()
                .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
                .map(|date| format!("Quarter ending: {}", long_date(date)));
            events.push(Desired::new(
                &format!("earnings:{}:{date}", event.symbol),
                format!("{} earnings", event.symbol),
                [Some(event.name.clone()), quarter, estimate]
                    .into_iter()
                    .flatten()
                    .filter(|line| !line.is_empty())
                    .collect(),
                date,
                date,
            ));
        }
    }
    events
}

/// What to change so the Brief calendar holds `desired`. Events are matched by the item their
/// link names; an event Brief didn't make, or one that ended before `floor`, is never touched.
/// Earnings events are kept when `earnings_known` is false, since their source was unreachable.
pub fn plan_sync(
    desired: Vec<Desired>,
    existing: &[Event],
    floor: NaiveDate,
    earnings_known: bool,
) -> SyncPlan {
    let mut plan = SyncPlan::default();
    let mut seen = std::collections::BTreeSet::new();
    for event in existing {
        let Some(item) = event.reference() else {
            continue;
        };
        let ended = Event::day(&event.end).is_some_and(|end| end < floor);
        let wanted = desired.iter().find(|wanted| wanted.reference == item);
        match wanted {
            // A second event for one item is a duplicate.
            Some(_) if !seen.insert(item.clone()) => plan.remove.push(event.id.clone()),
            Some(wanted) => {
                // Services may reformat notes, so only what they say is compared.
                let same = event.title == wanted.title
                    && plain_text(event.notes.as_deref().unwrap_or_default())
                        == plain_text(&wanted.notes)
                    && event.all_day
                    && Event::day(&event.start) == Some(wanted.start)
                    && Event::day(&event.end) == Some(wanted.end);
                if !same {
                    plan.update.push(Update {
                        id: event.id.clone(),
                        event: wanted.clone(),
                    });
                }
            }
            None if ended || (!earnings_known && item.starts_with("earnings-")) => {}
            None => plan.remove.push(event.id.clone()),
        }
    }
    plan.add = desired
        .into_iter()
        .filter(|wanted| !seen.contains(&wanted.reference))
        .collect();
    plan
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{c_char, c_void, CStr, CString};

    use tokio::sync::oneshot;

    use super::Access;

    unsafe extern "C" {
        fn brief_calendar_status() -> i32;
        fn brief_calendar_request(
            completion: *mut c_void,
            callback: extern "C" fn(*mut c_void, i32, *const c_char),
        );
        fn brief_calendar_sources() -> *mut c_char;
        fn brief_calendar_list() -> *mut c_char;
        fn brief_calendar_ensure(request: *const c_char) -> *mut c_char;
        fn brief_calendar_events(request: *const c_char) -> *mut c_char;
        fn brief_calendar_apply(request: *const c_char) -> *mut c_char;
        fn brief_calendar_free(value: *mut c_char);
    }

    pub fn access() -> Access {
        match unsafe { brief_calendar_status() } {
            0 => Access::NotAsked,
            1 => Access::Restricted,
            3 => Access::Allowed,
            4 => Access::WriteOnly,
            _ => Access::Denied,
        }
    }

    extern "C" fn granted(context: *mut c_void, granted: i32, _error: *const c_char) {
        let sender = unsafe { Box::from_raw(context.cast::<oneshot::Sender<bool>>()) };
        let _ = sender.send(granted == 1);
    }

    pub async fn request() -> Result<Access, String> {
        let (sender, receiver) = oneshot::channel::<bool>();
        let context = Box::into_raw(Box::new(sender)).cast::<c_void>();
        unsafe { brief_calendar_request(context, granted) };
        receiver
            .await
            .map_err(|_| "Calendar didn't answer".to_owned())?;
        Ok(access())
    }

    fn owned(output: *mut c_char) -> Option<String> {
        if output.is_null() {
            return None;
        }
        let text = unsafe { CStr::from_ptr(output) }
            .to_string_lossy()
            .into_owned();
        unsafe { brief_calendar_free(output) };
        Some(text)
    }

    /// Calls a bridge function with JSON and returns its owned JSON.
    pub fn call(
        input: &str,
        function: unsafe extern "C" fn(*const c_char) -> *mut c_char,
    ) -> Option<String> {
        let input = CString::new(input).ok()?;
        owned(unsafe { function(input.as_ptr()) })
    }

    pub fn sources() -> Option<String> {
        owned(unsafe { brief_calendar_sources() })
    }

    pub fn list() -> Option<String> {
        owned(unsafe { brief_calendar_list() })
    }

    pub fn ensure(request: &str) -> Option<String> {
        call(request, brief_calendar_ensure)
    }

    pub fn events(request: &str) -> Option<String> {
        call(request, brief_calendar_events)
    }

    pub fn apply(request: &str) -> Option<String> {
        call(request, brief_calendar_apply)
    }
}

fn access() -> Access {
    #[cfg(target_os = "macos")]
    return macos::access();
    #[cfg(not(target_os = "macos"))]
    Access::Unavailable
}

/// Runs a bridge call off the async runtime and parses its JSON answer.
async fn bridge<T: serde::de::DeserializeOwned + Send + 'static>(
    #[allow(unused_variables)] call: impl FnOnce() -> Option<String> + Send + 'static,
) -> Result<T, String> {
    #[cfg(target_os = "macos")]
    {
        let text = tokio::task::spawn_blocking(call)
            .await
            .map_err(|_| "Calendar stopped")?
            .ok_or("Couldn't read Calendar")?;
        if let Ok(failure) = serde_json::from_str::<Failure>(&text) {
            return Err(failure.error);
        }
        serde_json::from_str(&text).map_err(|_| "Couldn't read Calendar".into())
    }
    #[cfg(not(target_os = "macos"))]
    Err("Calendar is available only on macOS".into())
}

#[derive(Deserialize)]
struct Failure {
    error: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Ensured {
    Found(CalendarInfo),
    Missing {
        #[allow(dead_code)]
        missing: bool,
    },
}

#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
async fn ensure(
    id: Option<String>,
    source_id: Option<String>,
) -> Result<Option<CalendarInfo>, String> {
    let request = serde_json::json!({ "id": id, "sourceId": source_id }).to_string();
    #[cfg(target_os = "macos")]
    let call = move || macos::ensure(&request);
    #[cfg(not(target_os = "macos"))]
    let call = move || None;
    Ok(match bridge::<Ensured>(call).await? {
        Ensured::Found(info) => Some(info),
        Ensured::Missing { .. } => None,
    })
}

#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
/// Events in `calendar_ids` on these days; `notes` only for matching Brief's own.
async fn read_events(
    start: NaiveDate,
    end: NaiveDate,
    calendar_ids: Vec<String>,
    notes: bool,
) -> Result<Vec<Event>, String> {
    let request = serde_json::json!({
        "start": start, "end": end, "calendarIds": calendar_ids, "notes": notes,
    })
    .to_string();
    #[cfg(target_os = "macos")]
    let call = move || macos::events(&request);
    #[cfg(not(target_os = "macos"))]
    let call = move || None;
    bridge(call).await
}

/// The last sync's outcome, for Settings.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

static SYNCING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static OUTCOME: Mutex<Option<(NaiveDate, SyncOutcome)>> = Mutex::new(None);

fn record(outcome: Result<(), String>) {
    let entry = (
        Local::now().date_naive(),
        SyncOutcome {
            at: chrono::Utc::now().to_rfc3339(),
            error: outcome.err(),
        },
    );
    *OUTCOME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(entry);
}

/// Matches the Brief calendar to the saved data, when sync is on and Calendar allows it.
async fn sync(app: &AppHandle) -> Result<(), String> {
    let _syncing = SYNCING.lock().await;
    let state = app.state::<AppState>();
    let (enabled, calendar_id, view) = {
        let storage = state.storage()?;
        let settings = &storage.data.settings;
        (
            settings.calendar_sync,
            settings.calendar_id.clone(),
            storage.read_view()?,
        )
    };
    let Some(calendar_id) = calendar_id.filter(|_| enabled && access() == Access::Allowed) else {
        return Ok(());
    };
    let Some(calendar) = ensure(Some(calendar_id), None).await? else {
        return Err("The Brief calendar was deleted in Calendar".into());
    };
    let watch = view
        .insights
        .as_ref()
        .map(|insights| insights.investment_attention.earnings_watch.clone())
        .unwrap_or_default();
    let earnings = if watch.is_empty() {
        Ok(Vec::new())
    } else {
        state.providers.earnings_calendar(watch).await
    };
    let today = Local::now().date_naive();
    let floor = today - Duration::days(KEPT_DAYS);
    let wanted = desired(&view, earnings.as_deref().unwrap_or_default(), today);
    // Trips can begin long before the floor, so look back far enough to find their events.
    let first = wanted
        .iter()
        .map(|event| event.start)
        .min()
        .unwrap_or(floor)
        .min(floor);
    let last = today + Duration::days(crate::insights::agenda::HORIZON_DAYS + 1);
    let existing = read_events(first, last, vec![calendar.id.clone()], true).await?;
    apply(
        calendar.id,
        &plan_sync(wanted, &existing, floor, earnings.is_ok()),
    )
    .await
}

/// Saves a plan to the Brief calendar in one commit.
#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
async fn apply(calendar_id: String, plan: &SyncPlan) -> Result<(), String> {
    if *plan == SyncPlan::default() {
        return Ok(());
    }
    let mut request = serde_json::to_value(plan).map_err(|error| error.to_string())?;
    request["calendarId"] = serde_json::Value::String(calendar_id);
    #[cfg(target_os = "macos")]
    {
        #[derive(Deserialize)]
        struct Applied {
            failed: usize,
        }
        let request = request.to_string();
        let applied: Applied = bridge(move || macos::apply(&request)).await?;
        if applied.failed > 0 {
            return Err(format!(
                "{} Brief calendar {} couldn't be saved",
                applied.failed,
                if applied.failed == 1 {
                    "event"
                } else {
                    "events"
                }
            ));
        }
    }
    Ok(())
}

/// Syncs off the caller's path; a failure is kept for Settings and never affects the caller.
pub fn sync_in_background(app: AppHandle) {
    let enabled = app
        .state::<AppState>()
        .storage()
        .is_ok_and(|storage| storage.data.settings.calendar_sync);
    if !enabled {
        return;
    }
    tauri::async_runtime::spawn(async move {
        record(sync(&app).await);
    });
}

/// Syncs once a day while Brief runs, so dates past the horizon join as the days pass. The
/// first check waits for startup, whose reveal must never wait on Calendar.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let synced_today = OUTCOME
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_ref()
                .is_some_and(|(day, _)| *day == Local::now().date_naive());
            if !synced_today {
                sync_in_background(app.clone());
            }
            tokio::time::sleep(std::time::Duration::from_secs(29 * 60)).await;
        }
    });
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "CalendarStatus")]
pub(crate) struct Status {
    access: Access,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    calendar: Option<CalendarInfo>,
    /// Sync is on but its calendar is gone.
    missing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    last_sync: Option<SyncOutcome>,
}

#[tauri::command]
pub(crate) async fn calendar_status(state: tauri::State<'_, AppState>) -> Result<Status, String> {
    let access = access();
    let (enabled, id) = {
        let storage = state.storage()?;
        let settings = &storage.data.settings;
        (settings.calendar_sync, settings.calendar_id.clone())
    };
    let calendar = match id {
        Some(id) if access == Access::Allowed => ensure(Some(id), None).await?,
        _ => None,
    };
    Ok(Status {
        access,
        missing: enabled && access == Access::Allowed && calendar.is_none(),
        calendar,
        last_sync: OUTCOME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|(_, outcome)| outcome.clone()),
    })
}

/// Asks once; macOS answers from the saved choice afterward.
#[tauri::command]
pub(crate) async fn request_calendar_access() -> Result<Access, String> {
    #[cfg(target_os = "macos")]
    if access() == Access::NotAsked || access() == Access::WriteOnly {
        return macos::request().await;
    }
    Ok(access())
}

#[tauri::command]
pub(crate) async fn calendar_sources() -> Result<Vec<Source>, String> {
    if access() != Access::Allowed {
        return Ok(Vec::new());
    }
    #[cfg(target_os = "macos")]
    return bridge(macos::sources).await;
    #[cfg(not(target_os = "macos"))]
    Ok(Vec::new())
}

/// The Brief calendar: the saved one when it still exists, else a calendar the user already
/// named "Brief" (one made in Google Calendar, where apps can't make calendars), else a new one
/// in `source_id`.
#[tauri::command]
pub(crate) async fn create_brief_calendar(
    source_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<CalendarInfo, String> {
    if access() != Access::Allowed {
        return Err("Brief doesn't have full access to Calendar".into());
    }
    let saved = state.storage()?.data.settings.calendar_id.clone();
    ensure(saved, Some(source_id))
        .await?
        .ok_or_else(|| "Couldn't make the Brief calendar".into())
}

/// Removes every event Brief wrote to the Brief calendar, from eight years back to a year
/// ahead. The calendar itself stays: it may be one the user made, and holds their own events.
#[tauri::command]
pub(crate) async fn remove_brief_events(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let Some(id) = state.storage()?.data.settings.calendar_id.clone() else {
        return Ok(());
    };
    if access() != Access::Allowed {
        return Err("Brief doesn't have full access to Calendar".into());
    }
    let _syncing = SYNCING.lock().await;
    let today = Local::now().date_naive();
    // EventKit reads at most four years at a time.
    for years in [(-8, -4), (-4, 0), (0, 1)] {
        let start = today + Duration::days(365 * years.0);
        let end = today + Duration::days(365 * years.1);
        let remove = read_events(start, end, vec![id.clone()], true)
            .await?
            .into_iter()
            .filter(|event| event.reference().is_some())
            .map(|event| event.id)
            .collect();
        apply(
            id.clone(),
            &SyncPlan {
                remove,
                ..SyncPlan::default()
            },
        )
        .await?;
    }
    Ok(())
}

/// The user's calendars besides the Brief calendar, for choosing which to show.
#[tauri::command]
pub(crate) async fn calendar_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ListedCalendar>, String> {
    if access() != Access::Allowed {
        return Ok(Vec::new());
    }
    let brief = state.storage()?.data.settings.calendar_id.clone();
    #[cfg(target_os = "macos")]
    let listed: Vec<ListedCalendar> = bridge(macos::list).await?;
    #[cfg(not(target_os = "macos"))]
    let listed: Vec<ListedCalendar> = Vec::new();
    Ok(listed
        .into_iter()
        .filter(|calendar| Some(&calendar.id) != brief.as_ref())
        .collect())
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListedCalendar {
    pub id: String,
    pub title: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub color: Option<String>,
}

/// The user's events in the calendars they chose to show, on these days. At most a year, and
/// never the Brief calendar, whose items the page already shows from the agenda.
#[tauri::command]
pub(crate) async fn calendar_events(
    start: NaiveDate,
    end: NaiveDate,
    calendar_ids: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Event>, String> {
    if end < start || end - start > Duration::days(370) || calendar_ids.len() > 100 {
        return Err("Choose at most a year of events".into());
    }
    if access() != Access::Allowed {
        return Ok(Vec::new());
    }
    let brief = state.storage()?.data.settings.calendar_id.clone();
    let shown = calendar_ids
        .into_iter()
        .filter(|id| Some(id) != brief.as_ref())
        .collect::<Vec<_>>();
    if shown.is_empty() {
        return Ok(Vec::new());
    }
    read_events(start, end, shown, false).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn wanted(item: &str, date: &str) -> Desired {
        Desired::new(
            item,
            format!("{item} title"),
            vec!["Amount: $1.00".into()],
            day(date),
            day(date),
        )
    }

    /// The event Brief wrote for `item`, as Calendar reads it back.
    fn existing(id: &str, item: Option<&str>, date: &str, title: &str) -> Event {
        let made = item.map(|item| wanted(item, date));
        Event {
            id: id.into(),
            calendar: "Brief".into(),
            title: title.into(),
            start: date.into(),
            end: date.into(),
            all_day: true,
            color: None,
            url: made.as_ref().map(|made| made.url.clone()),
            notes: made.map(|made| made.notes),
            location: None,
            attendees: Vec::new(),
        }
    }

    #[test]
    fn events_carry_a_short_reference_instead_of_the_item() {
        let item = r#"income:{"account":"plaid:abc","merchant":"interest"}:2026-10-31"#;
        let made = wanted(item, "2026-10-31");
        assert!(made.reference.starts_with("income-"));
        assert_eq!(made.reference.len(), "income-".len() + 16);
        assert_eq!(
            made.url,
            format!("brief://calendar?day=2026-10-31&ref={}", made.reference)
        );
        assert_eq!(
            made.notes,
            format!("Amount: $1.00\nBrief ref {}", made.reference)
        );
        assert!(!made.notes.contains("plaid"));
    }

    #[test]
    fn events_are_known_by_their_link_their_notes_or_an_older_link() {
        let made = wanted("bill:rent:2026-04-01", "2026-04-01");
        let mut event = existing("e", Some("bill:rent:2026-04-01"), "2026-04-01", "x");
        assert_eq!(event.reference(), Some(made.reference.clone()));
        // Google may drop the link and keep the notes as HTML.
        event.url = None;
        event.notes = Some(
            made.notes
                .replace('\n', "<br>")
                .replace("Brief", "<b>Brief</b>"),
        );
        assert_eq!(event.reference(), Some(made.reference.clone()));
        // Reformatted notes that still say the same thing need no rewrite.
        event.title = made.title.clone();
        let plan = plan_sync(vec![made.clone()], &[event], day("2026-03-01"), true);
        assert_eq!(plan, SyncPlan::default());
        // Events written before references named the whole item in their link.
        let older = Event {
            notes: Some("brief://calendar?day=2026-04-01&item=bill%3Arent%3A2026-04-01".into()),
            ..existing("old", None, "2026-04-01", "x")
        };
        assert_eq!(older.reference(), Some(made.reference));
    }

    #[test]
    fn amounts_read_as_dollars_and_cents() {
        assert_eq!(money(-1_234_567.891), "$1,234,567.89");
        assert_eq!(money(12.3), "$12.30");
        assert_eq!(money(0.0), "$0.00");
    }

    #[test]
    fn sync_adds_updates_and_removes_only_brief_events_in_range() {
        let floor = day("2026-03-01");
        let plan = plan_sync(
            vec![
                wanted("due:card:2026-04-02", "2026-04-02"),
                wanted("bill:rent:2026-04-01", "2026-04-01"),
            ],
            &[
                // Unchanged.
                existing(
                    "same",
                    Some("due:card:2026-04-02"),
                    "2026-04-02",
                    "due:card:2026-04-02 title",
                ),
                // Retitled since: updated in place.
                existing(
                    "old-title",
                    Some("bill:rent:2026-04-01"),
                    "2026-04-01",
                    "Rent",
                ),
                // A duplicate of an item already matched.
                existing(
                    "twin",
                    Some("due:card:2026-04-02"),
                    "2026-04-02",
                    "due:card:2026-04-02 title",
                ),
                // No longer wanted.
                existing("gone", Some("bill:gym:2026-04-05"), "2026-04-05", "Gym due"),
                // Ended before the floor: history, left alone.
                existing("history", Some("trip:t1:2026-01-02"), "2026-01-04", "Trip"),
                // Made by the user: never touched.
                existing("mine", None, "2026-04-02", "Dentist"),
                // Earnings couldn't be read this time: kept.
                existing(
                    "earnings",
                    Some("earnings:VTI:2026-04-20"),
                    "2026-04-20",
                    "VTI",
                ),
            ],
            floor,
            false,
        );
        assert!(plan.add.is_empty());
        assert_eq!(plan.update.len(), 1);
        assert_eq!(plan.update[0].id, "old-title");
        assert_eq!(plan.remove, ["twin", "gone"]);
    }

    #[test]
    fn descriptors_in_capitals_read_as_words() {
        assert_eq!(display_case("ACH RENT PAYMENT"), "ACH Rent Payment");
        assert_eq!(display_case("Equinox"), "Equinox");
        assert_eq!(display_case("SQ *COFFEE 0412"), "SQ *COFFEE 0412");
    }

    #[test]
    fn sync_adds_what_is_missing() {
        let plan = plan_sync(
            vec![wanted("close:card:2026-04-10", "2026-04-10")],
            &[],
            day("2026-03-01"),
            true,
        );
        assert_eq!(plan.add.len(), 1);
        assert!(plan.update.is_empty() && plan.remove.is_empty());
    }
}

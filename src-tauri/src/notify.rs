//! macOS notifications for what a background refresh finds, and the menu bar item that
//! reaches a hidden app. Both are opt-in. A notification can appear on a locked screen, so it
//! names what needs action and never an amount or a balance.
use std::collections::HashSet;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};
use tauri_plugin_notification::NotificationExt;

use crate::{
    insights::attention::{AttentionItem, Item},
    AppState,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub id: String,
    pub title: String,
    pub body: String,
}

/// At most this many notifications follow one refresh.
const MAX_NOTICES: usize = 3;

fn in_days(days: i64) -> String {
    match days {
        0 => "today".into(),
        1 => "tomorrow".into(),
        days => format!("in {days} days"),
    }
}

/// What the brief says needs action, worded without amounts.
pub fn notices(needs: &[AttentionItem], account_name: impl Fn(&str) -> String) -> Vec<Notice> {
    needs
        .iter()
        .filter_map(|need| {
            let (title, body) = match &need.item {
                Item::PaymentDue {
                    account_id,
                    days,
                    overdue,
                    ..
                } => {
                    let name = account_name(account_id);
                    let title = if *overdue {
                        format!("{name} payment is overdue")
                    } else {
                        format!("{name} payment due {}", in_days(*days))
                    };
                    (title, "Open Brief for the statement and minimum.")
                }
                Item::CoverageShort { .. } => (
                    "Expected bills exceed cash".to_owned(),
                    "The next 30 days of expected bills are more than known cash.",
                ),
                Item::CashLow { date, .. } => (
                    format!("Cash may run short by {}", date.format("%b %-d")),
                    "Expected bills take cash below zero.",
                ),
                Item::UtilizationHigh {
                    account_id, days, ..
                } => (
                    format!(
                        "{} statement closes {}",
                        account_name(account_id),
                        in_days(*days)
                    ),
                    "Pay it down before it closes to report less.",
                ),
                Item::PossibleDuplicate { merchant, .. } => (
                    format!("Possible duplicate charge at {merchant}"),
                    "Two identical charges posted the same day.",
                ),
                Item::CreditExpiring { name, days, .. } => (
                    format!("{name} credit ends {}", in_days(*days)),
                    "Part of this window's allowance is unused.",
                ),
                Item::MissedBill { title, days, .. } => (
                    format!("{title} didn't post"),
                    if *days == 1 {
                        "It was due yesterday and nothing has posted."
                    } else {
                        "It was due and nothing has posted."
                    },
                ),
                Item::ContributionLimit {
                    plan,
                    tax_year,
                    on_pace: false,
                    ..
                } => (
                    format!(
                        "{tax_year} {} contributions are over the limit",
                        if *plan == "ira" { "IRA" } else { "401(k)" }
                    ),
                    "An excess is taxed until it's taken out.",
                ),
                Item::PayMissing { employer, .. } => (
                    format!("Pay from {employer} hasn't arrived"),
                    "The pay statement shows a deposit Brief hasn't seen.",
                ),
                Item::W2Differs { employer, .. } => (
                    format!("{employer}'s W-2 differs from your pay statements"),
                    "Check it before you file.",
                ),
                // None is tied to a date, so none interrupts.
                Item::WashWindow { .. }
                | Item::BenefitReview { .. }
                | Item::IdleCash { .. }
                | Item::ContributionsToConfirm { .. }
                | Item::ContributionLimit { .. } => return None,
            };
            Some(Notice {
                id: need.id.clone(),
                title,
                body: body.to_owned(),
            })
        })
        .collect()
}

/// The notices not yet delivered in this run of the app, marked delivered.
pub fn unseen(notices: Vec<Notice>, delivered: &mut HashSet<String>) -> Vec<Notice> {
    notices
        .into_iter()
        .filter(|notice| delivered.insert(notice.id.clone()))
        .take(MAX_NOTICES)
        .collect()
}

/// Posts a notification for each new need, when the user turned notifications on.
pub fn deliver(app: &AppHandle) {
    let state = app.state::<AppState>();
    let view = match state.storage() {
        Ok(storage) if storage.data.settings.notifications => storage.read_view().ok(),
        _ => None,
    };
    let Some(view) = view else { return };
    let Some(insights) = &view.insights else {
        return;
    };
    let account_name = |id: &str| view.account_name(id).unwrap_or("Card").to_owned();
    let Ok(mut delivered) = state.notified.lock() else {
        return;
    };
    for notice in unseen(notices(&insights.brief.needs, account_name), &mut delivered) {
        // A refused or unavailable notification never affects the refresh that found it.
        let _ = app
            .notification()
            .builder()
            .title(notice.title)
            .body(notice.body)
            .show();
    }
}

const TRAY_ID: &str = "brief";

/// Shows the menu bar item while background refresh keeps the app running without a window,
/// and removes it otherwise. Safe to call whenever the setting may have changed.
pub fn sync_tray(app: &AppHandle, enabled: bool) {
    if !enabled {
        let _ = app.remove_tray_by_id(TRAY_ID);
        return;
    }
    if app.tray_by_id(TRAY_ID).is_some() {
        return;
    }
    let build = || -> tauri::Result<()> {
        let open = MenuItem::with_id(app, "tray-open", "Open Brief", true, None::<&str>)?;
        let refresh = MenuItem::with_id(app, "tray-refresh", "Refresh Now", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "tray-quit", "Quit Brief", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&open, &refresh, &quit])?;
        let mut tray = TrayIconBuilder::with_id(TRAY_ID)
            .tooltip("Brief")
            .menu(&menu)
            .on_menu_event(|app, event| match event.id().as_ref() {
                "tray-open" => crate::window::bring_forward(app),
                "tray-refresh" => {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<AppState>();
                        if crate::refresh::refresh_finance_snapshot(app.clone(), state, Some(false))
                            .await
                            .is_ok()
                        {
                            let _ = app.emit("finance-snapshot-updated", ());
                        }
                    });
                }
                "tray-quit" => app.exit(0),
                _ => {}
            });
        if let Some(icon) = app.default_window_icon() {
            tray = tray.icon(icon.clone());
        }
        tray.build(app)?;
        Ok(())
    };
    // The menu bar item is a convenience; the Dock icon still reopens the window without it.
    let _ = build();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::attention::Severity;

    #[test]
    fn notices_name_what_needs_action_without_any_amount() {
        let date = |value: &str| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap();
        let need = |id: &str, item: Item| AttentionItem {
            id: id.into(),
            severity: Severity::Attention,
            item,
        };
        let payment = |account_id: &str, days: i64, overdue: bool| Item::PaymentDue {
            account_id: account_id.into(),
            due: date("2026-11-01"),
            days,
            overdue,
            statement_balance: Some(4321.09),
            minimum_payment: Some(35.0),
        };
        let needs = [
            need("payment", payment("plaid:card", 1, false)),
            need("late", payment("plaid:other", -2, true)),
            need(
                "short",
                Item::CoverageShort {
                    cash: 10.5,
                    expected_outflows: 99.0,
                    shortfall: 88.5,
                },
            ),
            need(
                "low",
                Item::CashLow {
                    date: date("2026-11-20"),
                    expected: -280.0,
                    low: -440.0,
                },
            ),
            need(
                "credit",
                Item::CreditExpiring {
                    account_id: "plaid:card".into(),
                    benefit_id: "resy".into(),
                    name: "Resy".into(),
                    remaining_low: 40.0,
                    remaining_high: 100.0,
                    window_end: date("2026-11-06"),
                    days: 5,
                },
            ),
            need(
                "wash",
                Item::WashWindow {
                    ticker: "TEST".into(),
                    account_id: "plaid:brokerage".into(),
                    sold_on: date("2026-10-20"),
                    until: date("2026-11-19"),
                    loss: -120.0,
                },
            ),
        ];
        let account_name = |id: &str| {
            if id == "plaid:card" {
                "Everyday card"
            } else {
                "Card"
            }
            .to_owned()
        };
        let notices = notices(&needs, account_name);
        assert_eq!(
            notices
                .iter()
                .map(|notice| notice.title.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Everyday card payment due tomorrow",
                "Card payment is overdue",
                "Expected bills exceed cash",
                "Cash may run short by Nov 20",
                "Resy credit ends in 5 days",
            ]
        );
        // Nothing shown on a lock screen carries a personal number.
        for notice in &notices {
            let text = format!("{} {}", notice.title, notice.body);
            for amount in ["4321", "35", "10.5", "99", "88.5", "280", "440", "120", "$"] {
                assert!(!text.contains(amount), "{text} contains {amount}");
            }
        }
        assert!(super::notices(&[], account_name).is_empty());
    }

    #[test]
    fn each_need_notifies_once_and_a_refresh_posts_at_most_three() {
        let notice = |id: &str| Notice {
            id: id.into(),
            title: id.into(),
            body: String::new(),
        };
        let mut delivered = HashSet::new();
        let first = unseen(
            vec![notice("a"), notice("b"), notice("c"), notice("d")],
            &mut delivered,
        );
        assert_eq!(first.len(), 3);
        // The one held back is still new next time; the delivered ones are not.
        let second = unseen(
            vec![
                notice("a"),
                notice("b"),
                notice("c"),
                notice("d"),
                notice("e"),
            ],
            &mut delivered,
        );
        assert_eq!(
            second
                .iter()
                .map(|notice| notice.id.as_str())
                .collect::<Vec<_>>(),
            vec!["d", "e"]
        );
    }
}

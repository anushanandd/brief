//! The main window: its macOS chrome, showing it, and the `brief://` links that open views.
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

#[cfg(target_os = "macos")]
pub(crate) fn hide_window_buttons(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    use objc2_app_kit::{NSWindow, NSWindowButton};
    let native = window.ns_window()? as usize;
    window.run_on_main_thread(move || {
        // SAFETY: Tauri owns this NSWindow for the life of the main app window.
        let native = unsafe { &*(native as *const NSWindow) };
        for kind in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
            if let Some(button) = native.standardWindowButton(kind) {
                button.setHidden(true);
            }
        }
    })
}

/// Shows and focuses the main window, as the Dock icon and links do.
pub(crate) fn bring_forward(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub(crate) fn reveal_main_window(app: AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("Window unavailable")?;
    window.show().map_err(|error| error.to_string())
}

/// A `brief://` link that opened the app before the window could listen for it.
static OPENED_VIEW: Mutex<Option<String>> = Mutex::new(None);

/// The in-app location a `brief://` link names: `brief://holdings/VTI?x=y` opens
/// `/holdings/VTI?x=y`. Links only navigate; the renderer checks the path against its routes.
fn opened_view(url: &tauri::Url) -> Option<String> {
    if url.scheme() != "brief" {
        return None;
    }
    let path = format!(
        "/{}{}",
        url.host_str().unwrap_or_default(),
        url.path().trim_end_matches('/')
    );
    let path = if path.len() > 1 {
        path.trim_end_matches('/').to_string()
    } else {
        path
    };
    Some(match url.query() {
        Some(query) => format!("{path}?{query}"),
        None => path,
    })
}

/// Opens the view the last `brief://` link names, keeping it for a window not yet listening.
pub(crate) fn open_links(app: &AppHandle, urls: &[tauri::Url]) {
    let Some(view) = urls.iter().rev().find_map(opened_view) else {
        return;
    };
    *OPENED_VIEW
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(view.clone());
    let _ = app.emit("open-view", view);
    bring_forward(app);
}

#[tauri::command]
pub(crate) fn take_opened_view() -> Option<String> {
    OPENED_VIEW
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brief_links_name_in_app_locations_only() {
        let view = |link: &str| opened_view(&tauri::Url::parse(link).unwrap());
        assert_eq!(
            view("brief://holdings/VTI").as_deref(),
            Some("/holdings/VTI")
        );
        assert_eq!(
            view("brief://accounts?account=checking").as_deref(),
            Some("/accounts?account=checking")
        );
        assert_eq!(view("brief://").as_deref(), Some("/"));
        assert_eq!(view("brief://spending/").as_deref(), Some("/spending"));
        assert_eq!(view("https://example.com/holdings"), None);
    }
}

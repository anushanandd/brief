//! The Mac surfaces of the renderer's commands: the menu bar lists every app and page command
//! the window has registered, and rows copy to the pasteboard from their context menu.

use std::sync::Mutex;

use serde::Deserialize;
use tauri::{
    menu::{Menu, MenuEvent, MenuId, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu},
    AppHandle, Emitter, Runtime,
};

/// Menu ids for renderer commands; activating one asks the renderer to run the command.
pub const ACTION_PREFIX: &str = "action:";
const SECTION_PREFIX: &str = "commands:";

/// Separators Brief added to macOS's own menus, so the next page's menus can remove them.
static ADDED_SEPARATORS: Mutex<Vec<MenuId>> = Mutex::new(Vec::new());

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSection {
    title: String,
    items: Vec<CommandItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandItem {
    id: String,
    title: String,
    enabled: bool,
    accelerator: Option<String>,
}

#[tauri::command]
pub fn set_command_menu<R: Runtime>(
    app: AppHandle<R>,
    sections: Vec<CommandSection>,
) -> Result<(), String> {
    let Some(menu) = app.menu() else {
        return Ok(());
    };
    replace_commands(&app, &menu, sections).map_err(|error| error.to_string())
}

fn replace_commands<R: Runtime>(
    app: &AppHandle<R>,
    menu: &Menu<R>,
    sections: Vec<CommandSection>,
) -> tauri::Result<()> {
    let mut separators = ADDED_SEPARATORS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for item in menu.items()? {
        let MenuItemKind::Submenu(submenu) = &item else {
            continue;
        };
        if submenu.id().as_ref().starts_with(SECTION_PREFIX) {
            menu.remove(&item)?;
            continue;
        }
        for child in submenu.items()? {
            if child.id().as_ref().starts_with(ACTION_PREFIX) || separators.contains(child.id()) {
                submenu.remove(&child)?;
            }
        }
    }
    separators.clear();

    for section in sections {
        let items = section
            .items
            .iter()
            .map(|item| {
                MenuItem::with_id(
                    app,
                    format!("{ACTION_PREFIX}{}", item.id),
                    &item.title,
                    item.enabled,
                    item.accelerator.as_deref(),
                )
            })
            .collect::<tauri::Result<Vec<_>>>()?;
        // A section named like one of macOS's menus (View) joins it below a separator.
        let existing = menu.items()?.into_iter().find_map(|item| match item {
            MenuItemKind::Submenu(submenu)
                if submenu.text().is_ok_and(|title| title == section.title) =>
            {
                Some(submenu)
            }
            _ => None,
        });
        if let Some(submenu) = existing {
            let separator = PredefinedMenuItem::separator(app)?;
            separators.push(separator.id().clone());
            submenu.append(&separator)?;
            for item in &items {
                submenu.append(item)?;
            }
            continue;
        }
        let submenu = Submenu::with_id(
            app,
            format!("{SECTION_PREFIX}{}", section.title),
            &section.title,
            true,
        )?;
        for item in &items {
            submenu.append(item)?;
        }
        // Brief's menus sit before Window and Help, as an app's own menus do.
        let position = menu
            .items()?
            .iter()
            .position(|item| {
                item.as_submenu()
                    .and_then(|submenu| submenu.text().ok())
                    .is_some_and(|title| title == "Window" || title == "Help")
            })
            .unwrap_or(menu.items()?.len());
        menu.insert(&submenu, position)?;
    }
    Ok(())
}

/// Puts plain text on the general pasteboard. Menu activations aren't user gestures, so the
/// renderer can't use the web clipboard for them.
#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::NSString;

        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        // SAFETY: AppKit defines the string pasteboard type for the life of the process.
        let kind = unsafe { NSPasteboardTypeString };
        if pasteboard.setString_forType(&NSString::from_str(&text), kind) {
            Ok(())
        } else {
            Err("The text could not be copied".into())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = text;
        Err("Copying needs macOS".into())
    }
}

/// Tells the renderer which menu item was chosen: Settings, hiding values, or one of its own
/// registered commands.
pub fn handle_shortcut<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let shortcut = event.id().as_ref();
    if shortcut == "open-settings" {
        let _ = app.emit("open-settings", ());
    } else if shortcut == "toggle-sensitive-values" {
        let _ = app.emit("toggle-sensitive-values", ());
    } else if let Some(action) = shortcut.strip_prefix(ACTION_PREFIX) {
        let _ = app.emit("run-action", action.to_string());
    }
}

/// The app's menu bar: the default menus less File, with Settings and value hiding added and
/// the items Brief has no use for removed. Pages add their own commands through
/// `set_command_menu`.
#[cfg(target_os = "macos")]
pub fn shortcut_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;

    let settings = MenuItem::with_id(app, "open-settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    if let Some(app_menu) = menu.items()?.first().and_then(|item| item.as_submenu()) {
        app_menu.insert(&settings, 1)?;
        for child in app_menu.items()? {
            if let Some(predefined) = child.as_predefined_menuitem() {
                let label = predefined.text()?.replace('&', "");
                if label.starts_with("Hide ") && label != "Hide Others" {
                    app_menu.remove(&child)?;
                }
            }
        }
        let hide_values = MenuItem::with_id(
            app,
            "toggle-sensitive-values",
            "Hide or Show Values",
            true,
            Some("CmdOrCtrl+H"),
        )?;
        app_menu.insert(&hide_values, 2)?;
    }

    for item in menu.items()? {
        let Some(submenu) = item.as_submenu() else {
            continue;
        };
        let title = submenu.text()?;

        if title == "File" {
            menu.remove(&item)?;
            continue;
        }

        if title == "Edit" || title == "Window" {
            for child in submenu.items()? {
                let Some(predefined) = child.as_predefined_menuitem() else {
                    continue;
                };
                let label = predefined.text()?.replace('&', "");
                if (title == "Edit" && label == "Select All")
                    || (title == "Window" && label != "Zoom")
                {
                    submenu.remove(&child)?;
                }
            }
        }
    }

    Ok(menu)
}

//! Read-only Mac Contacts, for People: names to add someone from, and their photos. Nothing
//! read here is saved except the name and card identifier the user picks, and nothing is
//! logged. Contacts is never written.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Contact {
    pub identifier: String,
    pub name: String,
    #[serde(default)]
    pub has_photo: bool,
}

/// Whether Brief may read Contacts: `notAsked`, `restricted`, `denied`, `allowed`, or
/// `unavailable` off macOS.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "ContactsAccess")]
pub enum Access {
    NotAsked,
    Restricted,
    Denied,
    Allowed,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Unavailable,
}

const MAX_QUERY: usize = 80;
const MAX_PHOTOS: usize = 100;

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{c_char, c_void, CStr, CString};

    use tokio::sync::oneshot;

    use super::Access;

    unsafe extern "C" {
        fn brief_contacts_status() -> i32;
        fn brief_contacts_request(
            completion: *mut c_void,
            callback: extern "C" fn(*mut c_void, i32, *const c_char),
        );
        fn brief_contacts_search(query: *const c_char) -> *mut c_char;
        fn brief_contacts_photos(identifiers: *const c_char) -> *mut c_char;
        fn brief_contacts_free(value: *mut c_char);
    }

    pub fn access() -> Access {
        match unsafe { brief_contacts_status() } {
            0 => Access::NotAsked,
            1 => Access::Restricted,
            2 => Access::Denied,
            _ => Access::Allowed,
        }
    }

    extern "C" fn granted(context: *mut c_void, granted: i32, _error: *const c_char) {
        let sender = unsafe { Box::from_raw(context.cast::<oneshot::Sender<bool>>()) };
        let _ = sender.send(granted == 1);
    }

    pub async fn request() -> Result<Access, String> {
        let (sender, receiver) = oneshot::channel::<bool>();
        let context = Box::into_raw(Box::new(sender)).cast::<c_void>();
        unsafe { brief_contacts_request(context, granted) };
        receiver
            .await
            .map_err(|_| "Contacts didn't answer".to_owned())?;
        Ok(access())
    }

    /// Calls a bridge function that returns owned JSON, and frees it.
    fn call(
        input: &str,
        function: unsafe extern "C" fn(*const c_char) -> *mut c_char,
    ) -> Option<String> {
        let input = CString::new(input).ok()?;
        let output = unsafe { function(input.as_ptr()) };
        if output.is_null() {
            return None;
        }
        let text = unsafe { CStr::from_ptr(output) }
            .to_string_lossy()
            .into_owned();
        unsafe { brief_contacts_free(output) };
        Some(text)
    }

    pub fn search(query: &str) -> Option<String> {
        call(query, brief_contacts_search)
    }

    pub fn photos(identifiers: &str) -> Option<String> {
        call(identifiers, brief_contacts_photos)
    }
}

#[cfg(target_os = "macos")]
fn access() -> Access {
    macos::access()
}

#[cfg(not(target_os = "macos"))]
fn access() -> Access {
    Access::Unavailable
}

#[tauri::command]
pub(crate) fn contacts_access() -> Access {
    access()
}

/// Asks once; macOS answers from the saved choice afterward.
#[tauri::command]
pub(crate) async fn request_contacts_access() -> Result<Access, String> {
    #[cfg(target_os = "macos")]
    {
        if access() == Access::NotAsked {
            return macos::request().await;
        }
    }
    Ok(access())
}

#[tauri::command]
pub(crate) async fn search_contacts(query: String) -> Result<Vec<Contact>, String> {
    let query = query.trim().chars().take(MAX_QUERY).collect::<String>();
    if query.is_empty() || access() != Access::Allowed {
        return Ok(Vec::new());
    }
    #[cfg(target_os = "macos")]
    {
        let found = tokio::task::spawn_blocking(move || macos::search(&query))
            .await
            .map_err(|_| "Contacts search stopped")?
            .ok_or("Couldn't read Contacts")?;
        serde_json::from_str(&found).map_err(|_| "Couldn't read Contacts".into())
    }
    #[cfg(not(target_os = "macos"))]
    Ok(Vec::new())
}

/// A Contacts card a name in payments could be, and how well it fits.
#[derive(Clone, Debug, Serialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContactFit {
    pub identifier: String,
    pub name: String,
    pub has_photo: bool,
    pub fit: crate::insights::people::Fit,
}

const MAX_MATCH_NAMES: usize = 20;
const MAX_CONTACT_FITS: usize = 3;

/// The cards each name could be, likeliest first, by the same rules that match saved people:
/// Contacts is searched by each word of the name, and every card found is scored against it.
#[tauri::command]
pub(crate) async fn match_contacts(
    names: Vec<String>,
) -> Result<BTreeMap<String, Vec<ContactFit>>, String> {
    if names.is_empty() || access() != Access::Allowed {
        return Ok(BTreeMap::new());
    }
    if names.len() > MAX_MATCH_NAMES || names.iter().any(|name| name.len() > MAX_QUERY) {
        return Err("Too many names to match".into());
    }
    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(move || {
            names
                .into_iter()
                .map(|name| {
                    let fits = contact_fits(&name, |word| {
                        macos::search(word)
                            .and_then(|found| serde_json::from_str::<Vec<Contact>>(&found).ok())
                            .unwrap_or_default()
                    });
                    (name, fits)
                })
                .collect()
        })
        .await
        .map_err(|_| "Contacts search stopped".into())
    }
    #[cfg(not(target_os = "macos"))]
    Ok(BTreeMap::new())
}

/// Scores every card a search for any word of `name` finds. Words under three letters (a
/// middle initial, "Ly") find too many cards to be worth searching.
fn contact_fits(name: &str, search: impl Fn(&str) -> Vec<Contact>) -> Vec<ContactFit> {
    let words = crate::settings::match_text(name);
    let mut found = BTreeMap::<String, Contact>::new();
    for word in words.split(' ').filter(|word| word.chars().count() >= 3) {
        for contact in search(word) {
            found.entry(contact.identifier.clone()).or_insert(contact);
        }
    }
    let mut fits = found
        .into_values()
        .filter_map(|contact| {
            let fit = crate::insights::people::name_fit(name, &contact.name)?;
            Some(ContactFit {
                identifier: contact.identifier,
                name: contact.name,
                has_photo: contact.has_photo,
                fit,
            })
        })
        .collect::<Vec<_>>();
    fits.sort_by(|left, right| {
        right
            .fit
            .cmp(&left.fit)
            .then_with(|| left.name.cmp(&right.name))
    });
    fits.truncate(MAX_CONTACT_FITS);
    fits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_in_payments_finds_cards_by_any_long_word_and_keeps_those_that_fit() {
        let cards = [
            ("1", "Eddie Person"),
            ("2", "Example Other"),
            ("3", "Someone Person"),
            ("4", "Edmund Ly Person"),
        ];
        let searched = std::cell::RefCell::new(Vec::new());
        let fits = contact_fits("EDMUND LY PERSON", |word| {
            searched.borrow_mut().push(word.to_owned());
            cards
                .iter()
                .filter(|(_, name)| crate::settings::match_text(name).contains(word))
                .map(|(identifier, name)| Contact {
                    identifier: (*identifier).into(),
                    name: (*name).into(),
                    has_photo: false,
                })
                .collect()
        });
        // "LY" is too short to search by.
        assert_eq!(*searched.borrow(), ["EDMUND", "PERSON"]);
        assert_eq!(
            fits.iter()
                .map(|fit| (fit.name.as_str(), fit.fit))
                .collect::<Vec<_>>(),
            [
                ("Edmund Ly Person", crate::insights::people::Fit::Likely),
                ("Eddie Person", crate::insights::people::Fit::Possible),
            ]
        );
    }
}

/// Thumbnails by card identifier, base64; a card without one is left out.
#[tauri::command]
pub(crate) async fn contact_photos(
    identifiers: Vec<String>,
) -> Result<BTreeMap<String, String>, String> {
    if identifiers.is_empty() || access() != Access::Allowed {
        return Ok(BTreeMap::new());
    }
    if identifiers.len() > MAX_PHOTOS || identifiers.iter().any(|id| id.len() > 200) {
        return Err("Too many photos requested".into());
    }
    #[cfg(target_os = "macos")]
    {
        let input = serde_json::to_string(&identifiers).map_err(|error| error.to_string())?;
        let found = tokio::task::spawn_blocking(move || macos::photos(&input))
            .await
            .map_err(|_| "Contacts photos stopped")?
            .ok_or("Couldn't read Contacts")?;
        serde_json::from_str(&found).map_err(|_| "Couldn't read Contacts".into())
    }
    #[cfg(not(target_os = "macos"))]
    Ok(BTreeMap::new())
}

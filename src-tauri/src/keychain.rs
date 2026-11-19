//! Generic passwords in the user's login Keychain, read and written the way the `keyring` crate
//! did before it, so items saved by earlier builds still read back.

/// The item's bytes, or None when the Keychain holds no such item.
#[cfg(target_os = "macos")]
pub fn read(service: &str, account: &str) -> Result<Option<Vec<u8>>, String> {
    use security_framework::os::macos::passwords::find_generic_password;
    match find_generic_password(Some(&[login_keychain()?]), service, account) {
        Ok((password, _)) => Ok(Some(password.to_vec())),
        Err(error) if error.code() == security_framework_sys::base::errSecItemNotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// Creates the item, or replaces its bytes when it exists.
#[cfg(target_os = "macos")]
pub fn write(service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
    login_keychain()?
        .set_generic_password(service, account, secret)
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
fn login_keychain() -> Result<security_framework::os::macos::keychain::SecKeychain, String> {
    use security_framework::os::macos::keychain::{SecKeychain, SecPreferencesDomain};
    SecKeychain::default_for_domain(SecPreferencesDomain::User).map_err(|error| error.to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn read(_service: &str, _account: &str) -> Result<Option<Vec<u8>>, String> {
    Err("Keychain is only available on macOS".into())
}

#[cfg(not(target_os = "macos"))]
pub fn write(_service: &str, _account: &str, _secret: &[u8]) -> Result<(), String> {
    Err("Keychain is only available on macOS".into())
}

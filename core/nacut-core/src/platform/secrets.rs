//! The Usage & Cost API key, in the OS secret store: macOS Keychain, Windows Credential Manager,
//! or the Secret Service (GNOME Keyring) on Linux. Never written to disk or logs.

/// Same service/account the Mac app has always used, so an existing key keeps working.
pub const SERVICE: &str = "com.caseygibson.NACUT";
pub const ANALYTICS_ACCOUNT: &str = "analytics-api-key";

fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, ANALYTICS_ACCOUNT).ok()
}

pub fn analytics_key() -> Option<String> {
    entry()?.get_password().ok().filter(|k| !k.is_empty())
}

pub fn save_analytics_key(key: &str) -> Result<(), String> {
    entry().ok_or("Secret store unavailable")?.set_password(key).map_err(|e| e.to_string())
}

pub fn delete_analytics_key() {
    if let Some(e) = entry() {
        let _ = e.delete_credential();
    }
}

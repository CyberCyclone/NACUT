//! The Claude Code sign-in (OAuth token) that plan limits are read with.
//!
//! macOS: Keychain item `Claude Code-credentials`. Windows and Linux: `~/.claude/.credentials.json`
//! (or under `$CLAUDE_CONFIG_DIR`). Both hold `{"claudeAiOauth": {"accessToken": …, "expiresAt": ms}}`.
//! The token is only ever read here, never written or refreshed.

use jiff::{SignedDuration, Timestamp};
use serde::Deserialize;

#[derive(Clone)]
pub struct ClaudeToken {
    pub access_token: String,
    pub expires_at: Option<Timestamp>,
    pub subscription_type: Option<String>,
}

impl std::fmt::Debug for ClaudeToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeToken").field("access_token", &"<redacted>").field("expires_at", &self.expires_at).finish()
    }
}

impl ClaudeToken {
    /// Treats tokens within a minute of expiry as expired.
    pub fn is_expired(&self, now: Timestamp) -> bool {
        self.expires_at.is_some_and(|e| e <= now + SignedDuration::from_secs(60))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CredentialError {
    NotFound,
    Unreadable(String),
}

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OAuth>,
}

#[derive(Deserialize)]
struct OAuth {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: Option<f64>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}

pub fn parse(json: &[u8]) -> Result<ClaudeToken, CredentialError> {
    let c: Credentials = serde_json::from_slice(json).map_err(|e| CredentialError::Unreadable(e.to_string()))?;
    let o = c.claude_ai_oauth.ok_or(CredentialError::NotFound)?;
    Ok(ClaudeToken {
        access_token: o.access_token,
        expires_at: o.expires_at.and_then(|ms| Timestamp::from_millisecond(ms as i64).ok()),
        subscription_type: o.subscription_type,
    })
}

/// Reads the current Claude Code sign-in.
pub fn read_claude_token() -> Result<ClaudeToken, CredentialError> {
    #[cfg(target_os = "macos")]
    {
        if let Some(t) = read_keychain() {
            return t;
        }
    }
    for dir in super::claude_config_dirs() {
        let path = dir.join(".credentials.json");
        if let Ok(data) = std::fs::read(&path) {
            return parse(&data);
        }
    }
    Err(CredentialError::NotFound)
}

#[cfg(target_os = "macos")]
fn read_keychain() -> Option<Result<ClaudeToken, CredentialError>> {
    let user = std::env::var("USER").ok()?;
    let entry = keyring::Entry::new("Claude Code-credentials", &user).ok()?;
    match entry.get_password() {
        Ok(json) => Some(parse(json.as_bytes())),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => Some(Err(CredentialError::Unreadable(e.to_string()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_redacts() {
        let t = parse(br#"{"claudeAiOauth":{"accessToken":"secret","refreshToken":"r","expiresAt":1795429672763,"subscriptionType":"max"}}"#).unwrap();
        assert_eq!(t.access_token, "secret");
        assert_eq!(t.expires_at.unwrap().as_millisecond(), 1795429672763);
        assert!(!format!("{t:?}").contains("secret"));
        assert!(t.is_expired(Timestamp::from_millisecond(1795429672763).unwrap()));
        assert_eq!(parse(br#"{}"#).unwrap_err(), CredentialError::NotFound);
    }
}

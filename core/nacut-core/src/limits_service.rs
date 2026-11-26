//! Fetches Max/Pro plan limit utilization with the Claude Code sign-in.
//! Port of `LimitsService.swift`.
//!
//! Uses the same unofficial endpoint as Claude Code's `/usage` screen, so it may change.
//! The token is never refreshed here; when it expires, [`refresher`] asks Claude Code to do it.

use std::time::Duration;

use jiff::Timestamp;

use crate::limits::PlanLimit;
use crate::platform::credentials::{read_claude_token, CredentialError};
use crate::platform::refresher;

pub const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Debug, Clone, PartialEq)]
pub enum LimitsError {
    NoCredential,
    Unreadable(String),
    Expired,
    Http(u16),
    Network(String),
    BadResponse,
}

impl std::fmt::Display for LimitsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let run = "Run `claude` once in a terminal";
        match self {
            Self::NoCredential => write!(f, "No Claude Code sign-in found. {run} and sign in."),
            Self::Unreadable(e) => write!(f, "Couldn't read the Claude Code sign-in ({e})."),
            Self::Expired => write!(f, "Claude Code sign-in expired. {run} to refresh it."),
            Self::Http(401) => write!(f, "Sign-in rejected. {run}."),
            Self::Http(c) => write!(f, "Limits request failed (HTTP {c})."),
            Self::Network(e) => write!(f, "Couldn't reach Anthropic ({e})."),
            Self::BadResponse => write!(f, "Unexpected limits response."),
        }
    }
}

impl std::error::Error for LimitsError {}

fn token() -> Result<(String, bool), LimitsError> {
    match read_claude_token() {
        Ok(t) => {
            let expired = t.is_expired(Timestamp::now());
            Ok((t.access_token, expired))
        }
        Err(CredentialError::NotFound) => Err(LimitsError::NoCredential),
        Err(CredentialError::Unreadable(e)) => Err(LimitsError::Unreadable(e)),
    }
}

/// Reads the sign-in and fetches limits. With `allow_refresh`, an expired (or rejected) sign-in is
/// refreshed by running Claude Code; without it, the user has to run `claude` themselves.
pub fn fetch(allow_refresh: bool) -> Result<Vec<PlanLimit>, LimitsError> {
    let (mut tok, expired) = token()?;
    let mut refreshed = false;
    if expired {
        if !allow_refresh {
            return Err(LimitsError::Expired);
        }
        refreshed = refresher::refresh();
        let (t, still_expired) = token()?;
        if still_expired {
            return Err(LimitsError::Expired);
        }
        tok = t;
    }
    match request(&tok) {
        Err(LimitsError::Http(401)) if !refreshed && allow_refresh => {
            // Token revoked early or rotated elsewhere: let Claude Code refresh once and retry.
            if !refresher::refresh() {
                return Err(LimitsError::Http(401));
            }
            request(&token()?.0)
        }
        other => other,
    }
}

fn request(token: &str) -> Result<Vec<PlanLimit>, LimitsError> {
    let agent = crate::platform::net::agent(Duration::from_secs(20));
    let resp = agent
        .get(USAGE_URL)
        .set("Authorization", &format!("Bearer {token}"))
        .set("anthropic-beta", "oauth-2025-04-20")
        .set("Accept", "application/json")
        .set("User-Agent", crate::analytics::USER_AGENT)
        .call();
    let resp = match resp {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _)) => return Err(LimitsError::Http(code)),
        Err(e) => return Err(LimitsError::Network(e.kind().to_string())),
    };
    let mut body = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(resp.into_reader(), 4 * 1024 * 1024), &mut body)
        .map_err(|e| LimitsError::Network(e.to_string()))?;
    let limits = PlanLimit::parse(&body);
    if limits.is_empty() {
        Err(LimitsError::BadResponse)
    } else {
        Ok(limits)
    }
}

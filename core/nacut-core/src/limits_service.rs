//! Fetches Max/Pro plan limit utilization with the Claude Code sign-in.
//! Port of `LimitsService.swift`.
//!
//! Uses the same unofficial endpoint as Claude Code's `/usage` screen, so it may change.
//! The token is never refreshed here; when it expires, [`refresher`] asks Claude Code to do it.

use std::time::Duration;

use jiff::Timestamp;

use crate::limits::PlanLimit;
use crate::platform::credentials::{read_claude_token, ClaudeToken, CredentialError};
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
    token_from(read_claude_token(), Timestamp::now())
}

/// The access token and whether it has expired, or why there isn't one.
fn token_from(read: Result<ClaudeToken, CredentialError>, now: Timestamp) -> Result<(String, bool), LimitsError> {
    match read {
        Ok(t) => {
            let expired = t.is_expired(now);
            Ok((t.access_token, expired))
        }
        Err(CredentialError::NotFound) => Err(LimitsError::NoCredential),
        Err(CredentialError::Unreadable(e)) => Err(LimitsError::Unreadable(e)),
    }
}

/// Reads the sign-in and fetches limits. With `allow_refresh`, an expired (or rejected) sign-in is
/// refreshed by running Claude Code; without it, the user has to run `claude` themselves.
pub fn fetch(allow_refresh: bool) -> Result<Vec<PlanLimit>, LimitsError> {
    fetch_with(allow_refresh, &mut token, &mut request, &mut refresher::refresh)
}

/// [`fetch`] with the sign-in reader, the HTTP request and the Claude Code refresh passed in.
fn fetch_with(
    allow_refresh: bool,
    token: &mut dyn FnMut() -> Result<(String, bool), LimitsError>,
    request: &mut dyn FnMut(&str) -> Result<Vec<PlanLimit>, LimitsError>,
    refresh: &mut dyn FnMut() -> bool,
) -> Result<Vec<PlanLimit>, LimitsError> {
    let (mut tok, expired) = token()?;
    let mut refreshed = false;
    if expired {
        if !allow_refresh {
            return Err(LimitsError::Expired);
        }
        refreshed = refresh();
        let (t, still_expired) = token()?;
        if still_expired {
            return Err(LimitsError::Expired);
        }
        tok = t;
    }
    match request(&tok) {
        Err(LimitsError::Http(401)) if !refreshed && allow_refresh => {
            // Token revoked early or rotated elsewhere: let Claude Code refresh once and retry.
            if !refresh() {
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
    limits_from(&body)
}

/// Parsed limits, or [`LimitsError::BadResponse`] when the body has none.
fn limits_from(body: &[u8]) -> Result<Vec<PlanLimit>, LimitsError> {
    let limits = PlanLimit::parse(body);
    if limits.is_empty() {
        Err(LimitsError::BadResponse)
    } else {
        Ok(limits)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    fn limit() -> Vec<PlanLimit> {
        vec![PlanLimit { id: "five_hour".into(), label: "5-hour".into(), utilization: 0.1, resets_at: None }]
    }

    /// Runs `fetch_with` against scripted sign-ins and responses; returns the result, the tokens requested and
    /// how many refreshes ran.
    fn run(
        allow_refresh: bool,
        tokens: Vec<Result<(&str, bool), LimitsError>>,
        responses: Vec<Result<Vec<PlanLimit>, LimitsError>>,
        refresh_ok: bool,
    ) -> (Result<Vec<PlanLimit>, LimitsError>, Vec<String>, usize) {
        let tokens = RefCell::new(tokens.into_iter());
        let responses = RefCell::new(responses.into_iter());
        let mut sent = vec![];
        let mut refreshes = 0;
        let result = fetch_with(
            allow_refresh,
            &mut || tokens.borrow_mut().next().expect("unexpected sign-in read").map(|(t, e)| (t.to_string(), e)),
            &mut |t| {
                sent.push(t.to_string());
                responses.borrow_mut().next().expect("unexpected request")
            },
            &mut || {
                refreshes += 1;
                refresh_ok
            },
        );
        (result, sent, refreshes)
    }

    #[test]
    fn valid_sign_in_fetches_once() {
        let (r, sent, refreshes) = run(true, vec![Ok(("a", false))], vec![Ok(limit())], true);
        assert_eq!(r, Ok(limit()));
        assert_eq!((sent, refreshes), (vec!["a".to_string()], 0));
    }

    #[test]
    fn missing_or_unreadable_sign_in_stops_before_the_network() {
        let (r, sent, _) = run(true, vec![Err(LimitsError::NoCredential)], vec![], true);
        assert_eq!((r, sent.len()), (Err(LimitsError::NoCredential), 0));
        let (r, _, _) = run(true, vec![Err(LimitsError::Unreadable("bad".into()))], vec![], true);
        assert_eq!(r, Err(LimitsError::Unreadable("bad".into())));
    }

    #[test]
    fn expired_sign_in_without_refresh_asks_the_user() {
        let (r, sent, refreshes) = run(false, vec![Ok(("a", true))], vec![], true);
        assert_eq!((r, sent.len(), refreshes), (Err(LimitsError::Expired), 0, 0));
    }

    #[test]
    fn expired_sign_in_is_refreshed_then_used() {
        let (r, sent, refreshes) = run(true, vec![Ok(("old", true)), Ok(("new", false))], vec![Ok(limit())], true);
        assert_eq!(r, Ok(limit()));
        assert_eq!((sent, refreshes), (vec!["new".to_string()], 1));
    }

    #[test]
    fn sign_in_still_expired_after_refresh() {
        let (r, sent, refreshes) = run(true, vec![Ok(("old", true)), Ok(("old", true))], vec![], false);
        assert_eq!((r, sent.len(), refreshes), (Err(LimitsError::Expired), 0, 1));
    }

    #[test]
    fn rejected_sign_in_is_refreshed_and_retried_once() {
        let (r, sent, refreshes) =
            run(true, vec![Ok(("a", false)), Ok(("b", false))], vec![Err(LimitsError::Http(401)), Ok(limit())], true);
        assert_eq!(r, Ok(limit()));
        assert_eq!((sent, refreshes), (vec!["a".to_string(), "b".to_string()], 1));
    }

    #[test]
    fn rejected_sign_in_stays_rejected_when_refresh_fails() {
        let (r, sent, refreshes) = run(true, vec![Ok(("a", false))], vec![Err(LimitsError::Http(401))], false);
        assert_eq!((r, sent.len(), refreshes), (Err(LimitsError::Http(401)), 1, 1));
    }

    #[test]
    fn rejected_sign_in_without_refresh_is_not_retried() {
        let (r, sent, refreshes) = run(false, vec![Ok(("a", false))], vec![Err(LimitsError::Http(401))], true);
        assert_eq!((r, sent.len(), refreshes), (Err(LimitsError::Http(401)), 1, 0));
    }

    #[test]
    fn no_second_refresh_after_an_expired_sign_in_was_refreshed() {
        let (r, sent, refreshes) =
            run(true, vec![Ok(("old", true)), Ok(("new", false))], vec![Err(LimitsError::Http(401))], true);
        assert_eq!((r, sent.len(), refreshes), (Err(LimitsError::Http(401)), 1, 1));
    }

    #[test]
    fn other_errors_pass_through() {
        for e in [LimitsError::Http(500), LimitsError::Network("dns".into()), LimitsError::BadResponse] {
            let (r, _, refreshes) = run(true, vec![Ok(("a", false))], vec![Err(e.clone())], true);
            assert_eq!((r, refreshes), (Err(e), 0));
        }
    }

    #[test]
    fn sign_in_errors_map_to_limit_errors() {
        let now: Timestamp = "2026-11-20T00:00:00Z".parse().unwrap();
        let tok = |exp: &str| ClaudeToken { access_token: "t".into(), expires_at: Some(exp.parse().unwrap()), subscription_type: None };
        assert_eq!(token_from(Ok(tok("2026-11-21T00:00:00Z")), now), Ok(("t".into(), false)));
        assert_eq!(token_from(Ok(tok("2026-11-19T00:00:00Z")), now), Ok(("t".into(), true)));
        assert_eq!(token_from(Err(CredentialError::NotFound), now), Err(LimitsError::NoCredential));
        assert_eq!(token_from(Err(CredentialError::Unreadable("x".into())), now), Err(LimitsError::Unreadable("x".into())));
    }

    #[test]
    fn responses_without_limits_are_bad() {
        assert_eq!(limits_from(b"{}"), Err(LimitsError::BadResponse));
        assert_eq!(limits_from(b"not json"), Err(LimitsError::BadResponse));
        let live = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../shared/fixtures/limits-live.json")).unwrap();
        assert!(!limits_from(&live).unwrap().is_empty());
    }

    #[test]
    fn error_messages_tell_the_user_what_to_do() {
        assert!(LimitsError::NoCredential.to_string().contains("Run `claude`"));
        assert!(LimitsError::Expired.to_string().contains("expired"));
        assert_eq!(LimitsError::Http(401).to_string(), "Sign-in rejected. Run `claude` once in a terminal.");
        assert_eq!(LimitsError::Http(503).to_string(), "Limits request failed (HTTP 503).");
        assert_eq!(LimitsError::Network("timed out".into()).to_string(), "Couldn't reach Anthropic (timed out).");
        assert_eq!(LimitsError::Unreadable("bad json".into()).to_string(), "Couldn't read the Claude Code sign-in (bad json).");
        assert_eq!(LimitsError::BadResponse.to_string(), "Unexpected limits response.");
    }
}

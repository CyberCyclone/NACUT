//! Client for Anthropic's Usage & Cost Analytics API (beta).
//! Port of `AnalyticsClient.swift` and `AnalyticsModels.swift`.
//!
//! Works with a Console Admin API key (`sk-ant-admin…`) or a Claude Enterprise
//! scoped key with `read:analytics`.

use std::collections::HashMap;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::scanner::parse_date;
use crate::store::Cal;

pub const BETA_HEADER: &str = "analytics-usage-cost-2026-08-24";
pub const API_VERSION: &str = "2026-10-21";
pub const USER_AGENT: &str = "NACUT/1.0";
const COST_PATH: &str = "/v1/organizations/analytics/cost";
/// Stops paging if the server keeps saying there's more (≈ 1,000 rows per page; a year of hourly rows by
/// model needs well under this).
const MAX_PAGES: usize = 500;

/// One hour of billed cost for one model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiCostRow {
    pub start: Timestamp,
    pub end: Timestamp,
    pub model: Option<String>,
    /// USD after discounts, before credits.
    pub amount: f64,
    /// USD at list price.
    pub list_amount: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct ApiActor {
    pub id: String,
    /// `user`, `api_key`, `service_account`…
    pub kind: String,
    pub display: String,
    pub cost: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiCostData {
    pub rows: Vec<ApiCostRow>,
    pub refreshed_at: Option<Timestamp>,
    pub coverage_start: Timestamp,
    pub coverage_end: Timestamp,
    pub fetched_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "detail", rename_all = "camelCase")]
pub enum ConnectionStatus {
    NotConfigured,
    Connected { refreshed_at: Option<Timestamp> },
    NoDataYet,
    Unauthorized(String),
    Forbidden(String),
    Unavailable(String),
    Failed(String),
}

impl ConnectionStatus {
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Connected { .. } | Self::NoDataYet)
    }

    pub fn message(&self, cal: &Cal) -> String {
        match self {
            Self::NotConfigured => "Not connected. Using local logs.".into(),
            Self::Connected { refreshed_at: Some(at) } => {
                format!("Connected · data through {}", at.to_zoned(cal.tz.clone()).strftime("%b %-d, %-I:%M %p"))
            }
            Self::Connected { refreshed_at: None } => "Connected".into(),
            Self::NoDataYet => "Connected, but the API has no data for this period yet (it can lag up to 24 hours).".into(),
            Self::Unauthorized(m) => format!(
                "Key rejected (401). Check that you pasted an Admin key or a scoped key with read:analytics. {m}"
            ),
            Self::Forbidden(m) => format!(
                "Access denied (403). This key or your organization isn't allowed to read usage analytics. {m}"
            ),
            Self::Unavailable(m) => format!("Usage analytics aren't available to this organization. {m}"),
            Self::Failed(m) => format!("Couldn't reach the API: {m}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    Http(u16, String),
    CursorExpired,
    Decoding(String),
    Transport(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(c, m) => write!(f, "HTTP {c} {m}"),
            Self::CursorExpired => write!(f, "cursor expired"),
            Self::Decoding(m) => write!(f, "unexpected response: {m}"),
            Self::Transport(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ApiError {}

/// A recorded HTTP request, for tests.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

impl HttpRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// Sends GET requests. `Ok((status, body))` for any HTTP response; `Err` for network failures.
pub trait Transport: Send + Sync {
    fn get(&self, req: &HttpRequest) -> Result<(u16, Vec<u8>), String>;
}

/// The real network, with a 30 s timeout.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self { agent: crate::platform::net::agent(Duration::from_secs(30)) }
    }
}

impl Transport for UreqTransport {
    fn get(&self, req: &HttpRequest) -> Result<(u16, Vec<u8>), String> {
        let mut r = self.agent.get(&req.url);
        for (k, v) in &req.headers {
            r = r.set(k, v);
        }
        let read = |resp: ureq::Response| {
            use std::io::Read;
            let status = resp.status();
            let mut body = Vec::new();
            resp.into_reader().take(8 * 1024 * 1024).read_to_end(&mut body).map_err(|e| e.to_string())?;
            Ok((status, body))
        };
        match r.call() {
            Ok(resp) => read(resp),
            Err(ureq::Error::Status(_, resp)) => read(resp),
            Err(e) => Err(e.to_string()),
        }
    }
}

pub struct AnalyticsClient {
    pub key: String,
    pub base_url: String,
    transport: Box<dyn Transport>,
}

#[derive(Deserialize)]
struct WirePage {
    #[serde(default)]
    data: Vec<WireCostRow>,
    has_more: Option<bool>,
    next_page: Option<String>,
    data_refreshed_at: Option<String>,
}

#[derive(Deserialize)]
struct WireCostRow {
    starting_at: String,
    ending_at: String,
    group: Option<WireGroup>,
    cost: WireCost,
}

#[derive(Deserialize)]
struct WireGroup {
    model: Option<String>,
    actor: Option<Value>,
}

#[derive(Deserialize)]
struct WireCost {
    amount: Option<String>,
    list_amount: Option<String>,
}

impl AnalyticsClient {
    pub fn new(key: impl Into<String>) -> Self {
        Self::with_transport(key, Box::new(UreqTransport::default()))
    }

    pub fn with_transport(key: impl Into<String>, transport: Box<dyn Transport>) -> Self {
        Self { key: key.into(), base_url: "https://api.anthropic.com".into(), transport }
    }

    /// Quick check: one cheap request over the last 7 days.
    pub fn test_connection(&self, now: Timestamp) -> ConnectionStatus {
        let start = now - SignedDuration::from_hours(7 * 24);
        match self.get(&[("starting_at", rfc3339(start)), ("ending_at", rfc3339(now)), ("limit", "1".into())]) {
            Ok(page) => match page.data_refreshed_at.as_deref().and_then(parse_date) {
                Some(r) => ConnectionStatus::Connected { refreshed_at: Some(r) },
                None if page.data.is_empty() => ConnectionStatus::NoDataYet,
                None => ConnectionStatus::Connected { refreshed_at: None },
            },
            Err(e) => status_for(&e),
        }
    }

    /// Hourly cost by model from `start` to `end`, split into ≤31-day requests. The API serves nothing before
    /// 2026-01-01 or more than 364 days back, so coverage starts there when `start` is older: the merge treats
    /// every covered day without rows as billed $0, which would otherwise wipe out older local-log days.
    pub fn hourly_cost(&self, start: Timestamp, end: Timestamp, actor_id: Option<&str>) -> Result<ApiCostData, ApiError> {
        let mut rows = Vec::new();
        let mut refreshed: Option<Timestamp> = None;
        let coverage_start = start.max(earliest(end));
        let mut chunk_start = floor_hour(coverage_start);
        while chunk_start < end {
            let chunk_end = (chunk_start + SignedDuration::from_hours(31 * 24)).min(end);
            let mut query = vec![
                ("starting_at", rfc3339(chunk_start)),
                ("ending_at", rfc3339(chunk_end)),
                ("time_bucket_width", "1h".to_string()),
                ("group_by[]", "model".to_string()),
                ("order_by", "time".to_string()),
                ("order", "asc".to_string()),
                ("limit", "1000".to_string()),
            ];
            if let Some(a) = actor_id {
                query.push(("actor_ids[]", a.to_string()));
            }
            let (chunk_rows, r) = self.paged(&query)?;
            rows.extend(chunk_rows);
            if let Some(r) = r {
                refreshed = Some(refreshed.map_or(r, |x| x.max(r)));
            }
            chunk_start = chunk_end;
        }
        Ok(ApiCostData { rows, refreshed_at: refreshed, coverage_start, coverage_end: end, fetched_at: Timestamp::now() })
    }

    /// Who spent what over the last 30 days, for picking "only my usage".
    pub fn actors(&self, now: Timestamp) -> Result<Vec<ApiActor>, ApiError> {
        let start = now - SignedDuration::from_hours(30 * 24);
        let mut out: HashMap<String, ApiActor> = HashMap::new();
        let mut page: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut q = vec![
                ("starting_at", rfc3339(start)),
                ("ending_at", rfc3339(now)),
                ("group_by[]", "actor".to_string()),
                ("limit", "100".to_string()),
            ];
            if let Some(p) = &page {
                q.push(("page", p.clone()));
            }
            let p = self.get(&q)?;
            for row in p.data {
                let Some(actor) = actor_from(row.group.as_ref().and_then(|g| g.actor.as_ref())) else { continue };
                out.entry(actor.id.clone()).or_insert(actor).cost += usd(row.cost.amount.as_deref());
            }
            page = if p.has_more == Some(true) { p.next_page } else { None };
            if page.is_none() {
                let mut v: Vec<ApiActor> = out.into_values().collect();
                v.sort_by(|a, b| b.cost.total_cmp(&a.cost));
                return Ok(v);
            }
        }
        Err(ApiError::Decoding("too many pages".into()))
    }

    fn paged(&self, query: &[(&str, String)]) -> Result<(Vec<ApiCostRow>, Option<Timestamp>), ApiError> {
        let mut attempts = 0;
        loop {
            match self.paged_once(query) {
                Err(ApiError::CursorExpired) if attempts < 2 => attempts += 1, // data refreshed mid-paging: start over
                other => return other,
            }
        }
    }

    fn paged_once(&self, query: &[(&str, String)]) -> Result<(Vec<ApiCostRow>, Option<Timestamp>), ApiError> {
        let mut rows = Vec::new();
        let mut page: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut q = query.to_vec();
            if let Some(p) = &page {
                q.push(("page", p.clone()));
            }
            let p = self.get(&q)?;
            let refreshed = p.data_refreshed_at.as_deref().and_then(parse_date);
            for row in p.data {
                let (Some(s), Some(e)) = (parse_date(&row.starting_at), parse_date(&row.ending_at)) else { continue };
                rows.push(ApiCostRow {
                    start: s,
                    end: e,
                    model: row.group.and_then(|g| g.model),
                    amount: usd(row.cost.amount.as_deref()),
                    list_amount: usd(row.cost.list_amount.as_deref()),
                });
            }
            page = if p.has_more == Some(true) { p.next_page } else { None };
            if page.is_none() {
                return Ok((rows, refreshed));
            }
        }
        Err(ApiError::Decoding("too many pages".into()))
    }

    fn get(&self, query: &[(&str, String)]) -> Result<WirePage, ApiError> {
        let qs: Vec<String> = query.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect();
        let req = HttpRequest {
            url: format!("{}{}?{}", self.base_url.trim_end_matches('/'), COST_PATH, qs.join("&")),
            headers: vec![
                ("x-api-key".into(), self.key.clone()),
                ("anthropic-version".into(), API_VERSION.into()),
                ("anthropic-beta".into(), BETA_HEADER.into()),
                ("User-Agent".into(), USER_AGENT.into()),
            ],
        };
        let (status, body) = self.transport.get(&req).map_err(ApiError::Transport)?;
        match status {
            200..=299 => serde_json::from_slice(&body).map_err(|e| ApiError::Decoding(e.to_string())),
            410 => Err(ApiError::CursorExpired),
            _ => {
                let msg = serde_json::from_slice::<Value>(&body)
                    .ok()
                    .and_then(|v| v.get("error")?.get("message")?.as_str().map(str::to_string))
                    .unwrap_or_else(|| String::from_utf8_lossy(&body[..body.len().min(300)]).into_owned());
                Err(ApiError::Http(status, msg))
            }
        }
    }
}

pub fn status_for(e: &ApiError) -> ConnectionStatus {
    match e {
        ApiError::Http(401, m) => ConnectionStatus::Unauthorized(m.clone()),
        ApiError::Http(403, m) => ConnectionStatus::Forbidden(m.clone()),
        ApiError::Http(404, m) => ConnectionStatus::Unavailable(if m.is_empty() { "(404)".into() } else { m.clone() }),
        ApiError::Http(code, m) => ConnectionStatus::Failed(format!("HTTP {code} {m}")),
        ApiError::Decoding(m) => ConnectionStatus::Failed(format!("Unexpected response: {}", truncate(m, 160))),
        ApiError::CursorExpired => ConnectionStatus::Failed("The API kept changing while paging; try again.".into()),
        ApiError::Transport(m) => ConnectionStatus::Failed(m.clone()),
    }
}

/// At most `max` bytes of `s`, cut on a character boundary.
fn truncate(s: &str, max: usize) -> &str {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Amounts are decimal strings in cents.
fn usd(cents: Option<&str>) -> f64 {
    cents.and_then(|c| c.parse::<f64>().ok()).map(|v| v / 100.0).unwrap_or(0.0)
}

pub fn actor_from(v: Option<&Value>) -> Option<ApiActor> {
    let v = v.filter(|v| v.is_object())?;
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let id = s("user_id").or_else(|| s("service_account_id")).or_else(|| s("api_key_id")).or_else(|| s("id")).or_else(|| s("email_address"))?;
    let display = s("email_address").or_else(|| s("email")).or_else(|| s("name")).unwrap_or_else(|| id.clone());
    Some(ApiActor { id, kind: s("type").unwrap_or_else(|| "unknown".into()), display, cost: 0.0 })
}

/// The first moment the Usage & Cost API has data for: nothing is served before 2026-01-01.
pub fn api_epoch() -> Timestamp {
    "2026-01-01T00:00:00Z".parse().expect("valid")
}

/// The API serves at most 365 days back and nothing before 2026-01-01.
fn earliest(now: Timestamp) -> Timestamp {
    api_epoch().max(now - SignedDuration::from_hours(364 * 24))
}

fn floor_hour(t: Timestamp) -> Timestamp {
    Timestamp::from_second(t.as_second().div_euclid(3600) * 3600).unwrap_or(t)
}

fn rfc3339(t: Timestamp) -> String {
    Timestamp::from_second(t.as_second()).unwrap_or(t).to_string()
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn encode_keeps_unreserved_characters() {
        assert_eq!(encode("aZ09-._~"), "aZ09-._~");
    }

    #[test]
    fn encode_escapes_everything_else_as_upper_hex() {
        assert_eq!(encode("group_by[]"), "group_by%5B%5D");
        assert_eq!(encode("2026-11-20T01:00:00Z"), "2026-11-20T01%3A00%3A00Z");
        assert_eq!(encode("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
        assert_eq!(encode(""), "");
    }

    #[test]
    fn floor_hour_drops_minutes_seconds_and_fractions() {
        assert_eq!(floor_hour(ts("2026-11-20T10:59:59.999Z")), ts("2026-11-20T10:00:00Z"));
        assert_eq!(floor_hour(ts("2026-11-20T10:00:00Z")), ts("2026-11-20T10:00:00Z"));
        assert_eq!(floor_hour(ts("1969-12-31T23:30:00Z")), ts("1969-12-31T23:00:00Z"));
    }

    #[test]
    fn rfc3339_has_whole_seconds_in_utc() {
        assert_eq!(rfc3339(ts("2026-11-20T10:30:45.999Z")), "2026-11-20T10:30:45Z");
        assert_eq!(rfc3339(ts("2026-11-20T10:30:45+02:00")), "2026-11-20T08:30:45Z");
    }

    #[test]
    fn earliest_is_the_later_of_2026_and_364_days_back() {
        assert_eq!(earliest(ts("2026-06-01T00:00:00Z")), ts("2026-01-01T00:00:00Z"));
        assert_eq!(earliest(ts("2027-06-01T00:00:00Z")), ts("2026-06-02T00:00:00Z"));
    }

    #[test]
    fn usd_converts_cents_and_defaults_to_zero() {
        assert_eq!(usd(Some("12345")), 123.45);
        assert_eq!(usd(Some("-50")), -0.5);
        assert_eq!(usd(Some("")), 0.0);
        assert_eq!(usd(Some("1,000")), 0.0);
        assert_eq!(usd(None), 0.0);
    }

    #[test]
    fn truncate_never_splits_a_character() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("abc", 2), "ab");
        assert_eq!(truncate("aé", 2), "a");
        assert_eq!(truncate("", 0), "");
    }
}

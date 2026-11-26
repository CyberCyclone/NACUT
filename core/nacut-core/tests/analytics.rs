//! The Usage & Cost API client against a fake transport: chunking, paging, errors and status messages.

use std::sync::{Arc, Mutex};

use nacut_core::analytics::{
    actor_from, status_for, AnalyticsClient, ApiActor, ApiError, ConnectionStatus, HttpRequest, Transport, API_VERSION, BETA_HEADER, USER_AGENT,
};
use nacut_core::Cal;
use serde_json::json;

mod common;

use common::{ts, Pages, Shared};

const EMPTY: &str = r#"{"data":[],"has_more":false,"next_page":null,"data_refreshed_at":null}"#;

fn client(pages: &Arc<Pages>) -> AnalyticsClient {
    AnalyticsClient::with_transport("k", Box::new(Shared(pages.clone())))
}

/// Answers every request with the same response and records the URLs.
#[derive(Clone)]
struct Repeat {
    status: u16,
    body: String,
    urls: Arc<Mutex<Vec<String>>>,
}

impl Repeat {
    fn new(status: u16, body: &str) -> Self {
        Self { status, body: body.into(), urls: Arc::default() }
    }
    fn urls(&self) -> Vec<String> {
        self.urls.lock().unwrap().clone()
    }
}

impl Transport for Repeat {
    fn get(&self, req: &HttpRequest) -> Result<(u16, Vec<u8>), String> {
        self.urls.lock().unwrap().push(req.url.clone());
        Ok((self.status, self.body.clone().into_bytes()))
    }
}

struct Offline;

impl Transport for Offline {
    fn get(&self, _req: &HttpRequest) -> Result<(u16, Vec<u8>), String> {
        Err("connection refused".into())
    }
}

fn query_param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    url.split_once('?')?.1.split('&').find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
}

fn page(refreshed: Option<&str>) -> String {
    json!({"data": [], "has_more": false, "next_page": null, "data_refreshed_at": refreshed}).to_string()
}

// MARK: hourly_cost

#[test]
fn first_request_has_the_full_query_and_headers() {
    let pages = Pages::new(vec![(200, EMPTY)]);
    client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None).unwrap();
    let calls = pages.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].url,
        "https://api.anthropic.com/v1/organizations/analytics/cost?starting_at=2026-03-01T00%3A00%3A00Z&ending_at=2026-03-02T00%3A00%3A00Z&time_bucket_width=1h&group_by%5B%5D=model&order_by=time&order=asc&limit=1000"
    );
    assert_eq!(calls[0].header("anthropic-version"), Some(API_VERSION));
    assert_eq!(calls[0].header("anthropic-beta"), Some(BETA_HEADER));
    assert_eq!(calls[0].header("user-agent"), Some(USER_AGENT));
}

#[test]
fn ranges_over_31_days_are_split_into_chunks() {
    let p1 = page(Some("2026-04-01T05:00:00Z"));
    let p2 = page(Some("2026-03-31T00:00:00Z"));
    let pages = Pages::new(vec![(200, &p1), (200, &p2)]);
    let data = client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-05-01T00:00:00Z"), None).unwrap();
    let calls = pages.calls();
    let bounds: Vec<_> = calls.iter().map(|c| (query_param(&c.url, "starting_at").unwrap(), query_param(&c.url, "ending_at").unwrap())).collect();
    assert_eq!(
        bounds,
        [("2026-03-01T00%3A00%3A00Z", "2026-04-01T00%3A00%3A00Z"), ("2026-04-01T00%3A00%3A00Z", "2026-05-01T00%3A00%3A00Z")]
    );
    // The latest refresh time across chunks wins.
    assert_eq!(data.refreshed_at, Some(ts("2026-04-01T05:00:00Z")));
    assert_eq!(data.coverage_start, ts("2026-03-01T00:00:00Z"));
    assert_eq!(data.coverage_end, ts("2026-05-01T00:00:00Z"));
}

#[test]
fn no_refresh_time_in_any_chunk_leaves_it_unset() {
    let pages = Pages::new(vec![(200, EMPTY), (200, EMPTY)]);
    let data = client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-05-01T00:00:00Z"), None).unwrap();
    assert_eq!(data.refreshed_at, None);
}

#[test]
fn start_is_clamped_to_2026() {
    let pages = Pages::new(vec![(200, EMPTY)]);
    let data = client(&pages).hourly_cost(ts("2025-06-01T00:00:00Z"), ts("2026-01-15T00:00:00Z"), None).unwrap();
    assert_eq!(query_param(&pages.calls()[0].url, "starting_at"), Some("2026-01-01T00%3A00%3A00Z"));
    // Coverage starts where the data does: days before it weren't fetched, so they mustn't count as billed $0.
    assert_eq!(data.coverage_start, ts("2026-01-01T00:00:00Z"));
}

#[test]
fn start_is_clamped_to_364_days_before_the_end() {
    let t = Repeat::new(200, EMPTY);
    let c = AnalyticsClient::with_transport("k", Box::new(t.clone()));
    c.hourly_cost(ts("2026-01-01T00:00:00Z"), ts("2027-03-01T00:00:00Z"), None).unwrap();
    let urls = t.urls();
    assert_eq!(query_param(&urls[0], "starting_at"), Some("2026-03-02T00%3A00%3A00Z"));
    assert_eq!(query_param(urls.last().unwrap(), "ending_at"), Some("2027-03-01T00%3A00%3A00Z"));
    assert_eq!(urls.len(), 12);
    assert_eq!(c.hourly_cost(ts("2026-01-01T00:00:00Z"), ts("2027-03-01T00:00:00Z"), None).unwrap().coverage_start, ts("2026-03-02T00:00:00Z"));
}

#[test]
fn coverage_keeps_an_unclamped_start_as_requested() {
    let pages = Pages::new(vec![(200, EMPTY)]);
    let data = client(&pages).hourly_cost(ts("2026-03-01T10:30:00Z"), ts("2026-03-02T00:00:00Z"), None).unwrap();
    assert_eq!(data.coverage_start, ts("2026-03-01T10:30:00Z"));
}

#[test]
fn start_is_floored_to_the_hour_and_seconds_are_dropped() {
    let pages = Pages::new(vec![(200, EMPTY)]);
    client(&pages).hourly_cost(ts("2026-03-01T10:30:45.5Z"), ts("2026-03-02T00:00:00.9Z"), None).unwrap();
    let url = &pages.calls()[0].url;
    assert_eq!(query_param(url, "starting_at"), Some("2026-03-01T10%3A00%3A00Z"));
    assert_eq!(query_param(url, "ending_at"), Some("2026-03-02T00%3A00%3A00Z"));
}

#[test]
fn actor_filter_is_appended() {
    let pages = Pages::new(vec![(200, EMPTY)]);
    client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), Some("user_1")).unwrap();
    assert!(pages.calls()[0].url.ends_with("&limit=1000&actor_ids%5B%5D=user_1"), "{}", pages.calls()[0].url);
}

#[test]
fn an_empty_range_makes_no_requests() {
    let pages = Pages::new(vec![]);
    let start = ts("2026-03-02T00:00:00Z");
    let data = client(&pages).hourly_cost(start, start, None).unwrap();
    assert!(data.rows.is_empty());
    assert_eq!(data.refreshed_at, None);
    let data = client(&pages).hourly_cost(start, ts("2026-03-01T00:00:00Z"), None).unwrap();
    assert!(data.rows.is_empty());
    assert!(pages.calls().is_empty());
}

#[test]
fn three_cursor_expiries_in_a_row_give_up() {
    let pages = Pages::new(vec![(410, "{}"), (410, "{}"), (410, "{}")]);
    let r = client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None);
    assert_eq!(r, Err(ApiError::CursorExpired));
    assert_eq!(pages.calls().len(), 3);
}

#[test]
fn endless_paging_stops_after_500_pages() {
    let t = Repeat::new(200, r#"{"data":[],"has_more":true,"next_page":"again"}"#);
    let c = AnalyticsClient::with_transport("k", Box::new(t.clone()));
    let r = c.hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None);
    assert_eq!(r, Err(ApiError::Decoding("too many pages".into())));
    assert_eq!(t.urls().len(), 500);
}

#[test]
fn has_more_without_a_next_page_stops() {
    let pages = Pages::new(vec![(200, r#"{"data":[],"has_more":true,"next_page":null}"#)]);
    assert!(client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None).is_ok());
    assert_eq!(pages.calls().len(), 1);
}

#[test]
fn rows_with_bad_dates_are_skipped_and_missing_amounts_are_zero() {
    let body = json!({
        "data": [
            {"starting_at": "nope", "ending_at": "2026-03-01T01:00:00Z", "cost": {"amount": "100"}},
            {"starting_at": "2026-03-01T00:00:00Z", "ending_at": "", "cost": {"amount": "100"}},
            {"starting_at": "2026-03-01T00:00:00Z", "ending_at": "2026-03-01T01:00:00Z", "cost": {}},
            {"starting_at": "2026-03-01T01:00:00Z", "ending_at": "2026-03-01T02:00:00Z", "group": {"model": "claude-opus-5-5"}, "cost": {"amount": "abc", "list_amount": "250.5"}}
        ],
        "has_more": false
    })
    .to_string();
    let pages = Pages::new(vec![(200, &body)]);
    let data = client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None).unwrap();
    assert_eq!(data.rows.len(), 2);
    assert_eq!((data.rows[0].model.as_deref(), data.rows[0].amount, data.rows[0].list_amount), (None, 0.0, 0.0));
    assert_eq!(data.rows[0].start, ts("2026-03-01T00:00:00Z"));
    assert_eq!((data.rows[1].model.as_deref(), data.rows[1].amount, data.rows[1].list_amount), (Some("claude-opus-5-5"), 0.0, 2.505));
}

#[test]
fn non_json_success_is_a_decoding_error() {
    let pages = Pages::new(vec![(200, "<html>proxy login</html>")]);
    let r = client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None);
    assert!(matches!(r, Err(ApiError::Decoding(_))), "{r:?}");
}

#[test]
fn error_message_is_read_from_the_json_body() {
    let pages = Pages::new(vec![(429, r#"{"error":{"type":"rate_limit_error","message":"slow down"}}"#)]);
    let r = client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None);
    assert_eq!(r, Err(ApiError::Http(429, "slow down".into())));
}

#[test]
fn error_without_a_message_keeps_the_first_300_bytes_of_the_body() {
    let body = "x".repeat(400);
    let pages = Pages::new(vec![(502, &body)]);
    let r = client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None);
    assert_eq!(r, Err(ApiError::Http(502, "x".repeat(300))));
    let pages = Pages::new(vec![(500, r#"{"error":{"type":"api_error"}}"#)]);
    let r = client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None);
    assert_eq!(r, Err(ApiError::Http(500, r#"{"error":{"type":"api_error"}}"#.into())));
}

#[test]
fn network_failures_are_transport_errors() {
    let c = AnalyticsClient::with_transport("k", Box::new(Offline));
    let r = c.hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None);
    assert_eq!(r, Err(ApiError::Transport("connection refused".into())));
}

#[test]
fn trailing_slash_on_the_base_url_is_ignored() {
    let pages = Pages::new(vec![(200, EMPTY)]);
    let mut c = client(&pages);
    c.base_url = "https://example.test/".into();
    c.hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None).unwrap();
    assert!(pages.calls()[0].url.starts_with("https://example.test/v1/organizations/analytics/cost?"));
}

// MARK: test_connection

#[test]
fn connection_test_asks_for_one_row_over_seven_days() {
    let pages = Pages::new(vec![(200, EMPTY)]);
    client(&pages).test_connection(ts("2026-11-20T12:34:56Z"));
    let url = &pages.calls()[0].url;
    assert!(url.ends_with("?starting_at=2026-11-13T12%3A34%3A56Z&ending_at=2026-11-20T12%3A34%3A56Z&limit=1"), "{url}");
}

#[test]
fn connection_with_a_refresh_time() {
    let p = page(Some("2026-11-24T16:02:11Z"));
    let pages = Pages::new(vec![(200, &p)]);
    assert_eq!(client(&pages).test_connection(ts("2026-11-25T00:00:00Z")), ConnectionStatus::Connected { refreshed_at: Some(ts("2026-11-24T16:02:11Z")) });
}

#[test]
fn connection_with_data_but_no_refresh_time() {
    let body = json!({"data": [{"starting_at": "2026-11-24T00:00:00Z", "ending_at": "2026-11-24T01:00:00Z", "cost": {}}], "has_more": false}).to_string();
    let pages = Pages::new(vec![(200, &body)]);
    assert_eq!(client(&pages).test_connection(ts("2026-11-25T00:00:00Z")), ConnectionStatus::Connected { refreshed_at: None });
}

#[test]
fn connection_errors_map_to_statuses() {
    let pages = Pages::new(vec![(401, r#"{"error":{"message":"invalid x-api-key"}}"#)]);
    assert_eq!(client(&pages).test_connection(ts("2026-11-25T00:00:00Z")), ConnectionStatus::Unauthorized("invalid x-api-key".into()));
    let c = AnalyticsClient::with_transport("k", Box::new(Offline));
    assert_eq!(c.test_connection(ts("2026-11-25T00:00:00Z")), ConnectionStatus::Failed("connection refused".into()));
}

// MARK: actors

fn actor_row(actor: serde_json::Value, cents: &str) -> serde_json::Value {
    json!({"starting_at": "2026-11-01T00:00:00Z", "ending_at": "2026-11-02T00:00:00Z", "group": {"actor": actor}, "cost": {"amount": cents}})
}

#[test]
fn actors_are_summed_across_pages_and_sorted_by_cost() {
    let p1 = json!({
        "data": [
            actor_row(json!({"type": "user", "user_id": "u1", "email_address": "a@x.test"}), "100"),
            actor_row(json!({"type": "user", "user_id": "u2", "email_address": "b@x.test"}), "50"),
            {"starting_at": "2026-11-01T00:00:00Z", "ending_at": "2026-11-02T00:00:00Z", "group": {"model": "m"}, "cost": {"amount": "999"}},
            actor_row(json!("not an object"), "999")
        ],
        "has_more": true,
        "next_page": "p2"
    })
    .to_string();
    let p2 = json!({
        "data": [
            actor_row(json!({"type": "user", "user_id": "u1", "email_address": "a@x.test"}), "300"),
            actor_row(json!({"type": "api_key", "api_key_id": "k1", "name": "ci"}), "200")
        ],
        "has_more": false
    })
    .to_string();
    let pages = Pages::new(vec![(200, &p1), (200, &p2)]);
    let actors = client(&pages).actors(ts("2026-11-25T00:00:00Z")).unwrap();
    assert_eq!(
        actors,
        [
            ApiActor { id: "u1".into(), kind: "user".into(), display: "a@x.test".into(), cost: 4.0 },
            ApiActor { id: "k1".into(), kind: "api_key".into(), display: "ci".into(), cost: 2.0 },
            ApiActor { id: "u2".into(), kind: "user".into(), display: "b@x.test".into(), cost: 0.5 },
        ]
    );
    let calls = pages.calls();
    assert!(calls[0].url.contains("starting_at=2026-10-26T00%3A00%3A00Z&ending_at=2026-11-25T00%3A00%3A00Z&group_by%5B%5D=actor&limit=100"), "{}", calls[0].url);
    assert!(!calls[0].url.contains("page="));
    assert!(calls[1].url.ends_with("&page=p2"));
}

#[test]
fn actors_pass_errors_through() {
    let pages = Pages::new(vec![(403, r#"{"error":{"message":"no"}}"#)]);
    assert_eq!(client(&pages).actors(ts("2026-11-25T00:00:00Z")), Err(ApiError::Http(403, "no".into())));
}

#[test]
fn actors_stop_after_500_pages() {
    let t = Repeat::new(200, r#"{"data":[],"has_more":true,"next_page":"again"}"#);
    let c = AnalyticsClient::with_transport("k", Box::new(t.clone()));
    assert_eq!(c.actors(ts("2026-11-25T00:00:00Z")), Err(ApiError::Decoding("too many pages".into())));
    assert_eq!(t.urls().len(), 500);
}

#[test]
fn actor_id_fallbacks() {
    let id = |v: serde_json::Value| actor_from(Some(&v)).map(|a| a.id);
    assert_eq!(id(json!({"user_id": "u", "service_account_id": "s"})), Some("u".into()));
    assert_eq!(id(json!({"service_account_id": "s", "api_key_id": "k"})), Some("s".into()));
    assert_eq!(id(json!({"api_key_id": "k", "id": "i"})), Some("k".into()));
    assert_eq!(id(json!({"id": "i", "email_address": "e@x.test"})), Some("i".into()));
    assert_eq!(id(json!({"email_address": "e@x.test"})), Some("e@x.test".into()));
    assert_eq!(id(json!({"type": "user"})), None);
    assert_eq!(id(json!({"user_id": 7})), None);
}

#[test]
fn actor_display_fallbacks() {
    let display = |v: serde_json::Value| actor_from(Some(&v)).unwrap().display;
    assert_eq!(display(json!({"id": "i", "email_address": "a@x.test", "email": "b@x.test", "name": "n"})), "a@x.test");
    assert_eq!(display(json!({"id": "i", "email": "b@x.test", "name": "n"})), "b@x.test");
    assert_eq!(display(json!({"id": "i", "name": "n"})), "n");
    assert_eq!(display(json!({"id": "i"})), "i");
}

#[test]
fn actor_kind_defaults_to_unknown() {
    let a = actor_from(Some(&json!({"id": "i"}))).unwrap();
    assert_eq!((a.kind.as_str(), a.cost), ("unknown", 0.0));
}

#[test]
fn non_object_actors_are_ignored() {
    assert_eq!(actor_from(None), None);
    assert_eq!(actor_from(Some(&json!("u1"))), None);
    assert_eq!(actor_from(Some(&json!(null))), None);
    assert_eq!(actor_from(Some(&json!(["u1"]))), None);
}

// MARK: Statuses

#[test]
fn status_for_every_error() {
    assert_eq!(status_for(&ApiError::Http(401, "m".into())), ConnectionStatus::Unauthorized("m".into()));
    assert_eq!(status_for(&ApiError::Http(403, "m".into())), ConnectionStatus::Forbidden("m".into()));
    assert_eq!(status_for(&ApiError::Http(404, "m".into())), ConnectionStatus::Unavailable("m".into()));
    assert_eq!(status_for(&ApiError::Http(404, String::new())), ConnectionStatus::Unavailable("(404)".into()));
    assert_eq!(status_for(&ApiError::Http(500, "oops".into())), ConnectionStatus::Failed("HTTP 500 oops".into()));
    assert_eq!(status_for(&ApiError::CursorExpired), ConnectionStatus::Failed("The API kept changing while paging; try again.".into()));
    assert_eq!(status_for(&ApiError::Transport("offline".into())), ConnectionStatus::Failed("offline".into()));
    assert_eq!(status_for(&ApiError::Decoding("short".into())), ConnectionStatus::Failed("Unexpected response: short".into()));
}

#[test]
fn decoding_errors_are_truncated_to_160_bytes() {
    let ConnectionStatus::Failed(m) = status_for(&ApiError::Decoding("y".repeat(500))) else { panic!() };
    assert_eq!(m, format!("Unexpected response: {}", "y".repeat(160)));
}

#[test]
fn only_connected_and_no_data_are_usable() {
    assert!(ConnectionStatus::Connected { refreshed_at: None }.is_usable());
    assert!(ConnectionStatus::NoDataYet.is_usable());
    for s in [
        ConnectionStatus::NotConfigured,
        ConnectionStatus::Unauthorized(String::new()),
        ConnectionStatus::Forbidden(String::new()),
        ConnectionStatus::Unavailable(String::new()),
        ConnectionStatus::Failed(String::new()),
    ] {
        assert!(!s.is_usable(), "{s:?}");
    }
}

#[test]
fn status_messages() {
    let cal = Cal::utc();
    let m = |s: ConnectionStatus| s.message(&cal);
    assert_eq!(m(ConnectionStatus::NotConfigured), "Not connected. Using local logs.");
    assert_eq!(m(ConnectionStatus::Connected { refreshed_at: Some(ts("2026-11-24T16:02:11Z")) }), "Connected · data through Nov 24, 4:02 PM");
    assert_eq!(m(ConnectionStatus::Connected { refreshed_at: Some(ts("2026-11-05T09:07:00Z")) }), "Connected · data through Nov 5, 9:07 AM");
    assert_eq!(m(ConnectionStatus::Connected { refreshed_at: None }), "Connected");
    assert_eq!(m(ConnectionStatus::NoDataYet), "Connected, but the API has no data for this period yet (it can lag up to 24 hours).");
    assert_eq!(
        m(ConnectionStatus::Unauthorized("x".into())),
        "Key rejected (401). Check that you pasted an Admin key or a scoped key with read:analytics. x"
    );
    assert_eq!(
        m(ConnectionStatus::Forbidden("x".into())),
        "Access denied (403). This key or your organization isn't allowed to read usage analytics. x"
    );
    assert_eq!(m(ConnectionStatus::Unavailable("x".into())), "Usage analytics aren't available to this organization. x");
    assert_eq!(m(ConnectionStatus::Failed("x".into())), "Couldn't reach the API: x");
}

#[test]
fn status_serde_shape() {
    let v = |s: &ConnectionStatus| serde_json::to_value(s).unwrap();
    assert_eq!(v(&ConnectionStatus::NotConfigured), json!({"state": "notConfigured"}));
    assert_eq!(v(&ConnectionStatus::NoDataYet), json!({"state": "noDataYet"}));
    assert_eq!(
        v(&ConnectionStatus::Connected { refreshed_at: Some(ts("2026-11-24T16:02:11Z")) }),
        json!({"state": "connected", "detail": {"refreshed_at": "2026-11-24T16:02:11Z"}})
    );
    assert_eq!(v(&ConnectionStatus::Connected { refreshed_at: None }), json!({"state": "connected", "detail": {"refreshed_at": null}}));
    assert_eq!(v(&ConnectionStatus::Unauthorized("m".into())), json!({"state": "unauthorized", "detail": "m"}));
    assert_eq!(v(&ConnectionStatus::Forbidden("m".into())), json!({"state": "forbidden", "detail": "m"}));
    assert_eq!(v(&ConnectionStatus::Unavailable("m".into())), json!({"state": "unavailable", "detail": "m"}));
    assert_eq!(v(&ConnectionStatus::Failed("m".into())), json!({"state": "failed", "detail": "m"}));
    let all = [
        ConnectionStatus::NotConfigured,
        ConnectionStatus::Connected { refreshed_at: Some(ts("2026-11-24T16:02:11Z")) },
        ConnectionStatus::NoDataYet,
        ConnectionStatus::Failed("m".into()),
    ];
    for s in all {
        assert_eq!(serde_json::from_value::<ConnectionStatus>(v(&s)).unwrap(), s);
    }
}

#[test]
fn api_error_display() {
    assert_eq!(ApiError::Http(500, "m".into()).to_string(), "HTTP 500 m");
    assert_eq!(ApiError::CursorExpired.to_string(), "cursor expired");
    assert_eq!(ApiError::Decoding("m".into()).to_string(), "unexpected response: m");
    assert_eq!(ApiError::Transport("m".into()).to_string(), "m");
}

#[test]
fn request_headers_are_matched_case_insensitively() {
    let r = HttpRequest { url: String::new(), headers: vec![("X-Api-Key".into(), "k".into())] };
    assert_eq!(r.header("x-api-key"), Some("k"));
    assert_eq!(r.header("X-API-KEY"), Some("k"));
    assert_eq!(r.header("anthropic-beta"), None);
}

#[test]
fn coverage_is_fetched_after_the_range_ends() {
    let pages = Pages::new(vec![(200, EMPTY)]);
    let data = client(&pages).hourly_cost(ts("2026-03-01T00:00:00Z"), ts("2026-03-02T00:00:00Z"), None).unwrap();
    assert!(data.fetched_at > data.coverage_end);
}

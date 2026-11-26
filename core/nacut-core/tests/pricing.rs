//! Model-id normalization, price lookup, long-context tiers, overrides and cost arithmetic.

use std::collections::HashMap;

use nacut_core::pricing::{ModelPrice, PriceTable, BUNDLED_PRICING_JSON};
use nacut_core::{PriceBook, Rates, UsageRecord};

mod common;

use common::record;

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

fn model(id: &str, family: &str, rank: i32, input: f64) -> ModelPrice {
    ModelPrice {
        id: id.into(),
        name: id.into(),
        family: family.into(),
        rank,
        input,
        output: 0.0,
        write5m: 0.0,
        write1h: 0.0,
        read: 0.0,
        long_context: None,
    }
}

fn table(models: Vec<ModelPrice>) -> PriceTable {
    PriceTable { updated: "2026-01-01".into(), web_search_per_thousand: 10.0, fast_multiplier: 2.0, models }
}

fn rec(model: &str) -> UsageRecord {
    record("x", "2026-11-20T01:00:00Z", model, 1_000_000, 0)
}

// MARK: Normalization

#[test]
fn vertex_ids_drop_the_at_date() {
    assert_eq!(PriceBook::normalize("claude-opus-5-5@20261001"), "claude-opus-5-5");
}

#[test]
fn regional_bedrock_prefixes_are_stripped() {
    assert_eq!(PriceBook::normalize("eu.anthropic.claude-sonnet-5-5"), "claude-sonnet-5-5");
    assert_eq!(PriceBook::normalize("global.anthropic.claude-sonnet-5-5"), "claude-sonnet-5-5");
    assert_eq!(PriceBook::normalize("us.anthropic.claude-sonnet-5-5"), "claude-sonnet-5-5");
}

#[test]
fn ids_are_lower_cased() {
    assert_eq!(PriceBook::normalize("Claude-Opus-5-5-20261001"), "claude-opus-5-5");
}

#[test]
fn a_v_without_digits_is_not_a_version_tag() {
    assert_eq!(PriceBook::normalize("claude-opus-5-5-vx"), "claude-opus-5-5-vx");
    assert_eq!(PriceBook::normalize("claude-opus-5-5-v"), "claude-opus-5-5-v");
}

#[test]
fn a_version_tag_with_an_empty_minor_is_kept() {
    assert_eq!(PriceBook::normalize("claude-opus-5-5-v1:"), "claude-opus-5-5-v1:");
}

#[test]
fn version_tag_without_minor_is_stripped() {
    assert_eq!(PriceBook::normalize("claude-opus-5-5-v2"), "claude-opus-5-5");
}

#[test]
fn short_numeric_suffixes_are_not_dates() {
    assert_eq!(PriceBook::normalize("claude-opus-5-5-2026"), "claude-opus-5-5-2026");
}

#[test]
fn family_finds_the_first_matching_family() {
    assert_eq!(PriceBook::family("claude-opus-9"), Some("opus"));
    assert_eq!(PriceBook::family("claude-fable-opus"), Some("fable"));
    assert_eq!(PriceBook::family("gpt-5"), None);
}

// MARK: Lookup

#[test]
fn unknown_family_has_no_price_and_is_estimated() {
    let (p, estimated) = PriceBook::bundled().price("gpt-5");
    assert!(p.is_none());
    assert!(estimated);
}

#[test]
fn unknown_family_costs_nothing_but_web_searches() {
    let book = PriceBook::bundled();
    let mut r = rec("gpt-5");
    assert_eq!(book.rates(&r), (Rates::default(), true));
    assert_eq!(book.cost(&r), (0.0, true));
    r.web_searches = 4;
    close(book.cost(&r).0, 0.04);
}

#[test]
fn unknown_version_falls_back_to_the_highest_ranked_family_member() {
    let book = PriceBook::new(table(vec![model("claude-opus-1", "opus", 1, 1.0), model("claude-opus-3", "opus", 3, 3.0), model("claude-opus-2", "opus", 2, 2.0)]), HashMap::new());
    let (p, estimated) = book.price("claude-opus-9");
    assert_eq!(p.unwrap().id, "claude-opus-3");
    assert!(estimated);
}

#[test]
fn duplicate_ids_resolve_to_the_first_entry() {
    let book = PriceBook::new(table(vec![model("claude-opus-1", "opus", 1, 1.0), model("claude-opus-1", "opus", 1, 9.0)]), HashMap::new());
    assert_eq!(book.price("claude-opus-1").0.unwrap().input, 1.0);
    close(book.cost(&rec("claude-opus-1")).0, 1.0);
}

#[test]
fn bundled_table_parses_and_every_family_has_a_model() {
    let t: PriceTable = serde_json::from_str(BUNDLED_PRICING_JSON).unwrap();
    assert_eq!(&t, &PriceBook::bundled().table);
    assert!(!t.updated.is_empty());
    for fam in ["fable", "opus", "sonnet", "haiku"] {
        assert!(t.models.iter().any(|m| m.family == fam), "{fam}");
    }
    for m in &t.models {
        assert_eq!(PriceBook::normalize(&m.id), m.id, "ids are stored normalized");
        assert_eq!(PriceBook::family(&m.id), Some(m.family.as_str()));
    }
}

// MARK: Display names

#[test]
fn known_models_use_the_table_name() {
    assert_eq!(PriceBook::bundled().display_name("us.anthropic.claude-haiku-5-5-v1:0"), "Haiku 5.5");
}

#[test]
fn family_only_ids_show_just_the_family() {
    assert_eq!(PriceBook::bundled().display_name("claude-opus"), "Opus");
}

#[test]
fn empty_model_name_stays_empty() {
    assert_eq!(PriceBook::bundled().display_name(""), "");
}

#[test]
fn non_claude_models_are_title_cased_with_dotted_versions() {
    assert_eq!(PriceBook::bundled().display_name("gpt-4-1"), "Gpt 4.1");
}

// MARK: Long context

#[test]
fn long_context_tier_starts_strictly_above_the_threshold() {
    let book = PriceBook::bundled();
    let at = record("x", "2026-11-20T01:00:00Z", "claude-haiku-5-5", 100_000, 0);
    let above = record("x", "2026-11-20T01:00:00Z", "claude-haiku-5-5", 100_001, 0);
    assert_eq!(book.rates(&at).0.input, 0.1);
    assert_eq!(book.rates(&above).0.input, 0.5);
}

#[test]
fn long_context_counts_cache_reads_and_writes_but_not_output() {
    let book = PriceBook::bundled();
    let mut r = record("x", "2026-11-20T01:00:00Z", "claude-haiku-5-5", 1, 1_000_000);
    assert_eq!(book.rates(&r).0.output, 0.5);
    r.cache_read = 60_000;
    r.cache_write_1h = 40_000;
    assert_eq!(book.rates(&r).0.output, 2.5);
}

// MARK: Overrides

#[test]
fn override_for_an_unknown_model_is_not_estimated() {
    let rates = Rates { input: 3.0, output: 0.0, write5m: 0.0, write1h: 0.0, read: 0.0 };
    let book = PriceBook::bundled().with_overrides(HashMap::from([("acme-1".to_string(), rates)]));
    assert_eq!(book.cost(&rec("acme-1")), (3.0, false));
}

#[test]
fn override_keys_must_be_normalized_ids() {
    let rates = Rates { input: 3.0, output: 0.0, write5m: 0.0, write1h: 0.0, read: 0.0 };
    let book = PriceBook::bundled().with_overrides(HashMap::from([("claude-opus-5-5-20261001".to_string(), rates)]));
    // The key isn't normalized, so the bundled $4 price applies.
    close(book.cost(&rec("claude-opus-5-5-20261001")).0, 4.0);
    let book = PriceBook::bundled().with_overrides(HashMap::from([("claude-opus-5-5".to_string(), rates)]));
    close(book.cost(&rec("claude-opus-5-5-20261001")).0, 3.0);
}

#[test]
fn override_replaces_long_context_rates_too() {
    let rates = Rates { input: 1.0, output: 0.0, write5m: 0.0, write1h: 0.0, read: 0.0 };
    let book = PriceBook::bundled().with_overrides(HashMap::from([("claude-haiku-5-5".to_string(), rates)]));
    assert_eq!(book.rates(&record("x", "2026-11-20T01:00:00Z", "claude-haiku-5-5", 200_000, 0)), (rates, false));
}

// MARK: Cost

#[test]
fn every_token_class_is_priced() {
    let mut r = record("x", "2026-11-20T01:00:00Z", "claude-opus-5-5", 1_000_000, 1_000_000);
    r.cache_write_5m = 1_000_000;
    r.cache_write_1h = 1_000_000;
    r.cache_read = 1_000_000;
    // 4 + 20 + 5 + 8 + 0.2
    let (cost, estimated) = PriceBook::bundled().cost(&r);
    close(cost, 37.2);
    assert!(!estimated);
}

#[test]
fn fast_multiplier_applies_to_tokens_not_web_searches() {
    let mut r = rec("claude-opus-5-5");
    r.fast = true;
    r.web_searches = 1000;
    // $4 × 2 + 1000 searches at $10 per thousand.
    close(PriceBook::bundled().cost(&r).0, 18.0);
}

#[test]
fn cache_savings_are_reads_at_input_minus_read_price() {
    let mut r = record("x", "2026-11-20T01:00:00Z", "claude-opus-5-5", 0, 0);
    r.cache_read = 1_000_000;
    close(PriceBook::bundled().cache_savings(&r), 3.8);
}

#[test]
fn cache_savings_scale_with_fast_mode() {
    let mut r = record("x", "2026-11-20T01:00:00Z", "claude-opus-5-5", 0, 0);
    r.cache_read = 1_000_000;
    r.fast = true;
    close(PriceBook::bundled().cache_savings(&r), 7.6);
}

#[test]
fn cache_savings_go_negative_when_reads_cost_more_than_input() {
    let rates = Rates { input: 1.0, output: 0.0, write5m: 0.0, write1h: 0.0, read: 3.0 };
    let book = PriceBook::bundled().with_overrides(HashMap::from([("claude-opus-5-5".to_string(), rates)]));
    let mut r = record("x", "2026-11-20T01:00:00Z", "claude-opus-5-5", 0, 0);
    r.cache_read = 1_000_000;
    close(book.cache_savings(&r), -2.0);
}

#[test]
fn price_all_sets_cost_and_estimated() {
    let mut records = vec![rec("claude-opus-5-5"), rec("claude-opus-9-9")];
    PriceBook::bundled().price_all(&mut records);
    assert_eq!((records[0].cost, records[0].estimated), (4.0, false));
    assert_eq!((records[1].cost, records[1].estimated), (4.0, true));
}

#[test]
fn rates_serialize_camel_case() {
    let r = Rates { input: 1.0, output: 2.0, write5m: 3.0, write1h: 4.0, read: 5.0 };
    let v = serde_json::to_value(r).unwrap();
    assert_eq!(v, serde_json::json!({"input":1.0,"output":2.0,"write5m":3.0,"write1h":4.0,"read":5.0}));
}

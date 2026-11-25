//! Turns token counts into USD at Anthropic list prices.
//! Port of `PriceBook.swift`; the price list is the same `Pricing.json` the Mac app ships.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::record::{capitalize_words, UsageRecord};

/// The bundled price list (shared with the Mac app).
pub const BUNDLED_PRICING_JSON: &str = include_str!("../../../shared/Pricing.json");

/// USD per million tokens.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Rates {
    pub input: f64,
    pub output: f64,
    pub write5m: f64,
    pub write1h: f64,
    pub read: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LongContextRates {
    pub threshold: u64,
    pub input: f64,
    pub output: f64,
    pub write5m: f64,
    pub write1h: f64,
    pub read: f64,
}

impl LongContextRates {
    fn rates(&self) -> Rates {
        Rates { input: self.input, output: self.output, write5m: self.write5m, write1h: self.write1h, read: self.read }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    pub id: String,
    pub name: String,
    pub family: String,
    pub rank: i32,
    pub input: f64,
    pub output: f64,
    pub write5m: f64,
    pub write1h: f64,
    pub read: f64,
    #[serde(default)]
    pub long_context: Option<LongContextRates>,
}

impl ModelPrice {
    pub fn rates(&self) -> Rates {
        Rates { input: self.input, output: self.output, write5m: self.write5m, write1h: self.write1h, read: self.read }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceTable {
    pub updated: String,
    pub web_search_per_thousand: f64,
    pub fast_multiplier: f64,
    pub models: Vec<ModelPrice>,
}

#[derive(Debug, Clone)]
pub struct PriceBook {
    pub table: PriceTable,
    by_id: HashMap<String, usize>,
    /// User overrides keyed by normalized model id.
    pub overrides: HashMap<String, Rates>,
}

impl PriceBook {
    pub fn new(table: PriceTable, overrides: HashMap<String, Rates>) -> Self {
        let mut by_id = HashMap::new();
        for (i, m) in table.models.iter().enumerate() {
            by_id.entry(m.id.clone()).or_insert(i);
        }
        Self { table, by_id, overrides }
    }

    pub fn bundled() -> &'static PriceBook {
        static BOOK: OnceLock<PriceBook> = OnceLock::new();
        BOOK.get_or_init(|| {
            let table: PriceTable = serde_json::from_str(BUNDLED_PRICING_JSON).expect("shared/Pricing.json is valid");
            PriceBook::new(table, HashMap::new())
        })
    }

    pub fn with_overrides(&self, overrides: HashMap<String, Rates>) -> PriceBook {
        let mut copy = self.clone();
        copy.overrides = overrides;
        copy
    }

    /// `claude-opus-5-5-20260401[1m]` → `claude-opus-5-5`; Mythos maps to the matching Fable.
    pub fn normalize(model: &str) -> String {
        let mut m = model.to_lowercase();
        if let Some(i) = m.find('[') {
            m.truncate(i);
        }
        for prefix in ["anthropic.", "us.anthropic.", "eu.anthropic.", "global.anthropic."] {
            if let Some(rest) = m.strip_prefix(prefix) {
                m = rest.to_string();
            }
        }
        if let Some(i) = m.find('@') {
            m.truncate(i);
        }
        // Strip a trailing date (-20250101) or version tag (-v1:0).
        if let Some(stripped) = strip_date_suffix(&m) {
            m = stripped;
        }
        if let Some(stripped) = strip_version_suffix(&m) {
            m = stripped;
        }
        m.replace("claude-mythos-", "claude-fable-")
    }

    pub fn family(normalized: &str) -> Option<&'static str> {
        ["fable", "opus", "sonnet", "haiku"].into_iter().find(|f| normalized.contains(f))
    }

    /// Price entry for a model, and whether it was guessed from the family.
    pub fn price(&self, model: &str) -> (Option<&ModelPrice>, bool) {
        let n = Self::normalize(model);
        if let Some(&i) = self.by_id.get(&n) {
            return (Some(&self.table.models[i]), false);
        }
        if let Some(fam) = Self::family(&n) {
            let best = self.table.models.iter().filter(|m| m.family == fam).max_by_key(|m| m.rank);
            return (best, true);
        }
        (None, true)
    }

    /// `claude-opus-7-1` → `Opus 7.1` when the model isn't in the price list.
    pub fn display_name(&self, model: &str) -> String {
        let n = Self::normalize(model);
        if let Some(&i) = self.by_id.get(&n) {
            return self.table.models[i].name.clone();
        }
        let stripped = n.replace("claude-", "");
        let parts: Vec<&str> = stripped.split('-').filter(|s| !s.is_empty()).collect();
        let Some(fam) = parts.first() else { return model.to_string() };
        let version = parts[1..].join(".");
        if version.is_empty() {
            capitalize_words(fam)
        } else {
            format!("{} {}", capitalize_words(fam), version)
        }
    }

    pub fn rates(&self, record: &UsageRecord) -> (Rates, bool) {
        let n = Self::normalize(&record.model);
        let (p, estimated) = self.price(&record.model);
        if let Some(o) = self.overrides.get(&n) {
            return (*o, false);
        }
        let Some(p) = p else { return (Rates::default(), true) };
        if let Some(lc) = p.long_context {
            if record.prompt_tokens() > lc.threshold {
                return (lc.rates(), estimated);
            }
        }
        (p.rates(), estimated)
    }

    pub fn cost(&self, record: &UsageRecord) -> (f64, bool) {
        let (r, est) = self.rates(record);
        let mut c = record.input as f64 * r.input
            + record.output as f64 * r.output
            + record.cache_write_5m as f64 * r.write5m
            + record.cache_write_1h as f64 * r.write1h
            + record.cache_read as f64 * r.read;
        c /= 1_000_000.0;
        if record.fast {
            c *= self.table.fast_multiplier;
        }
        c += record.web_searches as f64 * self.table.web_search_per_thousand / 1000.0;
        (c, est)
    }

    /// What cache reads would have cost at the full input price, minus what they did cost.
    pub fn cache_savings(&self, record: &UsageRecord) -> f64 {
        let (r, _) = self.rates(record);
        let mult = if record.fast { self.table.fast_multiplier } else { 1.0 };
        record.cache_read as f64 * (r.input - r.read) / 1_000_000.0 * mult
    }

    /// Sets `cost` and `estimated` on each record.
    pub fn price_all(&self, records: &mut [UsageRecord]) {
        for r in records.iter_mut() {
            let (c, e) = self.cost(r);
            r.cost = c;
            r.estimated = e;
        }
    }
}

/// `-20250101` at the end.
fn strip_date_suffix(m: &str) -> Option<String> {
    let b = m.as_bytes();
    if b.len() >= 9 && b[b.len() - 9] == b'-' && b[b.len() - 8..].iter().all(u8::is_ascii_digit) {
        return Some(m[..m.len() - 9].to_string());
    }
    None
}

/// `-v1` or `-v1:0` at the end.
fn strip_version_suffix(m: &str) -> Option<String> {
    let i = m.rfind("-v")?;
    let tail = &m[i + 2..];
    let (major, minor) = match tail.split_once(':') {
        Some((a, b)) => (a, Some(b)),
        None => (tail, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
    if digits(major) && minor.is_none_or(digits) {
        Some(m[..i].to_string())
    } else {
        None
    }
}

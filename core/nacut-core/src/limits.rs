//! Max/Pro plan limit windows (5-hour session, weekly, per-model weekly).
//! Port of `PlanLimits.swift` plus the projection helpers from `LimitsService.swift`.

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::scanner::parse_date;
use crate::store::Cal;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanLimit {
    pub id: String,
    pub label: String,
    /// 0…1 (can exceed 1 slightly when over the limit).
    pub utilization: f64,
    pub resets_at: Option<Timestamp>,
}

impl PlanLimit {
    /// Parses the subscription usage response (`GET /api/oauth/usage`).
    /// Prefers the `limits` array (it includes per-model weekly limits); falls back to
    /// top-level objects that have a numeric `utilization` percentage.
    pub fn parse(data: &[u8]) -> Vec<PlanLimit> {
        let Ok(Value::Object(obj)) = serde_json::from_slice::<Value>(data) else { return Vec::new() };
        if let Some(Value::Array(items)) = obj.get("limits") {
            let from_list = Self::parse_list(items);
            if !from_list.is_empty() {
                return from_list;
            }
        }
        let mut out = Vec::new();
        for (key, value) in &obj {
            if !value.is_object() {
                continue;
            }
            let Some(util) = number(value.get("utilization")) else { continue };
            let resets = value.get("resets_at").and_then(Value::as_str).and_then(parse_date);
            out.push(PlanLimit { id: key.clone(), label: label_for(key), utilization: (util / 100.0).clamp(0.0, 1.5), resets_at: resets });
        }
        sort(&mut out);
        out
    }

    fn parse_list(items: &[Value]) -> Vec<PlanLimit> {
        let mut out: Vec<PlanLimit> = Vec::new();
        for item in items {
            let (Some(kind), Some(pct)) = (item.get("kind").and_then(Value::as_str), number(item.get("percent"))) else { continue };
            let resets = item.get("resets_at").and_then(Value::as_str).and_then(parse_date);
            let scope = item.get("scope");
            let scope_name = scope
                .and_then(|s| s.get("model"))
                .and_then(|m| m.get("display_name"))
                .and_then(Value::as_str)
                .or_else(|| scope.and_then(|s| s.get("surface")).and_then(Value::as_str));
            let (id, label) = match (kind, scope_name) {
                ("session", _) => ("five_hour".to_string(), label_for("five_hour")),
                ("weekly_all", _) => ("seven_day".to_string(), label_for("seven_day")),
                ("weekly_scoped", Some(name)) => {
                    (format!("seven_day_{}", name.to_lowercase().replace(' ', "_")), format!("Weekly · {name}"))
                }
                (other, name) => {
                    let id = match name {
                        Some(n) => format!("{other}_{}", n.to_lowercase()),
                        None => other.to_string(),
                    };
                    let label = label_for(&id);
                    (id, label)
                }
            };
            if out.iter().any(|l| l.id == id) {
                continue;
            }
            out.push(PlanLimit { id, label, utilization: (pct / 100.0).clamp(0.0, 1.5), resets_at: resets });
        }
        sort(&mut out);
        out
    }

    /// `5h`, `Week`, `Fable`.
    pub fn short_label(&self) -> String {
        match self.id.as_str() {
            "five_hour" => "5h".into(),
            "seven_day" => "Week".into(),
            _ => self.label.replace("Weekly · ", ""),
        }
    }

    /// Utilization as of `now`: zero once the window has reset, even before the next fetch.
    pub fn effective_utilization(&self, now: Timestamp) -> f64 {
        match self.resets_at {
            Some(r) if r <= now => 0.0,
            _ => self.utilization,
        }
    }

    /// Window length implied by the limit id.
    pub fn window(&self) -> SignedDuration {
        if self.id == "five_hour" { SignedDuration::from_hours(5) } else { SignedDuration::from_hours(7 * 24) }
    }

    /// Utilization projected to the reset time at the current pace, or `None` if too early to tell.
    pub fn projected_at_reset(&self, now: Timestamp) -> Option<f64> {
        let resets = self.resets_at?;
        let window = self.window().as_secs_f64();
        let start = resets.checked_sub(self.window()).ok()?;
        let elapsed = now.duration_since(start).as_secs_f64() / window;
        (elapsed > 0.1 && elapsed < 1.0).then(|| self.utilization / elapsed)
    }

    /// When the limit would be hit at the current pace, if before reset.
    pub fn projected_hit(&self, now: Timestamp) -> Option<Timestamp> {
        let resets = self.resets_at?;
        if !(self.utilization > 0.0 && self.utilization < 1.0) {
            return None;
        }
        let window = self.window().as_secs_f64();
        let start = resets.checked_sub(self.window()).ok()?;
        let elapsed = now.duration_since(start).as_secs_f64();
        if elapsed <= window * 0.1 {
            return None;
        }
        let rate = self.utilization / elapsed;
        let hit = now.checked_add(SignedDuration::from_secs_f64((1.0 - self.utilization) / rate)).ok()?;
        (hit < resets).then_some(hit)
    }

    /// `resets in 4h 35m`, `resets in 12m`, or `resets Thu 7 PM` when a day or more away.
    pub fn reset_text(&self, now: Timestamp, cal: &Cal) -> String {
        let Some(r) = self.resets_at else { return String::new() };
        let s = r.duration_since(now).as_secs_f64().max(0.0) as i64;
        let (h, m) = (s / 3600, (s % 3600) / 60);
        if h >= 24 {
            return format!("resets {}", r.to_zoned(cal.tz.clone()).strftime("%a %-I %p"));
        }
        if h > 0 { format!("resets in {h}h {m}m") } else { format!("resets in {m}m") }
    }
}

fn number(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

pub fn label_for(key: &str) -> String {
    match key {
        "five_hour" => "5-hour session".into(),
        "seven_day" => "Weekly · all models".into(),
        "seven_day_opus" => "Weekly · Opus".into(),
        "seven_day_sonnet" => "Weekly · Sonnet".into(),
        "seven_day_fable" => "Weekly · Fable".into(),
        "seven_day_oauth_apps" => "Weekly · apps".into(),
        _ => key
            .split('_')
            .filter(|w| !w.is_empty())
            .map(crate::record::capitalize_words)
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn order(key: &str) -> u8 {
    match key {
        "five_hour" => 0,
        "seven_day" => 1,
        _ => 2,
    }
}

fn sort(v: &mut [PlanLimit]) {
    v.sort_by(|a, b| (order(&a.id), &a.id).cmp(&(order(&b.id), &b.id)));
}

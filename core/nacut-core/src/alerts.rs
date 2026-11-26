//! Budget and plan-limit alerts, each fired at most once per cycle or limit window.
//! Port of `AlertService.swift`'s decisions; each app posts the notification its own way.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::forecast::{BudgetStatus, CurrencyFormat, Forecast};
use crate::limits::PlanLimit;
use crate::settings::Settings;
use crate::store::Cal;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Alert {
    /// Stable id: the same alert never fires twice.
    pub id: String,
    pub title: String,
    pub body: String,
}

/// Remembers which alerts have fired, in `alerts-fired.json`.
#[derive(Debug, Default)]
pub struct AlertLedger {
    path: Option<PathBuf>,
    fired: BTreeSet<String>,
}

impl AlertLedger {
    pub fn load(path: &Path) -> Self {
        let fired = std::fs::read(path).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
        Self { path: Some(path.to_path_buf()), fired }
    }

    pub fn in_memory() -> Self {
        Self::default()
    }

    fn fire_once(&mut self, out: &mut Vec<Alert>, id: String, title: String, body: String) {
        if self.fired.insert(id.clone()) {
            out.push(Alert { id, title, body });
        }
    }

    fn save(&self) {
        if let Some(p) = &self.path {
            if let Ok(d) = serde_json::to_vec(&self.fired) {
                let _ = crate::platform::write_private(p, &d);
            }
        }
    }

    /// Alerts that should be shown now.
    pub fn evaluate(&mut self, f: &Forecast, limits: &[PlanLimit], settings: &Settings, now: Timestamp, cal: &Cal) -> Vec<Alert> {
        let mut out = Vec::new();
        if !settings.alerts_enabled {
            return out;
        }
        let cycle_key = f.cycle.start.to_string();
        let noun = if settings.is_subscription() { "API-equivalent usage" } else { "spend" };
        let usd = |v| CurrencyFormat::string(v, false);

        if f.budget > 0.0 {
            for &t in &settings.alert_thresholds {
                // Compared without dividing: 290 / 1000 × 100 is 28.999999999999996, which would miss 29%.
                if f.spent * 100.0 >= t as f64 * f.budget {
                    let title = if t >= 100 { "Claude budget reached".to_string() } else { format!("Claude budget {t}% used") };
                    let body = format!("{} of {} {noun} this cycle.", usd(f.spent), usd(f.budget));
                    self.fire_once(&mut out, format!("budget-{cycle_key}-{t}"), title, body);
                }
            }
            if settings.forecast_alert && f.status == BudgetStatus::WillExceed {
                if let Some(d) = f.exhaustion_date {
                    let when = d.to_zoned(cal.tz.clone()).strftime("%A, %B %-d").to_string();
                    let body = format!(
                        "At the current rate you'll hit {} around {when}. Projected: {}.",
                        usd(f.budget),
                        usd(f.projected)
                    );
                    self.fire_once(&mut out, format!("forecast-{cycle_key}"), "On pace to exceed your Claude budget".into(), body);
                }
            }
        }

        if settings.limit_alerts && settings.limits_enabled {
            for l in limits.iter().filter(|l| l.effective_utilization(now) >= 0.9) {
                let window = l.resets_at.map(|r| r.to_string()).unwrap_or_else(|| "none".into());
                let reset = l.reset_text(now, cal);
                let body = if reset.is_empty() { "You're close to your plan limit.".to_string() } else { format!("Plan limit {reset}.") };
                let title = format!("{} limit at {}", l.label, crate::views::percent(l.utilization));
                self.fire_once(&mut out, format!("limit-{}-{window}", l.id), title, body);
            }
        }
        if !out.is_empty() {
            self.save();
        }
        out
    }
}

pub fn test_alert() -> Alert {
    Alert {
        id: format!("test-{}", Timestamp::now().as_millisecond()),
        title: "NACUT alerts are on".into(),
        body: "You'll be notified at your budget thresholds.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::DailySeries;

    #[test]
    fn fires_once() {
        let cal = Cal::utc();
        let now: Timestamp = "2026-11-11T12:00:00Z".parse().unwrap();
        let mut s = DailySeries::new();
        s.insert(jiff::civil::date(2026, 11, 1), crate::store::DayCost::with_cost(600.0));
        let f = Forecast::new(&s, 1000.0, 1, now, &cal);
        let settings = Settings { monthly_budget: 1000.0, forecast_alert: false, ..Settings::default() };
        let mut ledger = AlertLedger::in_memory();
        let first = ledger.evaluate(&f, &[], &settings, now, &cal);
        assert_eq!(first.iter().map(|a| a.title.as_str()).collect::<Vec<_>>(), ["Claude budget 50% used"]);
        assert!(ledger.evaluate(&f, &[], &settings, now, &cal).is_empty());
    }
}

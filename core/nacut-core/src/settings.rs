//! User preferences shared by every platform. Mirrors `AppSettings.swift`.
//!
//! Windows and Linux persist this as JSON in the app's config folder ([`Settings::load`] / [`Settings::save`]);
//! the Mac keeps UserDefaults and hands the core a `Settings` value.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::merge::DataSourceMode;
use crate::pricing::Rates;
use crate::scanner::{expand_tilde, home_dir};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum AccountMode {
    /// Max / Pro subscription: costs are API-equivalent value.
    Subscription,
    /// API pay-as-you-go: costs are real spend.
    Api,
}

/// What a subscription's menu bar, panel header and widget lead with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum UsageDisplay {
    Limits,
    Cost,
}

/// The second figure in the menu bar / tray text (cost mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum SecondFigure {
    Cycle,
    AllTime,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Settings {
    pub account_mode: AccountMode,
    pub plan_price: f64,
    pub monthly_budget: f64,
    pub cycle_start_day: u8,
    pub second_figure: SecondFigure,
    pub compact_menu_bar: bool,
    pub usage_display: UsageDisplay,

    pub data_source: DataSourceMode,
    pub api_actor_id: Option<String>,
    pub api_actor_name: Option<String>,

    pub limits_enabled: bool,

    pub alerts_enabled: bool,
    pub alert_thresholds: Vec<u32>,
    pub forecast_alert: bool,
    pub limit_alerts: bool,

    pub extra_roots: Vec<String>,
    /// Windows: also read Claude Code transcripts inside WSL distributions.
    pub include_wsl: bool,
    pub price_overrides: HashMap<String, Rates>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            account_mode: AccountMode::Api,
            plan_price: 100.0,
            monthly_budget: 0.0,
            cycle_start_day: 1,
            second_figure: SecondFigure::Cycle,
            compact_menu_bar: false,
            usage_display: UsageDisplay::Limits,
            data_source: DataSourceMode::Logs,
            api_actor_id: None,
            api_actor_name: None,
            limits_enabled: false,
            alerts_enabled: true,
            alert_thresholds: vec![50, 80, 100],
            forecast_alert: true,
            limit_alerts: true,
            extra_roots: Vec::new(),
            include_wsl: true,
            price_overrides: HashMap::new(),
        }
    }
}

impl Settings {
    /// Defaults with the account mode and plan price detected from `~/.claude.json`.
    /// Subscriptions also start with plan limits on, since that's what they lead with.
    pub fn detected() -> Self {
        let mut s = Settings::default();
        if let Some((mode, price)) = detect_account(&home_dir().join(".claude.json")) {
            s.account_mode = mode;
            if let Some(p) = price {
                s.plan_price = p;
            }
            s.limits_enabled = mode == AccountMode::Subscription;
        }
        s
    }

    /// Loads settings from `path`, or detects defaults (and saves them) on first run.
    /// Accepts a UTF-8 byte-order mark (Windows editors add one). A file that can't be parsed is
    /// renamed to `*.invalid.json` rather than overwritten, so hand edits aren't lost.
    pub fn load(path: &Path) -> Self {
        let data = std::fs::read(path).ok();
        let parsed = data.as_deref().and_then(Self::parse);
        match parsed {
            Some(mut s) => {
                s.sanitize();
                s
            }
            None => {
                if data.is_some() {
                    let _ = std::fs::rename(path, path.with_extension("invalid.json"));
                }
                let s = Self::detected();
                let _ = s.save(path);
                s
            }
        }
    }

    /// Reads settings without creating or changing anything (for read-only tools like the CLI).
    pub fn read(path: &Path) -> Option<Self> {
        let mut s = Self::parse(&std::fs::read(path).ok()?)?;
        s.sanitize();
        Some(s)
    }

    /// Parses settings JSON, accepting a UTF-8 byte-order mark.
    fn parse(data: &[u8]) -> Option<Self> {
        let d = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
        serde_json::from_slice::<Settings>(d).ok()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let data = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(tmp, path)
    }

    pub fn sanitize(&mut self) {
        self.cycle_start_day = self.cycle_start_day.clamp(1, 28);
        self.monthly_budget = self.monthly_budget.max(0.0);
        self.plan_price = self.plan_price.max(0.0);
        self.alert_thresholds.retain(|t| (1..=1000).contains(t));
        self.alert_thresholds.sort_unstable();
        self.alert_thresholds.dedup();
    }

    pub fn is_subscription(&self) -> bool {
        self.account_mode == AccountMode::Subscription
    }

    /// Subscriptions lead with plan limits rather than API-equivalent cost, when the user prefers it.
    pub fn shows_limits(&self) -> bool {
        self.is_subscription() && self.usage_display == UsageDisplay::Limits && self.limits_enabled
    }

    pub fn extra_root_paths(&self) -> Vec<PathBuf> {
        self.extra_roots.iter().filter(|r| !r.trim().is_empty()).map(|r| expand_tilde(r.trim())).collect()
    }

    /// `Max 5x`, `Max 20x`, `Pro`.
    pub fn plan_name(&self) -> &'static str {
        match self.plan_price.round() as i64 {
            200 => "Max 20x",
            100 => "Max 5x",
            20 => "Pro",
            _ => "Subscription",
        }
    }
}

/// Reads `~/.claude.json` to guess whether this computer is signed in to a Max/Pro plan, and its price
/// (Max 5x → $100, Max 20x → $200, Pro → $20).
pub fn detect_account(path: &Path) -> Option<(AccountMode, Option<f64>)> {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let account = v.get("oauthAccount")?;
    let org_type = account.get("organizationType").and_then(|t| t.as_str()).unwrap_or("");
    let tier = account.get("organizationRateLimitTier").and_then(|t| t.as_str()).unwrap_or("");
    let subscription = org_type.starts_with("claude_max") || org_type.starts_with("claude_pro");
    let price = if tier.contains("20x") {
        Some(200.0)
    } else if tier.contains("5x") || org_type.starts_with("claude_max") {
        Some(100.0)
    } else if org_type.starts_with("claude_pro") {
        Some(20.0)
    } else {
        None
    };
    Some((if subscription { AccountMode::Subscription } else { AccountMode::Api }, price))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_settings_with_byte_order_mark() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"\xEF\xBB\xBF{\"monthlyBudget\":250,\"extraRoots\":[\"C:\\\\logs\"]}").unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.monthly_budget, 250.0);
        assert_eq!(s.extra_roots, vec!["C:\\logs".to_string()]);
    }

    #[test]
    fn keeps_unparseable_settings_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let _ = Settings::load(&path);
        assert_eq!(std::fs::read(dir.path().join("settings.invalid.json")).unwrap(), b"{ not json");
        assert!(serde_json::from_slice::<Settings>(&std::fs::read(&path).unwrap()).is_ok());
    }
}

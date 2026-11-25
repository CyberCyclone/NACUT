//! The same palette as the Mac and Windows apps: coral accent, a validated categorical palette in fixed
//! order, a sequential coral ramp and status colors, each with light and dark steps.

use nacut_core::forecast::BudgetStatus;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub u32);

impl Rgb {
    pub fn r(self) -> f64 {
        ((self.0 >> 16) & 0xFF) as f64 / 255.0
    }
    pub fn g(self) -> f64 {
        ((self.0 >> 8) & 0xFF) as f64 / 255.0
    }
    pub fn b(self) -> f64 {
        (self.0 & 0xFF) as f64 / 255.0
    }
    pub fn css(self) -> String {
        format!("#{:06x}", self.0)
    }
    /// Sets the cairo source to this color.
    pub fn set(self, cr: &gtk::cairo::Context, alpha: f64) {
        cr.set_source_rgba(self.r(), self.g(), self.b(), alpha);
    }
}

pub fn is_dark() -> bool {
    adw::StyleManager::default().is_dark()
}

fn pick(light: u32, dark: u32) -> Rgb {
    Rgb(if is_dark() { dark } else { light })
}

pub fn coral() -> Rgb {
    pick(0xD97757, 0xE58262)
}

/// Text color for the current theme (for cairo drawing).
pub fn ink() -> Rgb {
    pick(0x1E1E1E, 0xFFFFFF)
}

const CATEGORICAL: [(u32, u32); 8] = [
    (0x2A78D6, 0x3987E5), // blue
    (0xEB6834, 0xD95926), // orange
    (0x1BAF7A, 0x199E70), // aqua
    (0xEDA100, 0xC98500), // yellow
    (0xE87BA4, 0xD55181), // magenta
    (0x008300, 0x0CA30C), // green
    (0x6250D6, 0x9085E9), // violet
    (0xE34948, 0xE66767), // red
];

pub fn categorical(slot: usize) -> Rgb {
    let (l, d) = CATEGORICAL[slot % CATEGORICAL.len()];
    pick(l, d)
}

pub fn other() -> Rgb {
    pick(0x9A9890, 0x6E6D66)
}

/// Fixed model → slot registry so a model keeps its color across every view and filter.
const MODEL_SLOTS: [&str; 8] = [
    "claude-opus-5-5",
    "claude-fable-5-2",
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
    "claude-fable-5-1",
    "claude-opus-5",
    "claude-sonnet-5",
    "claude-opus-4-8",
];

pub fn slot_order(id: &str) -> usize {
    MODEL_SLOTS.iter().position(|m| *m == id).unwrap_or(99)
}

pub fn for_model(id: &str) -> Rgb {
    match MODEL_SLOTS.iter().position(|m| *m == id) {
        Some(s) => categorical(s),
        None => other(),
    }
}

const RAMP: [(u32, u32); 5] = [
    (0xFBE9E2, 0x3A2620),
    (0xF5C9B8, 0x5C3426),
    (0xEDA58B, 0x8A4730),
    (0xE0805F, 0xBD5F3F),
    (0xC25E3D, 0xE58262),
];

pub fn ramp_step(i: usize) -> Rgb {
    let (l, d) = RAMP[i.min(RAMP.len() - 1)];
    pick(l, d)
}

pub const RAMP_LEN: usize = RAMP.len();

/// Sequential coral ramp for magnitude (calendar, heatmap), light → dark. `None` for zero.
pub fn ramp(fraction: f64) -> Option<Rgb> {
    if fraction <= 0.0 {
        return None;
    }
    let i = ((fraction * RAMP.len() as f64).ceil() as usize).saturating_sub(1);
    Some(ramp_step(i))
}

pub fn status(s: BudgetStatus) -> Rgb {
    match s {
        BudgetStatus::NoBudget | BudgetStatus::UnderBudget => pick(0x0B8A4A, 0x2FBF71),
        BudgetStatus::Tight => pick(0xB86E00, 0xF0A23B),
        BudgetStatus::WillExceed => pick(0xC2410C, 0xF97316),
        BudgetStatus::Exceeded => pick(0xC0262D, 0xF05252),
    }
}

/// Symbolic icon for a status (Adwaita icon names).
pub fn status_icon(s: BudgetStatus) -> &'static str {
    match s {
        BudgetStatus::NoBudget | BudgetStatus::UnderBudget => "emblem-ok-symbolic",
        BudgetStatus::Tight => "dialog-information-symbolic",
        BudgetStatus::WillExceed => "dialog-warning-symbolic",
        BudgetStatus::Exceeded => "dialog-error-symbolic",
    }
}

pub fn status_class(s: BudgetStatus) -> &'static str {
    match s {
        BudgetStatus::NoBudget | BudgetStatus::UnderBudget => "good",
        BudgetStatus::Tight => "tight",
        BudgetStatus::WillExceed => "over",
        BudgetStatus::Exceeded => "exceeded",
    }
}

pub fn for_limit(u: f64) -> Rgb {
    if u >= 0.9 {
        status(BudgetStatus::Exceeded)
    } else if u >= 0.7 {
        status(BudgetStatus::Tight)
    } else {
        coral()
    }
}

/// App CSS for the current theme (GTK 4.14 has no `prefers-color-scheme`, so it's regenerated on change).
pub fn css() -> String {
    let coral = coral().css();
    let mut s = format!(
        r#"
.figure {{ font-weight: 600; font-feature-settings: "tnum"; }}
.numeric {{ font-feature-settings: "tnum"; }}
.coral {{ color: {coral}; }}
.subtle-box {{ background-color: alpha(currentColor, 0.05); border-radius: 12px; }}
.chip {{ background-color: alpha(currentColor, 0.09); border-radius: 8px; padding: 0 6px 1px 6px; font-size: 0.82em; font-weight: 500; }}
.pill {{ border-radius: 17px; padding: 7px 10px; }}
.pill.small {{ border-radius: 11px; padding: 2px 7px; }}
.value-badge {{ background-color: alpha({coral}, 0.14); border-radius: 12px; padding: 10px; }}
.panel-footer button {{ min-width: 28px; min-height: 28px; padding: 4px; }}
.day-cell {{ border-radius: 7px; padding: 3px 5px; border: 1px solid transparent; }}
.day-cell.empty {{ background-color: alpha(currentColor, 0.05); }}
.day-cell.today {{ border: 1.5px solid {coral}; }}
.day-cell.selected {{ border: 1.5px solid currentColor; }}
.day-cell:hover {{ border-color: alpha(currentColor, 0.4); }}
.small-caps {{ font-size: 0.78em; font-weight: 600; letter-spacing: 0.04em; }}
"#
    );
    for st in [BudgetStatus::UnderBudget, BudgetStatus::Tight, BudgetStatus::WillExceed, BudgetStatus::Exceeded] {
        let c = status(st).css();
        s.push_str(&format!(".pill.{cls} {{ background-color: alpha({c}, 0.13); }}\n.status-{cls} {{ color: {c}; }}\n", cls = status_class(st)));
    }
    for i in 0..RAMP_LEN {
        let c = ramp_step(i).css();
        let ink = if i >= 3 { "#ffffff" } else if is_dark() { "#ffffff" } else { "#1e1e1e" };
        s.push_str(&format!(".ramp-{i} {{ background-color: {c}; color: {ink}; }}\n"));
    }
    s
}

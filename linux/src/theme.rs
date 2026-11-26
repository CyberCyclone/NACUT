//! The same palette as the Mac and Windows apps: coral accent, a validated categorical palette in fixed
//! order, a sequential coral ramp and status colors, each with light and dark steps.

use nacut_core::forecast::BudgetStatus;
use nacut_core::views::model_slot;

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
    pick_for(light, dark, is_dark())
}

/// The light or dark step of a color.
fn pick_for(light: u32, dark: u32, dark_theme: bool) -> Rgb {
    Rgb(if dark_theme { dark } else { light })
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

const OTHER: (u32, u32) = (0x9A9890, 0x6E6D66);

pub fn other() -> Rgb {
    pick(OTHER.0, OTHER.1)
}

/// A model's color from the core's fixed slot registry, so it matches every view, filter and app.
pub fn for_model(id: &str) -> Rgb {
    match model_slot(id) {
        Some(s) => categorical(s as usize),
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
    ramp_index(fraction).map(ramp_step)
}

/// Ramp step for a 0…1 magnitude (each step covers a fifth, top step from 0.8 up); `None` for zero, negative or NaN.
pub fn ramp_index(fraction: f64) -> Option<usize> {
    if fraction.is_nan() || fraction <= 0.0 {
        return None;
    }
    Some(((fraction * RAMP.len() as f64).ceil() as usize).saturating_sub(1).min(RAMP.len() - 1))
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

#[cfg(test)]
mod tests {
    use super::*;
    use nacut_core::views::MODEL_SLOTS;

    const ALL_STATUSES: [BudgetStatus; 5] = [BudgetStatus::NoBudget, BudgetStatus::UnderBudget, BudgetStatus::Tight, BudgetStatus::WillExceed, BudgetStatus::Exceeded];

    #[test]
    fn rgb_channels() {
        let c = Rgb(0xD97757);
        assert_eq!(c.r(), 0xD9 as f64 / 255.0);
        assert_eq!(c.g(), 0x77 as f64 / 255.0);
        assert_eq!(c.b(), 0x57 as f64 / 255.0);
        assert_eq!((Rgb(0xFFFFFF).r(), Rgb(0xFFFFFF).g(), Rgb(0xFFFFFF).b()), (1.0, 1.0, 1.0));
        assert_eq!((Rgb(0).r(), Rgb(0).g(), Rgb(0).b()), (0.0, 0.0, 0.0));
        assert_eq!((Rgb(0xFF0000).g(), Rgb(0x00FF00).b(), Rgb(0x0000FF).r()), (0.0, 0.0, 0.0));
    }

    #[test]
    fn rgb_css_is_zero_padded_lowercase_hex() {
        assert_eq!(Rgb(0xD97757).css(), "#d97757");
        assert_eq!(Rgb(0x0000FF).css(), "#0000ff");
        assert_eq!(Rgb(0).css(), "#000000");
    }

    #[test]
    fn pick_for_chooses_the_theme_step() {
        assert_eq!(pick_for(0x111111, 0x222222, false), Rgb(0x111111));
        assert_eq!(pick_for(0x111111, 0x222222, true), Rgb(0x222222));
    }

    #[test]
    fn ramp_index_boundaries() {
        assert_eq!(ramp_index(0.0), None);
        assert_eq!(ramp_index(-0.0), None);
        assert_eq!(ramp_index(-1.0), None);
        assert_eq!(ramp_index(f64::NEG_INFINITY), None);
        assert_eq!(ramp_index(f64::NAN), None);
        assert_eq!(ramp_index(f64::MIN_POSITIVE), Some(0));
        assert_eq!(ramp_index(1e-300), Some(0));
        assert_eq!(ramp_index(0.2), Some(0));
        assert_eq!(ramp_index(0.2000001), Some(1));
        assert_eq!(ramp_index(0.5), Some(2));
        assert_eq!(ramp_index(0.8), Some(3));
        assert_eq!(ramp_index(0.81), Some(4));
        assert_eq!(ramp_index(1.0), Some(RAMP_LEN - 1));
        assert_eq!(ramp_index(1.5), Some(RAMP_LEN - 1));
        assert_eq!(ramp_index(f64::MAX), Some(RAMP_LEN - 1));
        assert_eq!(ramp_index(f64::INFINITY), Some(RAMP_LEN - 1));
    }

    #[test]
    fn ramp_index_is_monotonic() {
        let mut last = 0;
        for i in 1..=1000 {
            let step = ramp_index(i as f64 / 1000.0).unwrap();
            assert!(step >= last && step < RAMP_LEN);
            last = step;
        }
        assert_eq!(last, RAMP_LEN - 1);
    }

    #[test]
    fn status_icons() {
        assert_eq!(status_icon(BudgetStatus::NoBudget), "emblem-ok-symbolic");
        assert_eq!(status_icon(BudgetStatus::UnderBudget), "emblem-ok-symbolic");
        assert_eq!(status_icon(BudgetStatus::Tight), "dialog-information-symbolic");
        assert_eq!(status_icon(BudgetStatus::WillExceed), "dialog-warning-symbolic");
        assert_eq!(status_icon(BudgetStatus::Exceeded), "dialog-error-symbolic");
    }

    #[test]
    fn status_classes() {
        let classes: Vec<&str> = ALL_STATUSES.iter().map(|s| status_class(*s)).collect();
        assert_eq!(classes, ["good", "good", "tight", "over", "exceeded"]);
    }

    #[test]
    fn model_slots_get_distinct_categorical_colors() {
        let slots: Vec<usize> = MODEL_SLOTS.iter().map(|m| model_slot(m).expect("registered") as usize).collect();
        assert!(slots.iter().all(|s| *s < CATEGORICAL.len()));
        for dark in [false, true] {
            let colors: Vec<Rgb> = slots.iter().map(|s| pick_for(CATEGORICAL[*s].0, CATEGORICAL[*s].1, dark)).collect();
            for (i, a) in colors.iter().enumerate() {
                assert!(colors[i + 1..].iter().all(|b| b != a), "{} shares a color", MODEL_SLOTS[i]);
            }
        }
    }

    #[test]
    fn unknown_models_use_the_other_color() {
        assert_eq!(model_slot("claude-unknown-9"), None);
        assert_eq!(model_slot(""), None);
        assert!(CATEGORICAL.iter().all(|(l, d)| *l != OTHER.0 && *d != OTHER.1));
    }
}

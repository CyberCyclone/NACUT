//! # nacut-core
//!
//! The shared core of NACUT ("Not Another Claude Usage Tracker"): everything except drawing pixels.
//! The macOS (SwiftUI), Windows (WinUI 3) and Linux (GTK4) apps all call this crate, so their numbers
//! always agree.
//!
//! - [`scanner`] / [`cache`]: incremental reading of Claude Code JSONL transcripts, deduplicated by message.
//! - [`pricing`]: list prices from the shared `Pricing.json`.
//! - [`store`], [`forecast`], [`insights`]: daily series, breakdowns, budget forecast.
//! - [`limits`]: Max/Pro plan limit parsing and pace projections.
//! - [`analytics`], [`merge`]: Anthropic Usage & Cost API and the hybrid merge.
//! - [`engine`]: the object each app drives (background scanning, watching, timers, alerts, queries).
//! - [`views`]: FFI-friendly view models; [`platform`]: OS folders, sign-in, secrets, WSL, file watching.

pub mod alerts;
pub mod analytics;
pub mod cache;
pub mod engine;
pub mod forecast;
pub mod insights;
pub mod limits;
pub mod limits_service;
pub mod merge;
pub mod platform;
pub mod pricing;
pub mod record;
pub mod scanner;
pub mod settings;
pub mod store;
pub mod views;

#[cfg(feature = "ffi")]
pub mod ffi;
#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!("nacut");

pub use cache::LogCache;
pub use forecast::{BillingCycle, BudgetStatus, CurrencyFormat, Forecast};
pub use insights::Insights;
pub use limits::PlanLimit;
pub use pricing::{PriceBook, Rates};
pub use record::UsageRecord;
pub use scanner::LogScanner;
pub use store::{Cal, DailySeries, DayCost, Interval, UsageStore};
pub use engine::{Engine, EngineConfig, EngineObserver, Snapshot, StatRange};
pub use settings::Settings;

/// The core crate version (shown in each app's About section).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

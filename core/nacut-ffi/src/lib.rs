//! Packages `nacut-core` (with its `ffi` feature) as a static/dynamic library and re-exports its
//! UniFFI scaffolding, so `uniffi-bindgen --library` can generate Swift and C# bindings from it.

nacut_core::uniffi_reexport_scaffolding!();
pub use nacut_core;

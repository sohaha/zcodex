#![deny(clippy::print_stdout, clippy::print_stderr)]
#![deny(clippy::disallowed_methods)]

//! Compatibility shim for the legacy `codex-rs/tui` crate path.
//!
//! The active TUI implementation now lives in `codex-rs/tui_app_server`.
//! Keep this crate as a thin re-export so older local workflows that still
//! point at `codex-rs/tui` do not fail with missing sources.

pub use codex_tui_app_server::*;

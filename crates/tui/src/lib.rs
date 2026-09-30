//! `retina-tui` library — split from `main.rs` (same pattern as
//! `crates/query-api`/`crates/orchestrator`) so `tests/` can exercise
//! `api::ApiClient` against a real running `query-api` without needing a
//! terminal.

pub mod api;
pub mod app;
pub mod content;
pub mod logo;
pub mod ui;

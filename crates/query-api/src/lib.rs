//! Minimal query API (dossier section 2.2, étape 4): list recent traces,
//! fetch one trace's spans, and a basic per-kind metrics summary. Contract
//! documented in docs/interfaces/query-api.md.

pub mod app;
pub mod dto;
pub mod queries;
pub mod routes;

pub use app::build_app;

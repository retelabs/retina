//! Converts `OncologyRun` (the real state shape used by
//! oncology-pipeline/oncology-suite — docs/interfaces/oncology-governance.md)
//! into OTLP and replays it against a running kernel, mirroring
//! `crates/fraudos-replay` for the medical vertical.

pub mod convert;
pub mod ids;
pub mod oncology_run;

pub use convert::convert;
pub use oncology_run::OncologyRun;

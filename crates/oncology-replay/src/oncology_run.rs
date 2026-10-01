//! Mirrors the fields of the real `OncologyState`/`OncologyOutput`
//! (the oncology pipeline's `src/state/oncology_state.py`)
//! that matter for governance — see docs/interfaces/oncology-governance.md.
//!
//! `started_at`/`ended_at` are **not** real `OncologyState` fields (unlike
//! fraudos' `AgentSpan`, which does carry real timestamps) — LangGraph's
//! checkpointed state doesn't expose run-level wall-clock timing the way
//! `AgentSpan.from_run_result` computes it. Added here only so this replay
//! tool has something to put in OTLP's required `start_time_unix_nano`/
//! `end_time_unix_nano` — synthetic, not read from the source.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct OncologyRun {
    pub session_id: String,
    pub patient_id: String,
    /// One of `OncologyState.current_step`'s `Literal` values: "ingestion",
    /// "preprocessing", "modeling", "evaluation", "visualization",
    /// "recommendation", "monitoring", "done", "failed".
    pub current_step: String,
    pub hipaa_cleared: bool,
    pub gdpr_cleared: bool,
    pub compliance_flags: Vec<String>,
    pub submitted_by: Option<String>,
    pub approved_by: Option<String>,
    /// Synthetic — see module doc.
    pub started_at: String,
    pub ended_at: String,
}

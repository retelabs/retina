//! Mirrors the real `AgentSpan` dataclass from
//! the fraudos prototype repository `observability/span.py` (read 2026-08-14,
//! see docs/interfaces/fraudos-agentspan.md) — a rollup of one full agent
//! run, not a per-operation span. Field names/types match `AgentSpan.to_dict()`
//! exactly so a real CloudWatch/DynamoDB export could deserialize here
//! unmodified.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentSpan {
    pub session_id: String,
    /// "fraud_orchestrator" | "fraud_investigator" | "compliance_officer" |
    /// "fraud_scorer" (agents/factory.py roles, not a closed enum here —
    /// same reasoning as kernel_model::OperationName: the source can add
    /// roles without our tooling knowing in advance).
    pub agent_role: String,
    pub task_summary: String,
    pub total_turns: u32,
    pub escalated_to_opus: bool,
    pub tools_called: Vec<String>,
    pub tools_failed: Vec<String>,
    #[serde(default)]
    pub unique_tools: u32,
    pub success: bool,
    pub requires_human_review: bool,
    pub final_decision: Option<String>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub estimated_cost_usd: f64,
    pub primary_model_id: String,
    pub duration_seconds: f64,
    /// ISO 8601 (`datetime.isoformat()` from `observability/span.py::_ts`).
    pub started_at: String,
    pub ended_at: String,
    pub case_id: Option<String>,
    pub bank_id: Option<String>,
    pub transaction_id: Option<String>,
}

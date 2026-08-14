use super::{ExtraAttributes, SpanContext};

/// `gen_ai.execute_tool.internal` span (vendor/semconv-genai @ 30182acd,
/// `model/gen-ai/spans.yaml`) — the "appel outil" event (dossier section 2.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallEvent {
    pub span: SpanContext,

    /// required
    pub tool_name: String,
    /// recommended if available
    pub tool_call_id: Option<String>,
    /// Free string in the spec (examples: "function", "extension",
    /// "datastore") — NOT a closed enum unlike `operation_name`/`provider_name`,
    /// so this stays a `String` rather than a hand-rolled enum.
    pub tool_type: Option<String>,
    /// recommended if available; spec flags this as possibly sensitive
    pub tool_description: Option<String>,
    /// conditionally_required if applicable: the agent executing the tool
    pub agent_name: Option<String>,

    /// `gen_ai.tool.call.arguments` / `gen_ai.tool.call.result` are `opt_in`
    /// AND flagged "may contain sensitive information" in the spec — matching
    /// our own PII policy (dossier section 2.1: no prompt/response content by
    /// default). Deliberately not first-class fields; if ever captured, they
    /// belong in `extra_attributes` behind the same opt-in gate as message
    /// content, not hardcoded here.
    pub extra_attributes: ExtraAttributes,
}

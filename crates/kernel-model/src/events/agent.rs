use super::{ExtraAttributes, SpanContext};
use crate::error::ModelError;
use crate::operation::OperationName;
use crate::provider::ProviderName;
use crate::value::TokenCount;

/// Which of the two `invoke_agent` spans this event came from
/// (vendor/semconv-genai @ 30182acd, `model/gen-ai/spans.yaml`) — mirrors OTLP
/// `Span.kind` (`CLIENT` vs `INTERNAL`), see docs/interfaces/otlp-ingestion.md
/// "point de jonction" note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentInvocationKind {
    /// `gen_ai.invoke_agent.client` — remote agent service (e.g. AWS Bedrock
    /// Agents, OpenAI Assistants).
    Client,
    /// `gen_ai.invoke_agent.internal` — in-process agent (e.g. LangChain,
    /// CrewAI).
    Internal,
}

/// `gen_ai.invoke_agent.{client,internal}` span — the "run d'agent" event
/// (dossier section 2.1).
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRunEvent {
    pub span: SpanContext,
    pub invocation_kind: AgentInvocationKind,

    /// required, should be `InvokeAgent`
    pub operation_name: OperationName,

    /// conditionally_required if available
    pub agent_name: Option<String>,
    /// conditionally_required if applicable — stable id (e.g. Bedrock agent ARN)
    pub agent_id: Option<String>,
    pub agent_description: Option<String>,
    /// conditionally_required — client variant mainly
    pub agent_version: Option<String>,

    /// recommended, ONLY if the agent has a single fixed model — must stay
    /// unset for agents with dynamic model selection (spec note).
    pub request_model: Option<String>,

    /// required when `invocation_kind == Client`; not part of the attribute
    /// list for the `internal` variant at all (see docs/interfaces/semconv-genai.md)
    /// — an in-process agent has no transport-level "provider". Modeled as
    /// `Option` rather than two separate structs to avoid duplicating every
    /// other field; [`AgentRunEvent::validate`] enforces the constraint.
    pub provider_name: Option<ProviderName>,

    pub input_tokens: Option<TokenCount>,
    pub output_tokens: Option<TokenCount>,
    pub cache_read_input_tokens: Option<TokenCount>,
    pub cache_creation_input_tokens: Option<TokenCount>,

    pub conversation_id: Option<String>,
    pub extra_attributes: ExtraAttributes,
}

impl AgentRunEvent {
    /// Checks the one constraint that can't be expressed in the type alone:
    /// `provider_name` is required for `Client` invocations.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.invocation_kind == AgentInvocationKind::Client && self.provider_name.is_none() {
            return Err(ModelError::MissingRequiredField(
                "provider_name (required when invocation_kind = Client)",
            ));
        }
        Ok(())
    }
}

//! Internal data model for the observability kernel (dossier section 2.1,
//! étape 1). Types here are derived directly from the pinned contracts in
//! docs/interfaces/semconv-genai.md and docs/interfaces/otlp-ingestion.md —
//! see those files for the source material and reasoning behind each choice.

pub mod error;
pub mod events;
pub mod ids;
pub mod operation;
pub mod provider;
pub mod status;
pub mod value;

pub use error::ModelError;
pub use events::{AgentInvocationKind, AgentRunEvent, ModelCallEvent, SpanContext, ToolCallEvent};
pub use ids::{SpanId, TraceId};
pub use operation::OperationName;
pub use provider::ProviderName;
pub use status::{SpanStatus, StatusCode};
pub use value::{Attribute, AttributeValue, TokenCount};

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_span() -> SpanContext {
        SpanContext {
            trace_id: TraceId::try_from(&[1u8; 16][..]).unwrap(),
            span_id: SpanId::try_from(&[2u8; 8][..]).unwrap(),
            parent_span_id: None,
            start_time_unix_nano: 1_000,
            end_time_unix_nano: 1_500,
            status: SpanStatus::default(),
            error_type: None,
        }
    }

    #[test]
    fn duration_nanos_handles_well_formed_span() {
        assert_eq!(sample_span().duration_nanos(), Some(500));
    }

    #[test]
    fn duration_nanos_is_none_for_malformed_span_instead_of_panicking() {
        let mut span = sample_span();
        span.end_time_unix_nano = 0; // end before start: malformed input, not our bug
        assert_eq!(span.duration_nanos(), None);
    }

    #[test]
    fn agent_run_event_requires_provider_name_for_client_invocations() {
        let event = AgentRunEvent {
            span: sample_span(),
            invocation_kind: AgentInvocationKind::Client,
            operation_name: OperationName::InvokeAgent,
            agent_name: Some("fraud-triage".to_string()),
            agent_id: None,
            agent_description: None,
            agent_version: None,
            request_model: None,
            provider_name: None,
            input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            conversation_id: None,
            extra_attributes: vec![],
        };
        assert!(event.validate().is_err());

        let event = AgentRunEvent {
            provider_name: Some(ProviderName::AwsBedrock),
            ..event
        };
        assert!(event.validate().is_ok());
    }

    #[test]
    fn agent_run_event_allows_missing_provider_name_for_internal_invocations() {
        let event = AgentRunEvent {
            span: sample_span(),
            invocation_kind: AgentInvocationKind::Internal,
            operation_name: OperationName::InvokeAgent,
            agent_name: Some("langchain-agent".to_string()),
            agent_id: None,
            agent_description: None,
            agent_version: None,
            request_model: None,
            provider_name: None,
            input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            conversation_id: None,
            extra_attributes: vec![],
        };
        assert!(event.validate().is_ok());
    }
}

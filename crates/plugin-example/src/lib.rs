//! `ExamplePlugin` — the "plugin factice" dossier étape 5 asks for, to
//! validate the plugin-api contract before a real vertical exists. It is
//! deliberately generic/toy, not a stand-in for the fintech plugin (dossier
//! section 3): it computes a derived `example.total_tokens` attribute for
//! model_call/agent_run events, and warns when a tool call is missing
//! `tool_call_id`. Neither behavior is fintech-specific — the point is to
//! prove the trait is implementable and produces something observable, not
//! to anticipate a real vertical's rules.

use kernel_model::{AttributeValue, TokenCount};
use plugin_api::{KernelEvent, Plugin, PluginOutcome};

pub struct ExamplePlugin;

impl Plugin for ExamplePlugin {
    fn name(&self) -> &'static str {
        "example-plugin"
    }

    fn inspect(&self, event: KernelEvent<'_>) -> PluginOutcome {
        match event {
            KernelEvent::ModelCall(e) => total_tokens_outcome(e.input_tokens, e.output_tokens),
            KernelEvent::AgentRun(e) => total_tokens_outcome(e.input_tokens, e.output_tokens),
            KernelEvent::ToolCall(e) => {
                let mut outcome = PluginOutcome::default();
                if e.tool_call_id.is_none() {
                    outcome.warnings.push(format!(
                        "tool call to `{}` has no tool_call_id",
                        e.tool_name
                    ));
                }
                outcome
            }
        }
    }
}

fn total_tokens_outcome(input: Option<TokenCount>, output: Option<TokenCount>) -> PluginOutcome {
    let mut outcome = PluginOutcome::default();
    if let (Some(input), Some(output)) = (input, output) {
        // Same reflex as everywhere else in this kernel: no bare `as i64` on
        // a value that came from outside this function's control, even
        // though token counts overflowing i64 when summed is not a
        // realistic scenario in practice.
        if let Ok(total) = i64::try_from(input.get() + output.get()) {
            outcome.attributes.push((
                "example.total_tokens".to_string(),
                AttributeValue::Int(total),
            ));
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel_model::{
        AgentInvocationKind, AgentRunEvent, ModelCallEvent, OperationName, ProviderName,
        SpanContext, SpanId, SpanStatus, ToolCallEvent, TraceId,
    };

    fn sample_span() -> SpanContext {
        SpanContext {
            trace_id: TraceId::try_from(&[1u8; 16][..]).unwrap(),
            span_id: SpanId::try_from(&[2u8; 8][..]).unwrap(),
            parent_span_id: None,
            start_time_unix_nano: 0,
            end_time_unix_nano: 0,
            status: SpanStatus::default(),
            error_type: None,
        }
    }

    #[test]
    fn computes_total_tokens_for_a_model_call() {
        let event = ModelCallEvent {
            span: sample_span(),
            provider_name: ProviderName::AwsBedrock,
            operation_name: OperationName::Chat,
            request_model: None,
            response_model: None,
            input_tokens: Some(TokenCount::try_from(10).unwrap()),
            output_tokens: Some(TokenCount::try_from(5).unwrap()),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            finish_reasons: vec![],
            conversation_id: None,
            extra_attributes: vec![],
        };

        let outcome = ExamplePlugin.inspect(KernelEvent::ModelCall(&event));
        assert_eq!(
            outcome.attributes,
            vec![("example.total_tokens".to_string(), AttributeValue::Int(15))]
        );
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn skips_total_tokens_when_either_count_is_missing() {
        let event = ModelCallEvent {
            span: sample_span(),
            provider_name: ProviderName::AwsBedrock,
            operation_name: OperationName::Chat,
            request_model: None,
            response_model: None,
            input_tokens: Some(TokenCount::try_from(10).unwrap()),
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            finish_reasons: vec![],
            conversation_id: None,
            extra_attributes: vec![],
        };

        let outcome = ExamplePlugin.inspect(KernelEvent::ModelCall(&event));
        assert!(outcome.attributes.is_empty());
    }

    #[test]
    fn warns_on_tool_call_missing_an_id() {
        let event = ToolCallEvent {
            span: sample_span(),
            tool_name: "Flights".to_string(),
            tool_call_id: None,
            tool_type: None,
            tool_description: None,
            agent_name: None,
            extra_attributes: vec![],
        };

        let outcome = ExamplePlugin.inspect(KernelEvent::ToolCall(&event));
        assert_eq!(
            outcome.warnings,
            vec!["tool call to `Flights` has no tool_call_id".to_string()]
        );
    }

    #[test]
    fn agent_run_also_gets_total_tokens() {
        let event = AgentRunEvent {
            span: sample_span(),
            invocation_kind: AgentInvocationKind::Internal,
            operation_name: OperationName::InvokeAgent,
            agent_name: Some("triage".to_string()),
            agent_id: None,
            agent_description: None,
            agent_version: None,
            request_model: None,
            provider_name: None,
            input_tokens: Some(TokenCount::try_from(3).unwrap()),
            output_tokens: Some(TokenCount::try_from(4).unwrap()),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            conversation_id: None,
            extra_attributes: vec![],
        };

        let outcome = ExamplePlugin.inspect(KernelEvent::AgentRun(&event));
        assert_eq!(
            outcome.attributes,
            vec![("example.total_tokens".to_string(), AttributeValue::Int(7))]
        );
    }
}

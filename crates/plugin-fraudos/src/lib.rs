//! `FraudosPlugin` — the first plugin genuinely informed by a real vertical
//! (dossier section 2.4: "let the first real vertical inform
//! [the contract]"), unlike `plugin-example`/`plugin-wasm-example` which are
//! deliberately generic. Grounded in what `docs/interfaces/fraudos-agentspan.md`
//! found in the real fraudos prototype: agent runs carry `fraudos.*`
//! extra_attributes (attached by `crates/fraudos-replay`, not first-class
//! `kernel-model` fields — the fintech attributes the dossier anticipated in
//! section 3 never got promoted beyond the generic bag, which is exactly
//! what a plugin is for).
//!
//! Only acts on `AgentRunEvent`s that actually carry `fraudos.*` attributes
//! — anything else (a `ModelCallEvent`/`ToolCallEvent`, or an `AgentRunEvent`
//! from an unrelated vertical with no `fraudos.final_decision`) is a no-op.
//! A plugin misfiring on data it doesn't recognize would be a much bigger
//! problem than one that does nothing — same reasoning as
//! `docs/interfaces/plugin-contract-v0.md`'s infallible `PluginOutcome`.

use kernel_model::AttributeValue;
use plugin_api::{KernelEvent, Plugin, PluginOutcome};

/// Decisions serious enough that losing the ability to correlate them with
/// a real-world outcome later (dossier section 3: "l'issue arrive souvent
/// after inference") would matter for an audit.
const CONSEQUENTIAL_DECISIONS: &[&str] = &[
    "CONFIRMED_FRAUD",
    "REQUEST_BLOCK",
    "ESCALATED_COMPLIANCE",
    "CASE_OPENED",
];

/// Decisions urgent enough to flag for immediate human attention.
const URGENT_DECISIONS: &[&str] = &["CONFIRMED_FRAUD", "REQUEST_BLOCK"];

pub struct FraudosPlugin;

fn find_str<'a>(attrs: &'a [(String, AttributeValue)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_str())
}

impl Plugin for FraudosPlugin {
    fn name(&self) -> &str {
        "fraudos-plugin"
    }

    fn inspect(&self, event: KernelEvent<'_>) -> PluginOutcome {
        let KernelEvent::AgentRun(event) = event else {
            return PluginOutcome::default();
        };

        let Some(final_decision) = find_str(&event.extra_attributes, "fraudos.final_decision")
        else {
            // No fraudos.* attributes at all: not a fraudos-flavored run,
            // nothing to interpret.
            return PluginOutcome::default();
        };

        let transaction_id = find_str(&event.extra_attributes, "fraudos.transaction_id");
        let mut outcome = PluginOutcome::default();

        if CONSEQUENTIAL_DECISIONS.contains(&final_decision) && transaction_id.is_none() {
            outcome.warnings.push(format!(
                "consequential fraud decision `{final_decision}` has no fraudos.transaction_id — \
                 its real-world outcome can't be correlated back to this run later"
            ));
        }

        if URGENT_DECISIONS.contains(&final_decision) {
            outcome.attributes.push((
                "fraudos.requires_urgent_review".to_string(),
                AttributeValue::Bool(true),
            ));
        }

        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel_model::{
        AgentInvocationKind, AgentRunEvent, OperationName, SpanContext, SpanId, SpanStatus, TraceId,
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

    fn agent_run(extra_attributes: Vec<(String, AttributeValue)>) -> AgentRunEvent {
        AgentRunEvent {
            span: sample_span(),
            invocation_kind: AgentInvocationKind::Internal,
            operation_name: OperationName::InvokeAgent,
            agent_name: Some("fraud_investigator".to_string()),
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
            extra_attributes,
        }
    }

    #[test]
    fn confirmed_fraud_with_transaction_id_gets_flagged_urgent_and_no_warning() {
        let event = agent_run(vec![
            (
                "fraudos.final_decision".to_string(),
                AttributeValue::String("CONFIRMED_FRAUD".to_string()),
            ),
            (
                "fraudos.transaction_id".to_string(),
                AttributeValue::String("TX-1".to_string()),
            ),
        ]);

        let outcome = FraudosPlugin.inspect(KernelEvent::AgentRun(&event));

        assert!(outcome.warnings.is_empty());
        assert_eq!(
            outcome.attributes,
            vec![(
                "fraudos.requires_urgent_review".to_string(),
                AttributeValue::Bool(true)
            )]
        );
    }

    #[test]
    fn confirmed_fraud_without_transaction_id_warns() {
        let event = agent_run(vec![(
            "fraudos.final_decision".to_string(),
            AttributeValue::String("CONFIRMED_FRAUD".to_string()),
        )]);

        let outcome = FraudosPlugin.inspect(KernelEvent::AgentRun(&event));

        assert_eq!(outcome.warnings.len(), 1);
        assert!(outcome.warnings[0].contains("CONFIRMED_FRAUD"));
    }

    #[test]
    fn dismissed_without_transaction_id_is_fine_no_warning_no_urgent_flag() {
        let event = agent_run(vec![(
            "fraudos.final_decision".to_string(),
            AttributeValue::String("DISMISSED".to_string()),
        )]);

        let outcome = FraudosPlugin.inspect(KernelEvent::AgentRun(&event));

        assert!(outcome.warnings.is_empty());
        assert!(outcome.attributes.is_empty());
    }

    #[test]
    fn non_fraudos_agent_run_is_a_no_op() {
        let event = agent_run(vec![]);
        let outcome = FraudosPlugin.inspect(KernelEvent::AgentRun(&event));
        assert_eq!(outcome, PluginOutcome::default());
    }

    #[test]
    fn model_call_and_tool_call_events_are_always_a_no_op() {
        use kernel_model::{ModelCallEvent, OperationName as Op, ProviderName, ToolCallEvent};

        let model_call = ModelCallEvent {
            span: sample_span(),
            provider_name: ProviderName::AwsBedrock,
            operation_name: Op::Chat,
            request_model: None,
            response_model: None,
            input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            finish_reasons: vec![],
            conversation_id: None,
            extra_attributes: vec![(
                "fraudos.final_decision".to_string(),
                AttributeValue::String("CONFIRMED_FRAUD".to_string()),
            )],
        };
        assert_eq!(
            FraudosPlugin.inspect(KernelEvent::ModelCall(&model_call)),
            PluginOutcome::default()
        );

        let tool_call = ToolCallEvent {
            span: sample_span(),
            tool_name: "get_transaction_score".to_string(),
            tool_call_id: None,
            tool_type: None,
            tool_description: None,
            agent_name: None,
            extra_attributes: vec![],
        };
        assert_eq!(
            FraudosPlugin.inspect(KernelEvent::ToolCall(&tool_call)),
            PluginOutcome::default()
        );
    }
}

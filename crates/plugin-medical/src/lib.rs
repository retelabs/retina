//! `MedicalPlugin` — governance gates for the oncology vertical, grounded in
//! explicit invariants read from the real oncology pipeline's repository (not
//! invented) — see docs/interfaces/oncology-governance.md.
//!
//! Same safety posture as `plugin-fraudos`: no-op on anything that isn't an
//! `AgentRunEvent` carrying `oncology.*` attributes, so this plugin can run
//! against any pipeline without misinterpreting data from other verticals.

use kernel_model::AttributeValue;
use plugin_api::{KernelEvent, Plugin, PluginOutcome};

fn find_str<'a>(attrs: &'a [(String, AttributeValue)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_str())
}

fn find_bool(attrs: &[(String, AttributeValue)], key: &str) -> Option<bool> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| match v {
            AttributeValue::Bool(b) => Some(*b),
            _ => None,
        })
}

pub struct MedicalPlugin;

impl Plugin for MedicalPlugin {
    fn name(&self) -> &str {
        "medical-plugin"
    }

    fn inspect(&self, event: KernelEvent<'_>) -> PluginOutcome {
        let KernelEvent::AgentRun(event) = event else {
            return PluginOutcome::default();
        };

        let Some(current_step) = find_str(&event.extra_attributes, "oncology.current_step") else {
            // No oncology.* attributes: not this vertical, nothing to interpret.
            return PluginOutcome::default();
        };

        let hipaa_cleared = find_bool(&event.extra_attributes, "oncology.hipaa_cleared");
        let gdpr_cleared = find_bool(&event.extra_attributes, "oncology.gdpr_cleared");
        let submitted_by = find_str(&event.extra_attributes, "oncology.submitted_by");
        let approved_by = find_str(&event.extra_attributes, "oncology.approved_by");

        let mut outcome = PluginOutcome::default();

        // Rule 1: the deterministic compliance gate (route_after_ingestion)
        // must have halted the pipeline on a compliance failure — if it
        // reports failure but the run kept going, that's a defense-in-depth
        // signal, not proof of a real bypass (the kernel can't see the
        // gate's own control flow, only the attributes it produced).
        let compliance_failed = hipaa_cleared == Some(false) || gdpr_cleared == Some(false);
        if compliance_failed && current_step != "failed" {
            outcome.warnings.push(format!(
                "compliance gate reports a failure (hipaa_cleared={hipaa_cleared:?}, \
                 gdpr_cleared={gdpr_cleared:?}) but current_step=`{current_step}`, not `failed` — \
                 possible governance gate bypass"
            ));
        }

        // Rule 2: no clinical recommendation without HITL sign-off
        // (dossier invariant: "no clinical recommendation without human
        // sign-off").
        let reached_recommendation =
            matches!(current_step, "recommendation" | "monitoring" | "done");
        if reached_recommendation && approved_by.is_none() {
            outcome.warnings.push(format!(
                "current_step=`{current_step}` reached or passed the recommendation gate without \
                 oncology.approved_by — missing HITL sign-off"
            ));
        }

        // Derived signal (not a warning): a run legitimately paused awaiting
        // human approval — submitted but not yet approved.
        if submitted_by.is_some() && approved_by.is_none() {
            outcome.attributes.push((
                "oncology.awaiting_approval".to_string(),
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
            agent_name: Some("oncology_pipeline".to_string()),
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
    fn approved_recommendation_is_clean() {
        let event = agent_run(vec![
            (
                "oncology.current_step".to_string(),
                AttributeValue::String("done".to_string()),
            ),
            (
                "oncology.hipaa_cleared".to_string(),
                AttributeValue::Bool(true),
            ),
            (
                "oncology.gdpr_cleared".to_string(),
                AttributeValue::Bool(true),
            ),
            (
                "oncology.submitted_by".to_string(),
                AttributeValue::String("dr.smith".to_string()),
            ),
            (
                "oncology.approved_by".to_string(),
                AttributeValue::String("dr.jones".to_string()),
            ),
        ]);

        let outcome = MedicalPlugin.inspect(KernelEvent::AgentRun(&event));
        assert!(outcome.warnings.is_empty());
        assert!(outcome.attributes.is_empty());
    }

    #[test]
    fn pending_hitl_approval_is_a_derived_signal_not_a_warning() {
        // Realistic intermediate state: LangGraph paused at interrupt_before,
        // not a bug — see docs/interfaces/oncology-governance.md.
        let event = agent_run(vec![
            (
                "oncology.current_step".to_string(),
                AttributeValue::String("recommendation".to_string()),
            ),
            (
                "oncology.hipaa_cleared".to_string(),
                AttributeValue::Bool(true),
            ),
            (
                "oncology.gdpr_cleared".to_string(),
                AttributeValue::Bool(true),
            ),
            (
                "oncology.submitted_by".to_string(),
                AttributeValue::String("dr.smith".to_string()),
            ),
        ]);

        let outcome = MedicalPlugin.inspect(KernelEvent::AgentRun(&event));

        // Still triggers rule 2 (reached "recommendation" without approval)
        // — the awaiting_approval attribute doesn't suppress the warning,
        // it's a monitoring signal about *why*, not an excuse.
        assert_eq!(outcome.warnings.len(), 1);
        assert_eq!(
            outcome.attributes,
            vec![(
                "oncology.awaiting_approval".to_string(),
                AttributeValue::Bool(true)
            )]
        );
    }

    #[test]
    fn compliance_failure_that_did_not_halt_the_pipeline_warns() {
        let event = agent_run(vec![
            (
                "oncology.current_step".to_string(),
                AttributeValue::String("modeling".to_string()),
            ),
            (
                "oncology.hipaa_cleared".to_string(),
                AttributeValue::Bool(false),
            ),
            (
                "oncology.gdpr_cleared".to_string(),
                AttributeValue::Bool(true),
            ),
        ]);

        let outcome = MedicalPlugin.inspect(KernelEvent::AgentRun(&event));
        assert_eq!(outcome.warnings.len(), 1);
        assert!(outcome.warnings[0].contains("bypass"));
    }

    #[test]
    fn compliance_failure_that_correctly_halted_the_pipeline_is_fine() {
        let event = agent_run(vec![
            (
                "oncology.current_step".to_string(),
                AttributeValue::String("failed".to_string()),
            ),
            (
                "oncology.hipaa_cleared".to_string(),
                AttributeValue::Bool(false),
            ),
            (
                "oncology.gdpr_cleared".to_string(),
                AttributeValue::Bool(false),
            ),
        ]);

        let outcome = MedicalPlugin.inspect(KernelEvent::AgentRun(&event));
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn non_oncology_agent_run_is_a_no_op() {
        let event = agent_run(vec![]);
        assert_eq!(
            MedicalPlugin.inspect(KernelEvent::AgentRun(&event)),
            PluginOutcome::default()
        );
    }
}

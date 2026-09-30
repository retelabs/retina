//! `TriageEvalPlugin` — first "eval-as-plugin" (2026-08-17, initiated by
//! the first client's team), same deterministic substrate as `plugin-medical`: read
//! attributes already on the span, apply a rule, write attributes/warnings.
//! No LLM-as-judge here — see docs/interfaces/triage-eval-plugin.md for why
//! that's a deliberately separate, still-open chantier (doesn't fit the
//! synchronous/no-I/O `Plugin` contract, `docs/interfaces/plugin-contract-v0.md`).
//!
//! Grounded in the first client's real code, not invented (`TriageAgent.cs`,
//! `Domain/Entities/Service.cs`, `generate-rich-seed.py`): the triage
//! prompt's vocabulary ("cardiologie, pédiatrie, neurologie, biologie...")
//! is open-ended ("for example"), while the actual `Service` entities the client
//! routes against are a fixed, much smaller list — confirmed drift already
//! exists in their seed data (`biologie`/`neurologie` tags with no matching
//! `Service` row). This plugin makes that drift visible via
//! `eval.triage.tag_known`, it doesn't fix it (the client decides what to do
//! about a false result).

use kernel_model::AttributeValue;
use plugin_api::{KernelEvent, Plugin, PluginOutcome};

/// Real `Service.Name` rows as seeded today (`generate-rich-seed.py:129-134`)
/// — the default vocabulary if `TRIAGE_KNOWN_SERVICES` isn't set
/// (`crates/kernel/src/main.rs`). Not a guess: this is the client's actual current
/// data, provided by them, not a plausible-looking list invented for this
/// plugin.
pub const DEFAULT_KNOWN_SERVICES: &[&str] = &[
    "Cardiologie",
    "Pédiatrie",
    "Urgences",
    "Gynécologie-Obstétrique",
    "Dermatologie",
    "Médecine générale",
];

fn find_str<'a>(attrs: &'a [(String, AttributeValue)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_str())
}

/// Same normalization the client itself applies to the raw tag
/// (`TriageAgent.cs`: `.Trim().ToLowerInvariant()`) — comparing against
/// that already-normalized form, not the raw prompt output, so this plugin
/// doesn't invent its own notion of "equal".
fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

pub struct TriageEvalPlugin {
    known_services_normalized: Vec<String>,
}

impl TriageEvalPlugin {
    pub fn new(known_services: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self {
            known_services_normalized: known_services
                .into_iter()
                .map(|s| normalize(s.as_ref()))
                .collect(),
        }
    }
}

impl Plugin for TriageEvalPlugin {
    fn name(&self) -> &str {
        "triage-eval-plugin"
    }

    fn inspect(&self, event: KernelEvent<'_>) -> PluginOutcome {
        let KernelEvent::AgentRun(event) = event else {
            return PluginOutcome::default();
        };

        // Not yet emitted by the client as of 2026-08-17 (docs/interfaces/triage-eval-plugin.md)
        // — a real gap found while scoping this plugin, not assumed present.
        // No-op until the client adds it, same "not this vertical, nothing to
        // interpret" posture as plugin-medical's oncology.current_step gate.
        let Some(raw_tag) = find_str(&event.extra_attributes, "oncology.triage.tag") else {
            return PluginOutcome::default();
        };

        let known = self
            .known_services_normalized
            .iter()
            .any(|s| s == &normalize(raw_tag));

        let mut outcome = PluginOutcome::default();
        outcome.attributes.push((
            "eval.triage.tag_known".to_string(),
            AttributeValue::Bool(known),
        ));
        if !known {
            outcome.warnings.push(format!(
                "triage tag `{raw_tag}` doesn't match any known Service — possible \
                 vocabulary drift between the triage prompt and the Service entity"
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
            agent_name: Some("triage".to_string()),
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

    fn plugin() -> TriageEvalPlugin {
        TriageEvalPlugin::new(DEFAULT_KNOWN_SERVICES)
    }

    #[test]
    fn a_known_tag_is_marked_known_without_a_warning() {
        // Real case produced this session: a 6-year-old with a swallowed toy
        // -> "pédiatrie", matches Service "Pédiatrie".
        let event = agent_run(vec![(
            "oncology.triage.tag".to_string(),
            AttributeValue::String("pédiatrie".to_string()),
        )]);

        let outcome = plugin().inspect(KernelEvent::AgentRun(&event));
        assert_eq!(
            outcome.attributes,
            vec![(
                "eval.triage.tag_known".to_string(),
                AttributeValue::Bool(true)
            )]
        );
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn a_drifted_tag_already_seen_in_the_clients_own_seed_data_is_flagged() {
        // seed-dev.sql:139 has a real Call.AiTriageTag = 'biologie' with no
        // matching Service row — the exact drift that motivated this plugin.
        let event = agent_run(vec![(
            "oncology.triage.tag".to_string(),
            AttributeValue::String("biologie".to_string()),
        )]);

        let outcome = plugin().inspect(KernelEvent::AgentRun(&event));
        assert_eq!(
            outcome.attributes,
            vec![(
                "eval.triage.tag_known".to_string(),
                AttributeValue::Bool(false)
            )]
        );
        assert_eq!(outcome.warnings.len(), 1);
        assert!(outcome.warnings[0].contains("biologie"));
    }

    #[test]
    fn singular_urgence_does_not_match_plural_service_urgences() {
        // Prompt says "urgence" (singular), the real Service is "Urgences"
        // (plural) — a genuine mismatch even though the concept exists,
        // not just a missing-entity case.
        let event = agent_run(vec![(
            "oncology.triage.tag".to_string(),
            AttributeValue::String("urgence".to_string()),
        )]);

        let outcome = plugin().inspect(KernelEvent::AgentRun(&event));
        assert_eq!(
            outcome.attributes,
            vec![(
                "eval.triage.tag_known".to_string(),
                AttributeValue::Bool(false)
            )]
        );
    }

    #[test]
    fn comparison_is_case_and_whitespace_normalized_like_the_clients_own_postprocessing() {
        let event = agent_run(vec![(
            "oncology.triage.tag".to_string(),
            AttributeValue::String("  CARDIOLOGIE  ".to_string()),
        )]);

        let outcome = plugin().inspect(KernelEvent::AgentRun(&event));
        assert_eq!(
            outcome.attributes,
            vec![(
                "eval.triage.tag_known".to_string(),
                AttributeValue::Bool(true)
            )]
        );
    }

    #[test]
    fn absent_tag_is_a_no_op() {
        let event = agent_run(vec![]);
        assert_eq!(
            plugin().inspect(KernelEvent::AgentRun(&event)),
            PluginOutcome::default()
        );
    }

    #[test]
    fn non_agent_run_event_is_a_no_op() {
        use kernel_model::ToolCallEvent;
        let event = ToolCallEvent {
            span: sample_span(),
            tool_name: "irrelevant".to_string(),
            tool_call_id: None,
            tool_type: None,
            tool_description: None,
            agent_name: None,
            extra_attributes: vec![],
        };
        assert_eq!(
            plugin().inspect(KernelEvent::ToolCall(&event)),
            PluginOutcome::default()
        );
    }
}

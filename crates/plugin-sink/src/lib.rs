//! `PluginSink<S>` — the pipeline insertion point for plugins, resolving the
//! "où insérer l'appel plugin dans le pipeline" question left open since
//! étape 5 (docs/interfaces/plugin-contract-v0.md). See
//! docs/interfaces/oncology-governance.md for why this shape (a `SpanSink`
//! decorator) was chosen over wiring plugins directly into `otlp-receiver`.
//!
//! Wraps any [`SpanSink`]: before delegating a batch to the inner sink, runs
//! every event through each plugin and merges the result into
//! `extra_attributes` — the same generic bag every other provider/domain
//! extension already lives in, no new storage column needed. Warnings become
//! `("plugin.warning", "[<plugin name>] <text>")` entries; duplicate keys are
//! fine (`extra_attributes` was designed as a `Vec`, not a `Map`, from étape 1
//! specifically to tolerate this).

use kernel_model::AttributeValue;
use otlp_receiver::{ConvertedEvent, SpanSink};
use plugin_api::{KernelEvent, Plugin};

pub struct PluginSink<S> {
    inner: S,
    plugins: Vec<Box<dyn Plugin>>,
}

impl<S> PluginSink<S> {
    pub fn new(inner: S, plugins: Vec<Box<dyn Plugin>>) -> Self {
        Self { inner, plugins }
    }
}

fn as_kernel_event(event: &ConvertedEvent) -> KernelEvent<'_> {
    match event {
        ConvertedEvent::ModelCall(e) => KernelEvent::ModelCall(e),
        ConvertedEvent::ToolCall(e) => KernelEvent::ToolCall(e),
        ConvertedEvent::AgentRun(e) => KernelEvent::AgentRun(e),
    }
}

fn extra_attributes_mut(event: &mut ConvertedEvent) -> &mut Vec<(String, AttributeValue)> {
    match event {
        ConvertedEvent::ModelCall(e) => &mut e.extra_attributes,
        ConvertedEvent::ToolCall(e) => &mut e.extra_attributes,
        ConvertedEvent::AgentRun(e) => &mut e.extra_attributes,
    }
}

fn apply_plugins(mut event: ConvertedEvent, plugins: &[Box<dyn Plugin>]) -> ConvertedEvent {
    let mut additions: Vec<(String, AttributeValue)> = Vec::new();
    for plugin in plugins {
        let outcome = plugin.inspect(as_kernel_event(&event));
        additions.extend(outcome.attributes);
        for warning in outcome.warnings {
            additions.push((
                "plugin.warning".to_string(),
                AttributeValue::String(format!("[{}] {warning}", plugin.name())),
            ));
        }
    }
    extra_attributes_mut(&mut event).extend(additions);
    event
}

impl<S: SpanSink> SpanSink for PluginSink<S> {
    type Error = S::Error;

    async fn accept_batch(&self, events: Vec<ConvertedEvent>) -> Result<(), Self::Error> {
        let enriched: Vec<ConvertedEvent> = events
            .into_iter()
            .map(|e| apply_plugins(e, &self.plugins))
            .collect();
        self.inner.accept_batch(enriched).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel_model::{
        AgentInvocationKind, AgentRunEvent, OperationName, ProviderName, SpanContext, SpanId,
        SpanStatus, TraceId,
    };
    use otlp_receiver::InMemorySink;
    use plugin_api::PluginOutcome;

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

    struct AlwaysWarnPlugin;
    impl Plugin for AlwaysWarnPlugin {
        fn name(&self) -> &str {
            "always-warn"
        }
        fn inspect(&self, _event: KernelEvent<'_>) -> PluginOutcome {
            PluginOutcome {
                attributes: vec![("derived.flag".to_string(), AttributeValue::Bool(true))],
                warnings: vec!["something looks off".to_string()],
            }
        }
    }

    #[tokio::test]
    async fn merges_plugin_attributes_and_warnings_into_extra_attributes() {
        let sink = PluginSink::new(InMemorySink::new(), vec![Box::new(AlwaysWarnPlugin)]);

        let event = ConvertedEvent::AgentRun(AgentRunEvent {
            span: sample_span(),
            invocation_kind: AgentInvocationKind::Internal,
            operation_name: OperationName::InvokeAgent,
            agent_name: None,
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
        });

        sink.accept_batch(vec![event]).await.unwrap();
        let stored = sink.inner.drain();
        assert_eq!(stored.len(), 1);
        let ConvertedEvent::AgentRun(stored_event) = &stored[0] else {
            panic!("expected AgentRun")
        };
        assert!(
            stored_event
                .extra_attributes
                .contains(&("derived.flag".to_string(), AttributeValue::Bool(true)))
        );
        assert!(stored_event.extra_attributes.contains(&(
            "plugin.warning".to_string(),
            AttributeValue::String("[always-warn] something looks off".to_string())
        )));
    }

    #[tokio::test]
    async fn passes_through_untouched_when_no_plugins_are_registered() {
        let sink = PluginSink::new(InMemorySink::new(), vec![]);
        let event = ConvertedEvent::ModelCall(kernel_model::ModelCallEvent {
            span: sample_span(),
            provider_name: ProviderName::OpenAi,
            operation_name: OperationName::Chat,
            request_model: None,
            response_model: None,
            input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            finish_reasons: vec![],
            conversation_id: None,
            extra_attributes: vec![],
        });

        sink.accept_batch(vec![event]).await.unwrap();
        let stored = sink.inner.drain();
        let ConvertedEvent::ModelCall(stored_event) = &stored[0] else {
            panic!("expected ModelCall")
        };
        assert!(stored_event.extra_attributes.is_empty());
    }
}

//! `PluginSink<S>` — the pipeline insertion point for plugins, resolving the
//! "where to insert the plugin call in the pipeline" question left open since
//! step 5 (docs/interfaces/plugin-contract-v0.md). See
//! docs/interfaces/oncology-governance.md for why this shape (a `SpanSink`
//! decorator) was chosen over wiring plugins directly into `otlp-receiver`.
//!
//! Wraps any [`SpanSink`]: before delegating a batch to the inner sink, runs
//! every event through each plugin and merges the result into
//! `extra_attributes` — the same generic bag every other provider/domain
//! extension already lives in, no new storage column needed. Warnings become
//! `("plugin.warning", "[<plugin name>] <text>")` entries; duplicate keys are
//! fine (`extra_attributes` was designed as a `Vec`, not a `Map`, from step 1
//! specifically to tolerate this).
//!
//! **Isolation from the ingestion critical path** (2026-08-15): each plugin
//! runs on tokio's blocking pool (`spawn_blocking`) under a timeout, instead
//! of inline in the same task as the write. Before this, a single panicking
//! plugin unwound straight through `accept_batch` *before* `inner` ever ran
//! — losing the whole batch, not just that plugin's contribution, and not
//! just for the event it was inspecting. `spawn_blocking` isolates the panic
//! at the task boundary (tokio turns it into a `JoinError`, not an unwind
//! that propagates here); the timeout bounds how long one plugin can hold up
//! a batch. Both failure modes degrade to "this plugin's contribution for
//! this event is a warning", not "this batch never gets written". The write
//! itself (`inner.accept_batch`) stays synchronous in the same request —
//! deliberately not detached (deferring persistence and answering the RPC
//! before the write actually happens would change what an OTLP success
//! response means to the caller, a separate, bigger decision not made here).

use std::sync::Arc;
use std::time::Duration;

use kernel_model::AttributeValue;
use otlp_receiver::{ConvertedEvent, SpanSink};
use plugin_api::{KernelEvent, Plugin, PluginOutcome};

/// Generous relative to what a synchronous, no-I/O plugin (the only kind
/// `plugin-api`'s contract currently allows — docs/interfaces/plugin-contract-v0.md)
/// should ever need; tight enough to bound how long one misbehaving plugin
/// can hold up a batch.
const PLUGIN_TIMEOUT: Duration = Duration::from_millis(100);

pub struct PluginSink<S> {
    inner: S,
    plugins: Vec<Arc<dyn Plugin>>,
}

impl<S> PluginSink<S> {
    pub fn new(inner: S, plugins: Vec<Box<dyn Plugin>>) -> Self {
        Self {
            inner,
            plugins: plugins.into_iter().map(Arc::from).collect(),
        }
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

/// Runs one plugin against one event with panic and timeout isolation. The
/// event is cloned in so the blocking task can own a `'static` copy — the
/// borrow `KernelEvent<'a>` normally uses doesn't survive a task boundary.
async fn run_plugin_safely(plugin: Arc<dyn Plugin>, event: ConvertedEvent) -> PluginOutcome {
    let task_plugin = Arc::clone(&plugin);
    let handle = tokio::task::spawn_blocking(move || task_plugin.inspect(as_kernel_event(&event)));

    match tokio::time::timeout(PLUGIN_TIMEOUT, handle).await {
        Ok(Ok(outcome)) => outcome,
        // The blocking task panicked — spawn_blocking already isolated it
        // (this is a JoinError, not an unwind reaching us), so the rest of
        // the batch and the other plugins are unaffected.
        Ok(Err(join_err)) => PluginOutcome {
            attributes: vec![],
            warnings: vec![format!("panicked: {join_err}")],
        },
        // Still running past the deadline — can't force-kill a native
        // thread, so it keeps running on the blocking pool and its result
        // (if any) is simply discarded when it eventually finishes.
        Err(_elapsed) => PluginOutcome {
            attributes: vec![],
            warnings: vec![format!("timed out after {PLUGIN_TIMEOUT:?}")],
        },
    }
}

async fn apply_plugins(mut event: ConvertedEvent, plugins: &[Arc<dyn Plugin>]) -> ConvertedEvent {
    let mut additions: Vec<(String, AttributeValue)> = Vec::new();
    for plugin in plugins {
        let outcome = run_plugin_safely(Arc::clone(plugin), event.clone()).await;
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
        let mut enriched = Vec::with_capacity(events.len());
        for event in events {
            enriched.push(apply_plugins(event, &self.plugins).await);
        }
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

    struct PanicPlugin;
    impl Plugin for PanicPlugin {
        fn name(&self) -> &str {
            "panic-plugin"
        }
        fn inspect(&self, _event: KernelEvent<'_>) -> PluginOutcome {
            panic!("simulated plugin bug");
        }
    }

    #[tokio::test]
    async fn a_panicking_plugin_does_not_lose_the_batch() {
        // Before spawn_blocking isolation, this panic would unwind straight
        // through accept_batch — inner.accept_batch would never run, and
        // this event (and the rest of the batch) would just vanish.
        let sink = PluginSink::new(
            InMemorySink::new(),
            vec![Box::new(PanicPlugin), Box::new(AlwaysWarnPlugin)],
        );
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
        assert_eq!(stored.len(), 1, "the event must still reach the inner sink");
        let ConvertedEvent::ModelCall(stored_event) = &stored[0] else {
            panic!("expected ModelCall")
        };
        assert!(
            stored_event.extra_attributes.iter().any(
                |(k, v)| k == "plugin.warning"
                    && matches!(v, AttributeValue::String(s) if s.starts_with("[panic-plugin] panicked"))
            ),
            "expected a panic warning attributed to panic-plugin: {:?}",
            stored_event.extra_attributes
        );
        // The other plugin still ran and contributed normally.
        assert!(
            stored_event
                .extra_attributes
                .contains(&("derived.flag".to_string(), AttributeValue::Bool(true)))
        );
    }

    struct SlowPlugin;
    impl Plugin for SlowPlugin {
        fn name(&self) -> &str {
            "slow-plugin"
        }
        fn inspect(&self, _event: KernelEvent<'_>) -> PluginOutcome {
            std::thread::sleep(Duration::from_secs(2));
            PluginOutcome::default()
        }
    }

    #[tokio::test]
    async fn a_slow_plugin_times_out_instead_of_blocking_the_batch() {
        let sink = PluginSink::new(InMemorySink::new(), vec![Box::new(SlowPlugin)]);
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

        let started = std::time::Instant::now();
        sink.accept_batch(vec![event]).await.unwrap();
        // The plugin sleeps 2s; if we waited for it, this would take >=2s.
        // Bounded by PLUGIN_TIMEOUT (100ms) instead, with generous slack for
        // a loaded CI machine.
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "accept_batch should not wait for the slow plugin, took {:?}",
            started.elapsed()
        );

        let stored = sink.inner.drain();
        let ConvertedEvent::ModelCall(stored_event) = &stored[0] else {
            panic!("expected ModelCall")
        };
        assert!(
            stored_event.extra_attributes.iter().any(
                |(k, v)| k == "plugin.warning"
                    && matches!(v, AttributeValue::String(s) if s.starts_with("[slow-plugin] timed out"))
            ),
            "expected a timeout warning attributed to slow-plugin: {:?}",
            stored_event.extra_attributes
        );
    }
}

//! Loads the real compiled `.wasm` guest and checks it produces the exact
//! same `PluginOutcome` as the native `ExamplePlugin` for the same inputs —
//! proving the loading mechanism is behavior-preserving, not just that it
//! doesn't crash. Not run by default `cargo test` — requires:
//!
//!   scripts/build-wasm-plugins.sh
//!   cargo test -p plugin-wasm-host -- --ignored

use kernel_model::{
    AgentInvocationKind, AgentRunEvent, ModelCallEvent, OperationName, ProviderName, SpanContext,
    SpanId, SpanStatus, TokenCount, ToolCallEvent, TraceId,
};
use plugin_api::{KernelEvent, Plugin};
use plugin_example::ExamplePlugin;
use plugin_wasm_host::WasmPlugin;

fn wasm_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wasm32-unknown-unknown/release/plugin_wasm_example.wasm")
}

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
#[ignore = "requires `scripts/build-wasm-plugins.sh`"]
fn wasm_plugin_matches_the_native_plugin_for_a_model_call() {
    let wasm_plugin = WasmPlugin::from_file("example-wasm-plugin", wasm_path())
        .expect("failed to load .wasm — run scripts/build-wasm-plugins.sh");

    let event = ModelCallEvent {
        span: sample_span(),
        provider_name: ProviderName::AwsBedrock,
        operation_name: OperationName::Chat,
        request_model: Some("claude".to_string()),
        response_model: None,
        input_tokens: Some(TokenCount::try_from(10).unwrap()),
        output_tokens: Some(TokenCount::try_from(5).unwrap()),
        cache_read_input_tokens: None,
        cache_creation_input_tokens: None,
        finish_reasons: vec![],
        conversation_id: None,
        extra_attributes: vec![],
    };

    let native = ExamplePlugin.inspect(KernelEvent::ModelCall(&event));
    let wasm = wasm_plugin.inspect(KernelEvent::ModelCall(&event));

    assert_eq!(native, wasm);
    assert!(
        !wasm.attributes.is_empty(),
        "expected example.total_tokens to be produced"
    );
}

#[test]
#[ignore = "requires `scripts/build-wasm-plugins.sh`"]
fn wasm_plugin_matches_the_native_plugin_for_a_tool_call_missing_an_id() {
    let wasm_plugin = WasmPlugin::from_file("example-wasm-plugin", wasm_path())
        .expect("failed to load .wasm — run scripts/build-wasm-plugins.sh");

    let event = ToolCallEvent {
        span: sample_span(),
        tool_name: "Flights".to_string(),
        tool_call_id: None,
        tool_type: None,
        tool_description: None,
        agent_name: None,
        extra_attributes: vec![],
    };

    let native = ExamplePlugin.inspect(KernelEvent::ToolCall(&event));
    let wasm = wasm_plugin.inspect(KernelEvent::ToolCall(&event));

    assert_eq!(native, wasm);
    assert_eq!(
        wasm.warnings,
        vec!["tool call to `Flights` has no tool_call_id".to_string()]
    );
}

#[test]
#[ignore = "requires `scripts/build-wasm-plugins.sh`"]
fn wasm_plugin_matches_the_native_plugin_for_an_agent_run() {
    let wasm_plugin = WasmPlugin::from_file("example-wasm-plugin", wasm_path())
        .expect("failed to load .wasm — run scripts/build-wasm-plugins.sh");

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

    let native = ExamplePlugin.inspect(KernelEvent::AgentRun(&event));
    let wasm = wasm_plugin.inspect(KernelEvent::AgentRun(&event));

    assert_eq!(native, wasm);
}

#[test]
#[ignore = "requires `scripts/build-wasm-plugins.sh`"]
fn missing_wasm_file_becomes_a_load_error_not_a_panic() {
    let result = WasmPlugin::from_file("missing", "/nonexistent/path/plugin.wasm");
    assert!(result.is_err());
}

//! `plugin_api::KernelEvent`/`PluginOutcome` (borrows `kernel-model` types
//! directly) ↔ `plugin_wasm_wire` (owned, JSON-serializable). The one place
//! that knows about both shapes — same role as every other mapping layer in
//! this kernel (dossier section 2.1).

use kernel_model::{
    AgentInvocationKind, AgentRunEvent, Attribute, AttributeValue, ModelCallEvent, SpanContext,
    StatusCode, ToolCallEvent,
};
use plugin_api::{KernelEvent, PluginOutcome};
use plugin_wasm_wire::{
    WireAgentRunEvent, WireAttributeValue, WireAttributes, WireKernelEvent, WireModelCallEvent,
    WirePluginOutcome, WireSpanContext, WireToolCallEvent,
};

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn status_code_str(code: &StatusCode) -> &'static str {
    match code {
        StatusCode::Unset => "unset",
        StatusCode::Ok => "ok",
        StatusCode::Error => "error",
    }
}

fn to_wire_span(span: &SpanContext) -> WireSpanContext {
    WireSpanContext {
        trace_id: hex_encode(&span.trace_id.as_bytes()),
        span_id: hex_encode(&span.span_id.as_bytes()),
        parent_span_id: span.parent_span_id.map(|id| hex_encode(&id.as_bytes())),
        start_time_unix_nano: span.start_time_unix_nano,
        end_time_unix_nano: span.end_time_unix_nano,
        status_code: status_code_str(&span.status.code).to_string(),
        status_message: span.status.message.clone(),
        error_type: span.error_type.clone(),
    }
}

fn to_wire_value(value: &AttributeValue) -> WireAttributeValue {
    match value {
        AttributeValue::String(s) => WireAttributeValue::String(s.clone()),
        AttributeValue::Bool(b) => WireAttributeValue::Bool(*b),
        AttributeValue::Int(i) => WireAttributeValue::Int(*i),
        AttributeValue::Double(d) => WireAttributeValue::Double(*d),
        AttributeValue::Bytes(b) => WireAttributeValue::Bytes(b.clone()),
        AttributeValue::Array(items) => {
            WireAttributeValue::Array(items.iter().map(to_wire_value).collect())
        }
        AttributeValue::KeyValueList(kv) => WireAttributeValue::KeyValueList(
            kv.iter()
                .map(|(k, v)| (k.clone(), to_wire_value(v)))
                .collect(),
        ),
    }
}

fn to_wire_attrs(attrs: &[Attribute]) -> WireAttributes {
    attrs
        .iter()
        .map(|(k, v)| (k.clone(), to_wire_value(v)))
        .collect()
}

fn from_wire_value(value: WireAttributeValue) -> AttributeValue {
    match value {
        WireAttributeValue::String(s) => AttributeValue::String(s),
        WireAttributeValue::Bool(b) => AttributeValue::Bool(b),
        WireAttributeValue::Int(i) => AttributeValue::Int(i),
        WireAttributeValue::Double(d) => AttributeValue::Double(d),
        WireAttributeValue::Bytes(b) => AttributeValue::Bytes(b),
        WireAttributeValue::Array(items) => {
            AttributeValue::Array(items.into_iter().map(from_wire_value).collect())
        }
        WireAttributeValue::KeyValueList(kv) => AttributeValue::KeyValueList(
            kv.into_iter()
                .map(|(k, v)| (k, from_wire_value(v)))
                .collect(),
        ),
    }
}

fn model_call_to_wire(event: &ModelCallEvent) -> WireModelCallEvent {
    WireModelCallEvent {
        span: to_wire_span(&event.span),
        provider_name: event.provider_name.as_str().to_string(),
        operation_name: event.operation_name.as_str().to_string(),
        request_model: event.request_model.clone(),
        response_model: event.response_model.clone(),
        input_tokens: event.input_tokens.map(|t| t.get()),
        output_tokens: event.output_tokens.map(|t| t.get()),
        cache_read_input_tokens: event.cache_read_input_tokens.map(|t| t.get()),
        cache_creation_input_tokens: event.cache_creation_input_tokens.map(|t| t.get()),
        finish_reasons: event.finish_reasons.clone(),
        conversation_id: event.conversation_id.clone(),
        extra_attributes: to_wire_attrs(&event.extra_attributes),
    }
}

fn tool_call_to_wire(event: &ToolCallEvent) -> WireToolCallEvent {
    WireToolCallEvent {
        span: to_wire_span(&event.span),
        tool_name: event.tool_name.clone(),
        tool_call_id: event.tool_call_id.clone(),
        tool_type: event.tool_type.clone(),
        tool_description: event.tool_description.clone(),
        agent_name: event.agent_name.clone(),
        extra_attributes: to_wire_attrs(&event.extra_attributes),
    }
}

fn agent_run_to_wire(event: &AgentRunEvent) -> WireAgentRunEvent {
    WireAgentRunEvent {
        span: to_wire_span(&event.span),
        invocation_kind: match event.invocation_kind {
            AgentInvocationKind::Client => "client".to_string(),
            AgentInvocationKind::Internal => "internal".to_string(),
        },
        operation_name: event.operation_name.as_str().to_string(),
        agent_name: event.agent_name.clone(),
        agent_id: event.agent_id.clone(),
        agent_description: event.agent_description.clone(),
        agent_version: event.agent_version.clone(),
        request_model: event.request_model.clone(),
        provider_name: event.provider_name.as_ref().map(|p| p.as_str().to_string()),
        input_tokens: event.input_tokens.map(|t| t.get()),
        output_tokens: event.output_tokens.map(|t| t.get()),
        cache_read_input_tokens: event.cache_read_input_tokens.map(|t| t.get()),
        cache_creation_input_tokens: event.cache_creation_input_tokens.map(|t| t.get()),
        conversation_id: event.conversation_id.clone(),
        extra_attributes: to_wire_attrs(&event.extra_attributes),
    }
}

pub fn to_wire_event(event: &KernelEvent<'_>) -> WireKernelEvent {
    match event {
        KernelEvent::ModelCall(e) => WireKernelEvent::ModelCall(model_call_to_wire(e)),
        KernelEvent::ToolCall(e) => WireKernelEvent::ToolCall(tool_call_to_wire(e)),
        KernelEvent::AgentRun(e) => WireKernelEvent::AgentRun(agent_run_to_wire(e)),
    }
}

pub fn from_wire_outcome(outcome: WirePluginOutcome) -> PluginOutcome {
    PluginOutcome {
        attributes: outcome
            .attributes
            .into_iter()
            .map(|(k, v)| (k, from_wire_value(v)))
            .collect(),
        warnings: outcome.warnings,
    }
}

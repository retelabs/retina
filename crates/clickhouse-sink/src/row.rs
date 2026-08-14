//! `SpanRow` — the wide row shape backing the `spans` table
//! (migrations/0001_create_spans.sql, docs/interfaces/clickhouse-schema.md).
//! One struct for all 3 MVP event kinds, discriminated by `kind`, so a
//! trace's spans can be fetched in one query regardless of type (needed by
//! step 4: "récupérer l'arbre d'une trace").

use std::collections::HashMap;
use std::fmt;

use clickhouse::Row;
use kernel_model::{
    AgentInvocationKind, AgentRunEvent, AttributeValue, ModelCallEvent, SpanContext, StatusCode,
    ToolCallEvent,
};
use otlp_receiver::ConvertedEvent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct RowConversionError {
    field: &'static str,
    reason: String,
}

impl fmt::Display for RowConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "could not build spans row: field `{}`: {}",
            self.field, self.reason
        )
    }
}

impl std::error::Error for RowConversionError {}

#[derive(Debug, Clone, PartialEq, Row, Serialize, Deserialize)]
pub struct SpanRow {
    pub trace_id: [u8; 16],
    pub span_id: [u8; 8],
    pub parent_span_id: Option<[u8; 8]>,
    pub kind: String,

    /// Raw nanosecond ticks — `DateTime64(9, 'UTC')` (docs/interfaces/clickhouse-schema.md).
    pub start_time: i64,
    pub end_time: i64,
    pub status_code: String,
    pub status_message: String,
    pub error_type: Option<String>,

    pub operation_name: String,
    pub provider_name: Option<String>,
    pub request_model: Option<String>,
    pub response_model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub finish_reasons: Vec<String>,
    pub conversation_id: Option<String>,

    pub tool_name: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_type: Option<String>,
    pub tool_description: Option<String>,

    pub agent_invocation_kind: Option<String>,
    pub agent_name: Option<String>,
    pub agent_id: Option<String>,
    pub agent_description: Option<String>,
    pub agent_version: Option<String>,

    /// Stringified — see "extra_attributes: Map(String, String)" in
    /// docs/interfaces/clickhouse-schema.md for why this is lossy on purpose.
    pub extra_attributes: HashMap<String, String>,
}

fn nanos_to_i64(field: &'static str, nanos: u64) -> Result<i64, RowConversionError> {
    i64::try_from(nanos).map_err(|_| RowConversionError {
        field,
        reason: format!(
            "{nanos} does not fit in i64 (DateTime64 storage) — timestamp past year 2262?"
        ),
    })
}

fn status_code_str(code: &StatusCode) -> &'static str {
    match code {
        StatusCode::Unset => "unset",
        StatusCode::Ok => "ok",
        StatusCode::Error => "error",
    }
}

fn stringify_value(value: &AttributeValue) -> String {
    match value {
        AttributeValue::String(s) => s.clone(),
        AttributeValue::Bool(b) => b.to_string(),
        AttributeValue::Int(i) => i.to_string(),
        AttributeValue::Double(d) => d.to_string(),
        AttributeValue::Bytes(bytes) => bytes.iter().map(|b| format!("{b:02x}")).collect(),
        // Nested values lose structure here — acceptable per
        // docs/interfaces/clickhouse-schema.md (only opt_in, PII-gated
        // attributes are ever nested for the MVP's 3 event kinds).
        nested @ (AttributeValue::Array(_) | AttributeValue::KeyValueList(_)) => {
            format!("{nested:?}")
        }
    }
}

fn stringify_attrs(attrs: Vec<(String, AttributeValue)>) -> HashMap<String, String> {
    attrs
        .into_iter()
        .map(|(k, v)| (k, stringify_value(&v)))
        .collect()
}

struct CommonFields {
    trace_id: [u8; 16],
    span_id: [u8; 8],
    parent_span_id: Option<[u8; 8]>,
    start_time: i64,
    end_time: i64,
    status_code: String,
    status_message: String,
    error_type: Option<String>,
}

fn common_fields(span: &SpanContext) -> Result<CommonFields, RowConversionError> {
    Ok(CommonFields {
        trace_id: span.trace_id.as_bytes(),
        span_id: span.span_id.as_bytes(),
        parent_span_id: span.parent_span_id.map(|id| id.as_bytes()),
        start_time: nanos_to_i64("start_time_unix_nano", span.start_time_unix_nano)?,
        end_time: nanos_to_i64("end_time_unix_nano", span.end_time_unix_nano)?,
        status_code: status_code_str(&span.status.code).to_string(),
        status_message: span.status.message.clone(),
        error_type: span.error_type.clone(),
    })
}

impl TryFrom<ModelCallEvent> for SpanRow {
    type Error = RowConversionError;

    fn try_from(event: ModelCallEvent) -> Result<Self, Self::Error> {
        let c = common_fields(&event.span)?;
        Ok(SpanRow {
            trace_id: c.trace_id,
            span_id: c.span_id,
            parent_span_id: c.parent_span_id,
            kind: "model_call".to_string(),
            start_time: c.start_time,
            end_time: c.end_time,
            status_code: c.status_code,
            status_message: c.status_message,
            error_type: c.error_type,
            operation_name: event.operation_name.as_str().to_string(),
            provider_name: Some(event.provider_name.as_str().to_string()),
            request_model: event.request_model,
            response_model: event.response_model,
            input_tokens: event.input_tokens.map(|t| t.get()),
            output_tokens: event.output_tokens.map(|t| t.get()),
            cache_read_input_tokens: event.cache_read_input_tokens.map(|t| t.get()),
            cache_creation_input_tokens: event.cache_creation_input_tokens.map(|t| t.get()),
            finish_reasons: event.finish_reasons,
            conversation_id: event.conversation_id,
            tool_name: None,
            tool_call_id: None,
            tool_type: None,
            tool_description: None,
            agent_invocation_kind: None,
            agent_name: None,
            agent_id: None,
            agent_description: None,
            agent_version: None,
            extra_attributes: stringify_attrs(event.extra_attributes),
        })
    }
}

impl TryFrom<ToolCallEvent> for SpanRow {
    type Error = RowConversionError;

    fn try_from(event: ToolCallEvent) -> Result<Self, Self::Error> {
        let c = common_fields(&event.span)?;
        Ok(SpanRow {
            trace_id: c.trace_id,
            span_id: c.span_id,
            parent_span_id: c.parent_span_id,
            kind: "tool_call".to_string(),
            start_time: c.start_time,
            end_time: c.end_time,
            status_code: c.status_code,
            status_message: c.status_message,
            error_type: c.error_type,
            operation_name: "execute_tool".to_string(),
            provider_name: None,
            request_model: None,
            response_model: None,
            input_tokens: None,
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            finish_reasons: Vec::new(),
            conversation_id: None,
            tool_name: Some(event.tool_name),
            tool_call_id: event.tool_call_id,
            tool_type: event.tool_type,
            tool_description: event.tool_description,
            agent_invocation_kind: None,
            agent_name: event.agent_name,
            agent_id: None,
            agent_description: None,
            agent_version: None,
            extra_attributes: stringify_attrs(event.extra_attributes),
        })
    }
}

impl TryFrom<AgentRunEvent> for SpanRow {
    type Error = RowConversionError;

    fn try_from(event: AgentRunEvent) -> Result<Self, Self::Error> {
        let c = common_fields(&event.span)?;
        let agent_invocation_kind = match event.invocation_kind {
            AgentInvocationKind::Client => "client",
            AgentInvocationKind::Internal => "internal",
        };
        Ok(SpanRow {
            trace_id: c.trace_id,
            span_id: c.span_id,
            parent_span_id: c.parent_span_id,
            kind: "agent_run".to_string(),
            start_time: c.start_time,
            end_time: c.end_time,
            status_code: c.status_code,
            status_message: c.status_message,
            error_type: c.error_type,
            operation_name: event.operation_name.as_str().to_string(),
            provider_name: event.provider_name.map(|p| p.as_str().to_string()),
            request_model: event.request_model,
            response_model: None,
            input_tokens: event.input_tokens.map(|t| t.get()),
            output_tokens: event.output_tokens.map(|t| t.get()),
            cache_read_input_tokens: event.cache_read_input_tokens.map(|t| t.get()),
            cache_creation_input_tokens: event.cache_creation_input_tokens.map(|t| t.get()),
            finish_reasons: Vec::new(),
            conversation_id: event.conversation_id,
            tool_name: None,
            tool_call_id: None,
            tool_type: None,
            tool_description: None,
            agent_invocation_kind: Some(agent_invocation_kind.to_string()),
            agent_name: event.agent_name,
            agent_id: event.agent_id,
            agent_description: event.agent_description,
            agent_version: event.agent_version,
            extra_attributes: stringify_attrs(event.extra_attributes),
        })
    }
}

impl TryFrom<ConvertedEvent> for SpanRow {
    type Error = RowConversionError;

    fn try_from(event: ConvertedEvent) -> Result<Self, Self::Error> {
        match event {
            ConvertedEvent::ModelCall(e) => e.try_into(),
            ConvertedEvent::ToolCall(e) => e.try_into(),
            ConvertedEvent::AgentRun(e) => e.try_into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel_model::{OperationName, ProviderName, SpanId, SpanStatus, TokenCount, TraceId};

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
    fn model_call_event_maps_to_a_row_with_the_right_kind_and_promoted_fields() {
        let event = ModelCallEvent {
            span: sample_span(),
            provider_name: ProviderName::AwsBedrock,
            operation_name: OperationName::Chat,
            request_model: Some("claude".to_string()),
            response_model: None,
            input_tokens: Some(TokenCount::try_from(42).unwrap()),
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            finish_reasons: vec!["stop".to_string()],
            conversation_id: None,
            extra_attributes: vec![(
                "aws.bedrock.guardrail.id".to_string(),
                AttributeValue::String("gr1".to_string()),
            )],
        };

        let row = SpanRow::try_from(event).unwrap();
        assert_eq!(row.kind, "model_call");
        assert_eq!(row.provider_name.as_deref(), Some("aws.bedrock"));
        assert_eq!(row.input_tokens, Some(42));
        assert_eq!(row.start_time, 1_000);
        assert_eq!(row.end_time, 1_500);
        assert_eq!(
            row.extra_attributes.get("aws.bedrock.guardrail.id"),
            Some(&"gr1".to_string())
        );
        // fields that don't apply to this event kind stay empty
        assert!(row.tool_name.is_none());
        assert!(row.agent_name.is_none());
    }

    #[test]
    fn tool_call_event_maps_to_a_row() {
        let event = ToolCallEvent {
            span: sample_span(),
            tool_name: "Flights".to_string(),
            tool_call_id: Some("call_1".to_string()),
            tool_type: Some("function".to_string()),
            tool_description: None,
            agent_name: Some("triage-agent".to_string()),
            extra_attributes: vec![],
        };

        let row = SpanRow::try_from(event).unwrap();
        assert_eq!(row.kind, "tool_call");
        assert_eq!(row.tool_name.as_deref(), Some("Flights"));
        assert_eq!(row.agent_name.as_deref(), Some("triage-agent"));
        assert!(row.provider_name.is_none());
    }

    #[test]
    fn nanos_to_i64_rejects_values_that_overflow_i64() {
        let err = nanos_to_i64("start_time_unix_nano", u64::MAX);
        assert!(err.is_err());
    }

    #[test]
    fn stringify_value_hex_encodes_bytes() {
        assert_eq!(
            stringify_value(&AttributeValue::Bytes(vec![0xde, 0xad])),
            "dead"
        );
    }
}

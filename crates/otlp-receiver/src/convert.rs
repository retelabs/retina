//! Maps wire-shaped OTLP spans (crate::proto, from vendor/opentelemetry-proto
//! @ v1.11.0) onto the kernel's internal model (kernel_model, derived from
//! vendor/semconv-genai @ 30182acd). This is the "couche de mapping/adaptateur"
//! required by dossier section 2.1 — the one place that knows about both wire
//! shapes, so a future semconv or OTLP version bump only touches this file.

use kernel_model::{
    AgentInvocationKind, AgentRunEvent, Attribute, AttributeValue, ModelCallEvent, ModelError,
    OperationName, ProviderName, SpanContext, SpanId, SpanStatus, StatusCode, TokenCount,
    ToolCallEvent, TraceId,
};

use crate::proto::opentelemetry::proto::common::v1::any_value::Value as ProtoValue;
use crate::proto::opentelemetry::proto::common::v1::{
    AnyValue as ProtoAnyValue, KeyValue as ProtoKeyValue,
};
use crate::proto::opentelemetry::proto::trace::v1::span::SpanKind;
use crate::proto::opentelemetry::proto::trace::v1::{Span as ProtoSpan, Status as ProtoStatus};

#[derive(Debug, Clone, PartialEq)]
pub enum ConvertedEvent {
    ModelCall(ModelCallEvent),
    ToolCall(ToolCallEvent),
    AgentRun(AgentRunEvent),
}

#[derive(Debug, PartialEq)]
pub enum ConvertError {
    /// A structural/required-field problem — the span is malformed per
    /// docs/interfaces/otlp-ingestion.md or docs/interfaces/semconv-genai.md.
    Malformed(ModelError),
    /// Structurally valid, but not one of the 3 MVP event types (dossier
    /// section 4: retrieval/memory/workflow/embeddings/... are out of scope).
    /// Not an error the sender caused — the receiver should count it
    /// separately from rejects, not report it as `rejected_spans`.
    Unmodeled { operation_name: Option<String> },
}

impl From<ModelError> for ConvertError {
    fn from(e: ModelError) -> Self {
        ConvertError::Malformed(e)
    }
}

/// Converts one `AnyValue` (proto) into [`AttributeValue`] (kernel). Returns
/// `None` for the empty oneof case and for the Profiling-only Alpha variant
/// `string_value_strindex` — per docs/interfaces/otlp-ingestion.md, a traces
/// receiver must treat the latter as a non-fatal anomaly, not interpret it.
fn convert_value(value: ProtoAnyValue) -> Option<AttributeValue> {
    match value.value? {
        ProtoValue::StringValue(s) => Some(AttributeValue::String(s)),
        ProtoValue::BoolValue(b) => Some(AttributeValue::Bool(b)),
        ProtoValue::IntValue(i) => Some(AttributeValue::Int(i)),
        ProtoValue::DoubleValue(d) => Some(AttributeValue::Double(d)),
        ProtoValue::BytesValue(b) => Some(AttributeValue::Bytes(b)),
        ProtoValue::ArrayValue(arr) => Some(AttributeValue::Array(
            arr.values.into_iter().filter_map(convert_value).collect(),
        )),
        ProtoValue::KvlistValue(kv) => {
            Some(AttributeValue::KeyValueList(convert_attributes(kv.values)))
        }
        ProtoValue::StringValueStrindex(_) => None,
    }
}

/// Converts a full attribute list. Duplicate keys are preserved as-is here
/// (this is the passthrough bag) — the "last occurrence wins" policy
/// documented in docs/interfaces/otlp-ingestion.md only applies at lookup
/// time, via [`find_last`], not by mutating the list itself.
fn convert_attributes(attrs: Vec<ProtoKeyValue>) -> Vec<Attribute> {
    attrs
        .into_iter()
        .filter_map(|kv| {
            let value = convert_value(kv.value?)?;
            Some((kv.key, value))
        })
        .collect()
}

/// Looks up `key`, returning the *last* matching entry — the duplicate-key
/// policy decided in docs/interfaces/otlp-ingestion.md ("comportement
/// imprévisible" left to the receiver by OTLP itself).
fn find_last<'a>(attrs: &'a [Attribute], key: &str) -> Option<&'a AttributeValue> {
    attrs.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn take_str(attrs: &[Attribute], key: &str) -> Option<String> {
    find_last(attrs, key)
        .and_then(AttributeValue::as_str)
        .map(str::to_owned)
}

/// Malformed (negative) token counts are dropped rather than failing the
/// whole span: `gen_ai.usage.*` is `recommended`, not `required`, in the
/// pinned semconv (docs/interfaces/semconv-genai.md) — a bad optional field
/// shouldn't reject an otherwise-valid span.
fn take_token_count(attrs: &[Attribute], key: &str) -> Option<TokenCount> {
    find_last(attrs, key)
        .and_then(AttributeValue::as_int)
        .and_then(|i| TokenCount::try_from(i).ok())
}

/// `gen_ai.response.finish_reasons` is `string[]` (docs/interfaces/semconv-genai.md)
/// — an `AttributeValue::Array` of `AttributeValue::String`. Non-string
/// elements are dropped rather than failing the whole span, same rationale as
/// [`take_token_count`].
fn take_str_array(attrs: &[Attribute], key: &str) -> Vec<String> {
    match find_last(attrs, key) {
        Some(AttributeValue::Array(items)) => items
            .iter()
            .filter_map(AttributeValue::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn convert_status(status: Option<ProtoStatus>) -> SpanStatus {
    let Some(status) = status else {
        return SpanStatus::default();
    };
    let code = match status.code {
        1 => StatusCode::Ok,
        2 => StatusCode::Error,
        _ => StatusCode::Unset,
    };
    SpanStatus {
        code,
        message: status.message,
    }
}

fn convert_span_context(
    span: &ProtoSpan,
    attributes: Vec<Attribute>,
) -> Result<(SpanContext, Vec<Attribute>), ConvertError> {
    let trace_id = TraceId::try_from(span.trace_id.as_slice())?;
    let span_id = SpanId::try_from(span.span_id.as_slice())?;
    let parent_span_id = if span.parent_span_id.is_empty() {
        None
    } else {
        Some(SpanId::try_from(span.parent_span_id.as_slice())?)
    };
    let error_type = take_str(&attributes, "error.type");
    let ctx = SpanContext {
        trace_id,
        span_id,
        parent_span_id,
        start_time_unix_nano: span.start_time_unix_nano,
        end_time_unix_nano: span.end_time_unix_nano,
        status: convert_status(span.status.clone()),
        error_type,
    };
    Ok((ctx, attributes))
}

const MODEL_CALL_KNOWN_KEYS: &[&str] = &[
    "gen_ai.operation.name",
    "gen_ai.provider.name",
    "gen_ai.request.model",
    "gen_ai.response.model",
    "gen_ai.usage.input_tokens",
    "gen_ai.usage.output_tokens",
    "gen_ai.usage.cache_read.input_tokens",
    "gen_ai.usage.cache_creation.input_tokens",
    "gen_ai.response.finish_reasons",
    "gen_ai.conversation.id",
    "error.type",
];

const TOOL_CALL_KNOWN_KEYS: &[&str] = &[
    "gen_ai.operation.name",
    "gen_ai.tool.name",
    "gen_ai.tool.call.id",
    "gen_ai.tool.type",
    "gen_ai.tool.description",
    "gen_ai.agent.name",
    "error.type",
];

const AGENT_RUN_KNOWN_KEYS: &[&str] = &[
    "gen_ai.operation.name",
    "gen_ai.agent.name",
    "gen_ai.agent.id",
    "gen_ai.agent.description",
    "gen_ai.agent.version",
    "gen_ai.request.model",
    "gen_ai.provider.name",
    "gen_ai.usage.input_tokens",
    "gen_ai.usage.output_tokens",
    "gen_ai.usage.cache_read.input_tokens",
    "gen_ai.usage.cache_creation.input_tokens",
    "gen_ai.conversation.id",
    "error.type",
];

fn extra_attributes(attributes: Vec<Attribute>, known: &[&str]) -> Vec<Attribute> {
    attributes
        .into_iter()
        .filter(|(k, _)| !known.contains(&k.as_str()))
        .collect()
}

/// Entry point: converts one OTLP `Span` into a kernel event, or reports why
/// it couldn't. Dispatch is driven by `gen_ai.operation.name` — OTLP carries
/// no dedicated "this is a gen_ai span" marker on the wire, so the attribute
/// value is the only signal available at ingestion time.
pub fn convert_span(span: &ProtoSpan) -> Result<ConvertedEvent, ConvertError> {
    let attributes = convert_attributes(span.attributes.clone());
    let (ctx, attributes) = convert_span_context(span, attributes)?;

    let operation_name_str = take_str(&attributes, "gen_ai.operation.name");
    let operation_name = match &operation_name_str {
        Some(s) => OperationName::from(s.as_str()),
        None => {
            return Err(ConvertError::Unmodeled {
                operation_name: None,
            });
        }
    };

    match operation_name {
        OperationName::Chat | OperationName::GenerateContent | OperationName::TextCompletion => {
            let provider_name = take_str(&attributes, "gen_ai.provider.name")
                .ok_or(ModelError::MissingRequiredField("gen_ai.provider.name"))?;
            Ok(ConvertedEvent::ModelCall(ModelCallEvent {
                span: ctx,
                provider_name: ProviderName::from(provider_name.as_str()),
                operation_name,
                request_model: take_str(&attributes, "gen_ai.request.model"),
                response_model: take_str(&attributes, "gen_ai.response.model"),
                input_tokens: take_token_count(&attributes, "gen_ai.usage.input_tokens"),
                output_tokens: take_token_count(&attributes, "gen_ai.usage.output_tokens"),
                cache_read_input_tokens: take_token_count(
                    &attributes,
                    "gen_ai.usage.cache_read.input_tokens",
                ),
                cache_creation_input_tokens: take_token_count(
                    &attributes,
                    "gen_ai.usage.cache_creation.input_tokens",
                ),
                finish_reasons: take_str_array(&attributes, "gen_ai.response.finish_reasons"),
                conversation_id: take_str(&attributes, "gen_ai.conversation.id"),
                extra_attributes: extra_attributes(attributes, MODEL_CALL_KNOWN_KEYS),
            }))
        }
        OperationName::ExecuteTool => Ok(ConvertedEvent::ToolCall(ToolCallEvent {
            span: ctx,
            tool_name: take_str(&attributes, "gen_ai.tool.name")
                .ok_or(ModelError::MissingRequiredField("gen_ai.tool.name"))?,
            tool_call_id: take_str(&attributes, "gen_ai.tool.call.id"),
            tool_type: take_str(&attributes, "gen_ai.tool.type"),
            tool_description: take_str(&attributes, "gen_ai.tool.description"),
            agent_name: take_str(&attributes, "gen_ai.agent.name"),
            extra_attributes: extra_attributes(attributes, TOOL_CALL_KNOWN_KEYS),
        })),
        OperationName::InvokeAgent => {
            let invocation_kind = match span.kind() {
                SpanKind::Client => AgentInvocationKind::Client,
                _ => AgentInvocationKind::Internal,
            };
            let event = AgentRunEvent {
                span: ctx,
                invocation_kind,
                operation_name,
                agent_name: take_str(&attributes, "gen_ai.agent.name"),
                agent_id: take_str(&attributes, "gen_ai.agent.id"),
                agent_description: take_str(&attributes, "gen_ai.agent.description"),
                agent_version: take_str(&attributes, "gen_ai.agent.version"),
                request_model: take_str(&attributes, "gen_ai.request.model"),
                provider_name: take_str(&attributes, "gen_ai.provider.name")
                    .map(|s| ProviderName::from(s.as_str())),
                input_tokens: take_token_count(&attributes, "gen_ai.usage.input_tokens"),
                output_tokens: take_token_count(&attributes, "gen_ai.usage.output_tokens"),
                cache_read_input_tokens: take_token_count(
                    &attributes,
                    "gen_ai.usage.cache_read.input_tokens",
                ),
                cache_creation_input_tokens: take_token_count(
                    &attributes,
                    "gen_ai.usage.cache_creation.input_tokens",
                ),
                conversation_id: take_str(&attributes, "gen_ai.conversation.id"),
                extra_attributes: extra_attributes(attributes, AGENT_RUN_KNOWN_KEYS),
            };
            event.validate()?;
            Ok(ConvertedEvent::AgentRun(event))
        }
        OperationName::Other(s) => Err(ConvertError::Unmodeled {
            operation_name: Some(s),
        }),
        other => Err(ConvertError::Unmodeled {
            operation_name: Some(other.as_str().to_string()),
        }),
    }
}

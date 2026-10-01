//! `AgentSpan` → OTLP `ExportTraceServiceRequest`. Best-effort reconstruction,
//! not a faithful conversion — see docs/interfaces/fraudos-agentspan.md for
//! exactly what's approximated and why (no per-turn granularity exists in
//! the source).

use chrono::DateTime;
use otlp_receiver::proto::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;
use otlp_receiver::proto::opentelemetry::proto::common::v1::any_value::Value as ProtoValue;
use otlp_receiver::proto::opentelemetry::proto::common::v1::{
    AnyValue, InstrumentationScope, KeyValue,
};
use otlp_receiver::proto::opentelemetry::proto::resource::v1::Resource;
use otlp_receiver::proto::opentelemetry::proto::trace::v1::span::SpanKind;
use otlp_receiver::proto::opentelemetry::proto::trace::v1::status::StatusCode;
use otlp_receiver::proto::opentelemetry::proto::trace::v1::{
    ResourceSpans, ScopeSpans, Span, Status,
};

use crate::agent_span::AgentSpan;
use crate::ids::{derive_span_id, derive_trace_id};

fn kv(key: &str, value: ProtoValue) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue { value: Some(value) }),
        ..Default::default()
    }
}

fn str_attr(key: &str, value: &str) -> KeyValue {
    kv(key, ProtoValue::StringValue(value.to_string()))
}

fn int_attr(key: &str, value: u64) -> KeyValue {
    // u64 -> i64: same reflex as docs/interfaces/semconv-genai.md — checked,
    // not a bare `as`. Token/cost counts here are far below i64::MAX.
    kv(
        key,
        ProtoValue::IntValue(i64::try_from(value).unwrap_or(i64::MAX)),
    )
}

fn bool_attr(key: &str, value: bool) -> KeyValue {
    kv(key, ProtoValue::BoolValue(value))
}

fn double_attr(key: &str, value: f64) -> KeyValue {
    kv(key, ProtoValue::DoubleValue(value))
}

fn parse_unix_nanos(iso8601: &str) -> Result<u64, String> {
    let dt = DateTime::parse_from_rfc3339(iso8601)
        .map_err(|e| format!("invalid timestamp `{iso8601}`: {e}"))?;
    let nanos = dt
        .timestamp_nanos_opt()
        .ok_or_else(|| format!("timestamp `{iso8601}` out of representable range"))?;
    u64::try_from(nanos).map_err(|_| format!("timestamp `{iso8601}` predates the Unix epoch"))
}

/// Converts one `AgentSpan` into a 3-span micro-trace: root
/// `gen_ai.invoke_agent.internal`, one aggregate `gen_ai.inference.client`,
/// and one `gen_ai.execute_tool.internal` per entry in `tools_called`.
pub fn convert(span: &AgentSpan) -> Result<ExportTraceServiceRequest, String> {
    let trace_id = derive_trace_id(&span.session_id).to_vec();
    let start = parse_unix_nanos(&span.started_at)?;
    let end = parse_unix_nanos(&span.ended_at)?;
    let root_span_id = derive_span_id(&span.session_id, "root");

    let mut root_attributes = vec![
        str_attr("gen_ai.operation.name", "invoke_agent"),
        str_attr("gen_ai.agent.name", &span.agent_role),
        int_attr("gen_ai.usage.input_tokens", span.total_input_tokens),
        int_attr("gen_ai.usage.output_tokens", span.total_output_tokens),
        str_attr("fraudos.task_summary", &span.task_summary),
        bool_attr("fraudos.escalated_to_opus", span.escalated_to_opus),
        bool_attr("fraudos.requires_human_review", span.requires_human_review),
        double_attr("fraudos.estimated_cost_usd", span.estimated_cost_usd),
    ];
    for (key, value) in [
        ("fraudos.case_id", &span.case_id),
        ("fraudos.bank_id", &span.bank_id),
        ("fraudos.transaction_id", &span.transaction_id),
        ("fraudos.final_decision", &span.final_decision),
    ] {
        if let Some(value) = value {
            root_attributes.push(str_attr(key, value));
        }
    }

    let root = Span {
        trace_id: trace_id.clone(),
        span_id: root_span_id.to_vec(),
        parent_span_id: vec![],
        name: format!("invoke_agent {}", span.agent_role),
        kind: SpanKind::Internal as i32,
        start_time_unix_nano: start,
        end_time_unix_nano: end,
        attributes: root_attributes,
        status: Some(Status {
            code: if span.success {
                StatusCode::Ok
            } else {
                StatusCode::Error
            } as i32,
            message: span.final_decision.clone().unwrap_or_default(),
        }),
        ..Default::default()
    };

    let model_call_span_id = derive_span_id(&span.session_id, "model_call");
    let model_call = Span {
        trace_id: trace_id.clone(),
        span_id: model_call_span_id.to_vec(),
        parent_span_id: root_span_id.to_vec(),
        name: format!("chat {}", span.primary_model_id),
        kind: SpanKind::Client as i32,
        start_time_unix_nano: start,
        end_time_unix_nano: end,
        attributes: vec![
            str_attr("gen_ai.operation.name", "chat"),
            str_attr("gen_ai.provider.name", "aws.bedrock"),
            str_attr("gen_ai.request.model", &span.primary_model_id),
            int_attr("gen_ai.usage.input_tokens", span.total_input_tokens),
            int_attr("gen_ai.usage.output_tokens", span.total_output_tokens),
        ],
        ..Default::default()
    };

    let mut spans = vec![root, model_call];
    for (i, tool_name) in span.tools_called.iter().enumerate() {
        let tool_span_id = derive_span_id(&span.session_id, &format!("tool-{i}-{tool_name}"));
        let failed = span.tools_failed.contains(tool_name);
        spans.push(Span {
            trace_id: trace_id.clone(),
            span_id: tool_span_id.to_vec(),
            parent_span_id: root_span_id.to_vec(),
            name: format!("execute_tool {tool_name}"),
            kind: SpanKind::Internal as i32,
            start_time_unix_nano: start,
            end_time_unix_nano: end,
            attributes: vec![
                str_attr("gen_ai.operation.name", "execute_tool"),
                str_attr("gen_ai.tool.name", tool_name),
            ],
            status: Some(Status {
                code: if failed {
                    StatusCode::Error
                } else {
                    StatusCode::Unset
                } as i32,
                message: if failed {
                    format!("tool `{tool_name}` failed or was denied")
                } else {
                    String::new()
                },
            }),
            ..Default::default()
        });
    }

    Ok(ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![
                    str_attr("service.name", "fraudos"),
                    str_attr("service.namespace", "fraudos"),
                ],
                ..Default::default()
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "fraudos-replay".to_string(),
                    ..Default::default()
                }),
                spans,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_fixture(name: &str) -> AgentSpan {
        let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parsing {path}: {e}"))
    }

    #[test]
    fn converts_a_confirmed_fraud_case_into_a_valid_micro_trace() {
        let agent_span = load_fixture("fraud_investigator_confirmed.json");
        let request = convert(&agent_span).expect("conversion should succeed");

        let spans = &request.resource_spans[0].scope_spans[0].spans;
        // root (invoke_agent) + 1 aggregate model_call + 4 tools_called
        assert_eq!(spans.len(), 6);

        let root = &spans[0];
        assert!(
            root.parent_span_id.is_empty(),
            "root span must have no parent"
        );
        assert_eq!(
            root.trace_id.len(),
            16,
            "trace_id must be exactly 16 bytes (kernel_model::TraceId)"
        );
        assert_eq!(
            root.span_id.len(),
            8,
            "span_id must be exactly 8 bytes (kernel_model::SpanId)"
        );
        assert_ne!(
            root.trace_id,
            vec![0u8; 16],
            "trace_id must not be all-zero"
        );

        // every non-root span must be parented under the root, and must
        // share the same trace_id — otherwise kernel_model would build 6
        // disconnected single-span traces instead of one 6-span trace.
        for span in &spans[1..] {
            assert_eq!(span.trace_id, root.trace_id);
            assert_eq!(span.parent_span_id, root.span_id);
        }
    }

    #[test]
    fn marks_the_failed_tool_call_as_error() {
        let agent_span = load_fixture("compliance_officer_dismissed.json");
        let request = convert(&agent_span).expect("conversion should succeed");
        let spans = &request.resource_spans[0].scope_spans[0].spans;

        let failed_tool_span = spans
            .iter()
            .find(|s| s.name == "execute_tool check_existing_reports")
            .expect("expected a span for the failed tool");
        let status = failed_tool_span.status.as_ref().expect("expected a status");
        assert_eq!(status.code, StatusCode::Error as i32);
    }

    #[test]
    fn rejects_an_unparseable_timestamp_instead_of_panicking() {
        let mut agent_span = load_fixture("fraud_investigator_confirmed.json");
        agent_span.started_at = "not-a-timestamp".to_string();
        assert!(convert(&agent_span).is_err());
    }
}

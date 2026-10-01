//! `OncologyRun` → OTLP. See docs/interfaces/oncology-governance.md for the
//! mapping decision: 1 root `gen_ai.invoke_agent.internal` span + 1 child
//! `gen_ai.inference.client` span *only if* the run reached or passed the
//! `recommendation` step (that's the only real LLM call in this pipeline —
//! unlike fraudos, there is no per-tool-call structure to reconstruct here).

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

use crate::ids::{derive_span_id, derive_trace_id};
use crate::oncology_run::OncologyRun;

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

fn bool_attr(key: &str, value: bool) -> KeyValue {
    kv(key, ProtoValue::BoolValue(value))
}

fn parse_unix_nanos(iso8601: &str) -> Result<u64, String> {
    let dt = DateTime::parse_from_rfc3339(iso8601)
        .map_err(|e| format!("invalid timestamp `{iso8601}`: {e}"))?;
    let nanos = dt
        .timestamp_nanos_opt()
        .ok_or_else(|| format!("timestamp `{iso8601}` out of representable range"))?;
    u64::try_from(nanos).map_err(|_| format!("timestamp `{iso8601}` predates the Unix epoch"))
}

/// Whether `recommendation_node` (the one real LLM call) has actually *run*.
/// `current_step == "recommendation"` does NOT qualify: that value is set by
/// `visualization_node`'s own output, right before `interrupt_before=["recommendation"]`
/// pauses the graph — i.e. it means "about to call the LLM, hasn't yet"
/// (verified by reading both `visualization_node` and `recommendation_node`
/// in `src/agents/oncology_pipeline.py`, not assumed from the name). Only
/// `recommendation_node`'s own output (`current_step: "monitoring"`) or
/// later means the call happened.
fn reached_recommendation(current_step: &str) -> bool {
    matches!(current_step, "monitoring" | "done")
}

pub fn convert(run: &OncologyRun) -> Result<ExportTraceServiceRequest, String> {
    let trace_id = derive_trace_id(&run.session_id).to_vec();
    let start = parse_unix_nanos(&run.started_at)?;
    let end = parse_unix_nanos(&run.ended_at)?;
    let root_span_id = derive_span_id(&run.session_id, "root");

    let mut root_attributes = vec![
        str_attr("gen_ai.operation.name", "invoke_agent"),
        str_attr("gen_ai.agent.name", "oncology_pipeline"),
        str_attr("oncology.current_step", &run.current_step),
        str_attr("oncology.patient_id", &run.patient_id),
        bool_attr("oncology.hipaa_cleared", run.hipaa_cleared),
        bool_attr("oncology.gdpr_cleared", run.gdpr_cleared),
        str_attr("oncology.compliance_flags", &run.compliance_flags.join(";")),
    ];
    if let Some(submitted_by) = &run.submitted_by {
        root_attributes.push(str_attr("oncology.submitted_by", submitted_by));
    }
    if let Some(approved_by) = &run.approved_by {
        root_attributes.push(str_attr("oncology.approved_by", approved_by));
    }

    let root_ok = run.current_step != "failed";
    let root = Span {
        trace_id: trace_id.clone(),
        span_id: root_span_id.to_vec(),
        parent_span_id: vec![],
        name: "invoke_agent oncology_pipeline".to_string(),
        kind: SpanKind::Internal as i32,
        start_time_unix_nano: start,
        end_time_unix_nano: end,
        attributes: root_attributes,
        status: Some(Status {
            code: if root_ok {
                StatusCode::Ok
            } else {
                StatusCode::Error
            } as i32,
            message: run.current_step.clone(),
        }),
        ..Default::default()
    };

    let mut spans = vec![root];

    if reached_recommendation(&run.current_step) {
        let model_call_span_id = derive_span_id(&run.session_id, "model_call");
        spans.push(Span {
            trace_id: trace_id.clone(),
            span_id: model_call_span_id.to_vec(),
            parent_span_id: root_span_id.to_vec(),
            name: "chat gpt-4o".to_string(),
            kind: SpanKind::Client as i32,
            start_time_unix_nano: start,
            end_time_unix_nano: end,
            attributes: vec![
                str_attr("gen_ai.operation.name", "chat"),
                str_attr("gen_ai.provider.name", "openai"),
                str_attr("gen_ai.request.model", "gpt-4o"),
                // No token counts: the source doesn't capture LLM usage
                // metadata here (docs/interfaces/oncology-governance.md) —
                // omitted, not invented.
            ],
            ..Default::default()
        });
    }

    Ok(ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![
                    str_attr("service.name", "oncology-suite"),
                    str_attr("service.namespace", "oncology"),
                ],
                ..Default::default()
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "oncology-replay".to_string(),
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

    fn load_fixture(name: &str) -> OncologyRun {
        let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parsing {path}: {e}"))
    }

    #[test]
    fn approved_run_produces_root_plus_model_call_span() {
        let run = load_fixture("approved_recommendation.json");
        let request = convert(&run).expect("conversion should succeed");
        let spans = &request.resource_spans[0].scope_spans[0].spans;
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[1].parent_span_id, spans[0].span_id);
    }

    #[test]
    fn pending_approval_run_produces_only_the_root_span() {
        // current_step = "recommendation" means the graph is paused there
        // (interrupt_before) — the LLM call hasn't actually run yet, so no
        // model_call span should be synthesized.
        let run = load_fixture("pending_hitl_approval.json");
        let request = convert(&run).expect("conversion should succeed");
        let spans = &request.resource_spans[0].scope_spans[0].spans;
        assert_eq!(spans.len(), 1);
    }
}

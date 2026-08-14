//! OTLP/traces receiver (dossier section 2.2, étape 2). Accepts, validates
//! and hands off spans — persistence (étape 3) is behind the [`SpanSink`]
//! seam, not decided here. Contracts this crate implements are documented in
//! docs/interfaces/otlp-ingestion.md and docs/interfaces/semconv-genai.md.

pub mod convert;
pub mod proto;
pub mod service;
pub mod sink;

pub use convert::{ConvertError, ConvertedEvent, convert_span};
pub use proto::opentelemetry::proto::collector::trace::v1::trace_service_client::TraceServiceClient;
pub use proto::opentelemetry::proto::collector::trace::v1::trace_service_server::TraceServiceServer;
pub use service::Receiver;
pub use sink::{InMemorySink, SpanSink};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::opentelemetry::proto::collector::trace::v1::ExportTraceServiceRequest;
    use crate::proto::opentelemetry::proto::collector::trace::v1::trace_service_server::TraceService as _;
    use crate::proto::opentelemetry::proto::common::v1::any_value::Value as ProtoValue;
    use crate::proto::opentelemetry::proto::common::v1::{AnyValue, KeyValue};
    use crate::proto::opentelemetry::proto::resource::v1::Resource;
    use crate::proto::opentelemetry::proto::trace::v1::span::SpanKind;
    use crate::proto::opentelemetry::proto::trace::v1::{ResourceSpans, ScopeSpans, Span};
    use kernel_model::ProviderName;

    fn attr(key: &str, value: ProtoValue) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue { value: Some(value) }),
            ..Default::default()
        }
    }

    fn model_call_span(trace_id: [u8; 16], span_id: [u8; 8]) -> Span {
        Span {
            trace_id: trace_id.to_vec(),
            span_id: span_id.to_vec(),
            parent_span_id: vec![],
            trace_state: String::new(),
            flags: 0,
            name: "chat gpt-4".to_string(),
            kind: SpanKind::Client as i32,
            start_time_unix_nano: 1_000,
            end_time_unix_nano: 1_500,
            attributes: vec![
                attr(
                    "gen_ai.operation.name",
                    ProtoValue::StringValue("chat".to_string()),
                ),
                attr(
                    "gen_ai.provider.name",
                    ProtoValue::StringValue("aws.bedrock".to_string()),
                ),
                attr("gen_ai.usage.input_tokens", ProtoValue::IntValue(42)),
            ],
            dropped_attributes_count: 0,
            events: vec![],
            dropped_events_count: 0,
            links: vec![],
            dropped_links_count: 0,
            status: None,
        }
    }

    #[test]
    fn convert_span_builds_model_call_event_from_bedrock_span() {
        let span = model_call_span([1u8; 16], [2u8; 8]);
        let event = convert_span(&span).expect("should convert");
        match event {
            ConvertedEvent::ModelCall(m) => {
                assert_eq!(m.provider_name, ProviderName::AwsBedrock);
                assert_eq!(m.input_tokens.unwrap().get(), 42);
                assert_eq!(m.span.duration_nanos(), Some(500));
            }
            other => panic!("expected ModelCall, got {other:?}"),
        }
    }

    #[test]
    fn convert_span_rejects_invalid_trace_id() {
        let mut span = model_call_span([1u8; 16], [2u8; 8]);
        span.trace_id = vec![0u8; 15]; // wrong length
        assert!(matches!(
            convert_span(&span),
            Err(ConvertError::Malformed(_))
        ));
    }

    #[test]
    fn convert_span_reports_unmodeled_operations_without_error() {
        let mut span = model_call_span([1u8; 16], [2u8; 8]);
        span.attributes = vec![attr(
            "gen_ai.operation.name",
            ProtoValue::StringValue("search_memory".to_string()),
        )];
        assert_eq!(
            convert_span(&span),
            Err(ConvertError::Unmodeled {
                operation_name: Some("search_memory".to_string())
            })
        );
    }

    #[tokio::test]
    async fn export_reports_partial_success_when_a_span_is_malformed() {
        let mut bad_span = model_call_span([1u8; 16], [3u8; 8]);
        bad_span.trace_id = vec![0u8; 16]; // all-zero: invalid

        let request = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource::default()),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![model_call_span([1u8; 16], [2u8; 8]), bad_span],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };

        let sink = InMemorySink::new();
        let receiver = Receiver::new(sink);
        let response = receiver
            .export(tonic::Request::new(request))
            .await
            .unwrap()
            .into_inner();

        let partial_success = response
            .partial_success
            .expect("expected a partial success");
        assert_eq!(partial_success.rejected_spans, 1);
    }
}

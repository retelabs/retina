//! `TraceService::export` (crate::proto) — the gRPC entry point (dossier
//! step 2). Validates each span via `convert::convert_span`, hands accepted
//! events to a [`SpanSink`] as one batch per `Export` call, and reports
//! rejections via OTLP's partial-success mechanism
//! (docs/interfaces/otlp-ingestion.md) instead of failing the whole batch on
//! one bad span.

use crate::convert::{ConvertError, ConvertedEvent, convert_span};
use crate::proto::opentelemetry::proto::collector::trace::v1::trace_service_server::TraceService;
use crate::proto::opentelemetry::proto::collector::trace::v1::{
    ExportTracePartialSuccess, ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use crate::sink::SpanSink;

pub struct Receiver<S> {
    sink: S,
}

impl<S: SpanSink> Receiver<S> {
    pub fn new(sink: S) -> Self {
        Self { sink }
    }
}

#[tonic::async_trait]
impl<S: SpanSink + 'static> TraceService for Receiver<S> {
    async fn export(
        &self,
        request: tonic::Request<ExportTraceServiceRequest>,
    ) -> Result<tonic::Response<ExportTraceServiceResponse>, tonic::Status> {
        let req = request.into_inner();
        let mut accepted: Vec<ConvertedEvent> = Vec::new();
        let mut rejected_spans: i64 = 0;

        for resource_spans in req.resource_spans {
            for scope_spans in resource_spans.scope_spans {
                for span in &scope_spans.spans {
                    match convert_span(span) {
                        Ok(event) => accepted.push(event),
                        Err(ConvertError::Malformed(_)) => rejected_spans += 1,
                        // Structurally valid but not one of the 3 MVP event
                        // types (dossier section 4) — not the sender's fault,
                        // so it must not count as `rejected_spans`.
                        Err(ConvertError::Unmodeled { .. }) => {}
                    }
                }
            }
        }

        let mut persist_error: Option<String> = None;
        if !accepted.is_empty() {
            let accepted_count = accepted.len() as i64;
            // docs/interfaces/clickhouse-schema.md: the sink commits a whole
            // batch or none of it — a persist failure means none of these
            // structurally-valid spans made it, so they count as rejected
            // too (an OTLP client can't distinguish "invalid" from "valid
            // but not stored" — both mean "not exported").
            if let Err(e) = self.sink.accept_batch(accepted).await {
                rejected_spans += accepted_count;
                persist_error = Some(e.to_string());
            }
        }

        let partial_success = if rejected_spans > 0 {
            let error_message = match persist_error {
                Some(persist_error) => {
                    format!("{rejected_spans} span(s) rejected: {persist_error}")
                }
                None => format!(
                    "{rejected_spans} span(s) rejected: invalid trace_id/span_id or missing required gen_ai attributes"
                ),
            };
            Some(ExportTracePartialSuccess {
                rejected_spans,
                error_message,
            })
        } else {
            None
        };

        Ok(tonic::Response::new(ExportTraceServiceResponse {
            partial_success,
        }))
    }
}

//! `TraceService::export` (crate::proto) — the gRPC entry point (dossier
//! step 2). Validates each span via `convert::convert_span`, hands accepted
//! events to a [`SpanSink`], and reports rejections via OTLP's partial-success
//! mechanism (docs/interfaces/otlp-ingestion.md) instead of failing the whole
//! batch on one bad span.

use crate::convert::{ConvertError, convert_span};
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
        let mut rejected_spans: i64 = 0;

        for resource_spans in req.resource_spans {
            for scope_spans in resource_spans.scope_spans {
                for span in &scope_spans.spans {
                    match convert_span(span) {
                        Ok(event) => self.sink.accept(event),
                        Err(ConvertError::Malformed(_)) => rejected_spans += 1,
                        // Structurally valid but not one of the 3 MVP event
                        // types (dossier section 4) — not the sender's fault,
                        // so it must not count as `rejected_spans`.
                        Err(ConvertError::Unmodeled { .. }) => {}
                    }
                }
            }
        }

        let partial_success = if rejected_spans > 0 {
            Some(ExportTracePartialSuccess {
                rejected_spans,
                error_message: format!(
                    "{rejected_spans} span(s) rejected: invalid trace_id/span_id or missing required gen_ai attributes"
                ),
            })
        } else {
            None
        };

        Ok(tonic::Response::new(ExportTraceServiceResponse {
            partial_success,
        }))
    }
}

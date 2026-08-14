/// Mirrors OTLP `Status` (opentelemetry-proto v1.11.0, `trace/v1/trace.proto`).
/// Distinct from the `error.type` attribute: this is the span's logical
/// success/failure outcome, `error.type` is a low-cardinality error identifier
/// (see docs/interfaces/semconv-genai.md).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StatusCode {
    #[default]
    Unset,
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpanStatus {
    pub code: StatusCode,
    pub message: String,
}

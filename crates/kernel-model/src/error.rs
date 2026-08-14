use std::fmt;

/// Errors raised when converting raw OTLP-shaped input into the kernel's
/// internal model. See docs/interfaces/semconv-genai.md and
/// docs/interfaces/otlp-ingestion.md for the contracts these enforce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    /// `gen_ai.usage.*` attributes are transported as signed `int64`
    /// (opentelemetry-proto v1.11.0, AnyValue.int_value) — a negative value is
    /// malformed input, never silently wrapped into a huge `u64`.
    NegativeTokenCount(i64),
    MissingRequiredField(&'static str),
    /// OTLP `trace_id` must be exactly 16 bytes (trace.proto, Span.trace_id).
    InvalidTraceId {
        got_len: usize,
    },
    /// OTLP `span_id` must be exactly 8 bytes (trace.proto, Span.span_id).
    InvalidSpanId {
        got_len: usize,
    },
    /// Correct length but all-zero — explicitly "considered invalid" by
    /// trace.proto's Span.trace_id doc comment.
    ZeroTraceId,
    /// Correct length but all-zero — explicitly "considered invalid" by
    /// trace.proto's Span.span_id doc comment.
    ZeroSpanId,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::NegativeTokenCount(v) => {
                write!(
                    f,
                    "negative token count received from OTLP int64 value: {v}"
                )
            }
            ModelError::MissingRequiredField(name) => {
                write!(f, "missing required field: {name}")
            }
            ModelError::InvalidTraceId { got_len } => {
                write!(f, "invalid trace_id: expected 16 bytes, got {got_len}")
            }
            ModelError::InvalidSpanId { got_len } => {
                write!(f, "invalid span_id: expected 8 bytes, got {got_len}")
            }
            ModelError::ZeroTraceId => write!(f, "invalid trace_id: all-zero"),
            ModelError::ZeroSpanId => write!(f, "invalid span_id: all-zero"),
        }
    }
}

impl std::error::Error for ModelError {}

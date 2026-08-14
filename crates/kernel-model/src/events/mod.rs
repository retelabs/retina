mod agent;
mod inference;
mod tool;

pub use agent::{AgentInvocationKind, AgentRunEvent};
pub use inference::ModelCallEvent;
pub use tool::ToolCallEvent;

use crate::ids::{SpanId, TraceId};
use crate::status::SpanStatus;
use crate::value::Attribute;

/// Fields every one of the 3 MVP span types carries, taken directly from OTLP
/// `Span` (trace.proto) rather than re-derived per event — see
/// docs/interfaces/otlp-ingestion.md.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    pub status: SpanStatus,
    /// `error.type` — a low-cardinality error identifier, distinct from
    /// `status` (see docs/interfaces/semconv-genai.md).
    pub error_type: Option<String>,
}

impl SpanContext {
    /// `end_time_unix_nano - start_time_unix_nano`, or `None` if the span is
    /// malformed (end before start). OTLP only says end >= start is *expected*,
    /// not guaranteed (docs/interfaces/otlp-ingestion.md) — so this is a
    /// `checked_sub`, never a bare `-`.
    pub fn duration_nanos(&self) -> Option<u64> {
        self.end_time_unix_nano
            .checked_sub(self.start_time_unix_nano)
    }
}

/// Attributes seen on a span but not promoted to a first-class field on the
/// event struct: provider-specific extensions (e.g. `aws.bedrock.*`) and
/// generic `gen_ai.*` attributes not yet modeled for the MVP (dossier section
/// 2.1: kept separate rather than forcing a normalization that would lose
/// information).
pub type ExtraAttributes = Vec<Attribute>;

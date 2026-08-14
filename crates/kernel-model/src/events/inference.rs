use super::{ExtraAttributes, SpanContext};
use crate::operation::OperationName;
use crate::provider::ProviderName;
use crate::value::TokenCount;

/// `gen_ai.inference.client` span (vendor/semconv-genai @ 30182acd,
/// `model/gen-ai/spans.yaml`) — the "appel modèle" event (dossier section 2.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelCallEvent {
    pub span: SpanContext,

    /// required, sampling_relevant
    pub provider_name: ProviderName,
    /// required (from `attributes.gen_ai.common`)
    pub operation_name: OperationName,

    /// conditionally_required: "If available."
    pub request_model: Option<String>,
    /// recommended
    pub response_model: Option<String>,

    pub input_tokens: Option<TokenCount>,
    pub output_tokens: Option<TokenCount>,
    /// already included in `input_tokens` per spec note — kept separately too,
    /// since Anthropic reports it outside `input_tokens` and callers computing
    /// provider-normalized totals need the raw parts.
    pub cache_read_input_tokens: Option<TokenCount>,
    pub cache_creation_input_tokens: Option<TokenCount>,

    /// recommended
    pub finish_reasons: Vec<String>,
    /// conditionally_required
    pub conversation_id: Option<String>,

    /// Fine-grained request params (top_p, temperature, seed, stream, ...) are
    /// deliberately NOT first-class fields for the MVP — see "Ignoré
    /// volontairement" in docs/interfaces/semconv-genai.md. They live here if
    /// captured.
    pub extra_attributes: ExtraAttributes,
}

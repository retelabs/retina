//! JSON wire format exchanged across the WASM guest/host boundary
//! (docs/interfaces/wasm-plugin-loading.md). Deliberately its own crate,
//! separate from `kernel-model` (which stays dependency-free, CLAUDE.md
//! step 1) and from `plugin-api` (whose `KernelEvent<'a>` borrows
//! `kernel-model` types directly — not serializable, and not meant to be:
//! that type is for in-process native plugins). Both the WASM guest and the
//! native host depend on this crate so the JSON shape is defined once, not
//! duplicated and hoped to stay in sync.
//!
//! `operation_name`/`provider_name`/`invocation_kind`/`status_code` are
//! plain `String`s here, not the semi-open enums `kernel-model` uses
//! internally (`OperationName`, `ProviderName`) — a guest plugin consumes
//! the string value either way, and mirroring the `Other(String)` fallback
//! machinery across a JSON boundary buys nothing for a v0 whose point is
//! validating the loading mechanism, not the type system.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WireAttributeValue {
    String(String),
    Bool(bool),
    Int(i64),
    Double(f64),
    Bytes(Vec<u8>),
    Array(Vec<WireAttributeValue>),
    KeyValueList(Vec<(String, WireAttributeValue)>),
}

pub type WireAttributes = Vec<(String, WireAttributeValue)>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireSpanContext {
    /// Lower-hex, 32 chars — same encoding as OTLP/JSON
    /// (docs/interfaces/otlp-ingestion.md) and `query-api`'s DTOs.
    pub trace_id: String,
    /// Lower-hex, 16 chars.
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    /// "unset" | "ok" | "error"
    pub status_code: String,
    pub status_message: String,
    pub error_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireModelCallEvent {
    pub span: WireSpanContext,
    pub provider_name: String,
    pub operation_name: String,
    pub request_model: Option<String>,
    pub response_model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub finish_reasons: Vec<String>,
    pub conversation_id: Option<String>,
    pub extra_attributes: WireAttributes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireToolCallEvent {
    pub span: WireSpanContext,
    pub tool_name: String,
    pub tool_call_id: Option<String>,
    pub tool_type: Option<String>,
    pub tool_description: Option<String>,
    pub agent_name: Option<String>,
    pub extra_attributes: WireAttributes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireAgentRunEvent {
    pub span: WireSpanContext,
    /// "client" | "internal"
    pub invocation_kind: String,
    pub operation_name: String,
    pub agent_name: Option<String>,
    pub agent_id: Option<String>,
    pub agent_description: Option<String>,
    pub agent_version: Option<String>,
    pub request_model: Option<String>,
    pub provider_name: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub conversation_id: Option<String>,
    pub extra_attributes: WireAttributes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WireKernelEvent {
    ModelCall(WireModelCallEvent),
    ToolCall(WireToolCallEvent),
    AgentRun(WireAgentRunEvent),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WirePluginOutcome {
    pub attributes: WireAttributes,
    pub warnings: Vec<String>,
}

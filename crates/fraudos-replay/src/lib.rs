//! Converts `AgentSpan` (the real shape used by fraudos-prototype/fraudos —
//! docs/interfaces/fraudos-agentspan.md) into OTLP and replays it against a
//! running kernel, for dossier step 7 ("run the kernel against a real
//! telemetry flow") without modifying the fraudos-prototype repo itself.

pub mod agent_span;
pub mod convert;
pub mod ids;

pub use agent_span::AgentSpan;
pub use convert::convert;

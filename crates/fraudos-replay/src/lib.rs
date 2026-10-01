//! Converts `AgentSpan` (the real shape used by the fraudos prototype —
//! docs/interfaces/fraudos-agentspan.md) into OTLP and replays it against a
//! running kernel, for dossier step 7 ("run the kernel against a real
//! telemetry flow") without modifying the prototype's repository.

pub mod agent_span;
pub mod convert;
pub mod ids;

pub use agent_span::AgentSpan;
pub use convert::convert;

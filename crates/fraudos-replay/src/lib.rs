//! Converts `AgentSpan` (the real shape used by fraudos-prototype/fraudos —
//! docs/interfaces/fraudos-agentspan.md) into OTLP and replays it against a
//! running kernel, for dossier étape 7 ("faire tourner le kernel contre un
//! vrai flux de télémétrie") without modifying the fraudos-prototype repo itself.

pub mod agent_span;
pub mod convert;
pub mod ids;

pub use agent_span::AgentSpan;
pub use convert::convert;

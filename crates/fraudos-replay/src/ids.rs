//! Deterministic trace_id/span_id derivation from `session_id`, so replaying
//! the same fixture twice produces the same ids (useful for re-running
//! against a fresh ClickHouse without accumulating unbounded duplicate
//! rows). Not cryptographic, not guaranteed stable across Rust versions
//! (`DefaultHasher`'s algorithm isn't a stability guarantee per its own
//! docs) — acceptable here because this is a validation/replay tool, not
//! part of the pinned interoperability contracts (docs/interfaces/), just
//! this crate's own internal convenience.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn hash_bytes(parts: &[&str]) -> [u8; 8] {
    let mut hasher = DefaultHasher::new();
    for part in parts {
        part.hash(&mut hasher);
    }
    hasher.finish().to_be_bytes()
}

/// 16 bytes: two independent 8-byte hashes of `session_id`, salted
/// differently so they don't just repeat.
pub fn derive_trace_id(session_id: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&hash_bytes(&[session_id, "trace-id-salt-a"]));
    out[8..].copy_from_slice(&hash_bytes(&[session_id, "trace-id-salt-b"]));
    out
}

/// 8 bytes: hash of `session_id` plus a role tag, so the root span and each
/// synthetic child span within one trace get distinct, stable span ids.
pub fn derive_span_id(session_id: &str, role: &str) -> [u8; 8] {
    hash_bytes(&[session_id, "span-id", role])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_session_id_yields_same_ids() {
        assert_eq!(derive_trace_id("s1"), derive_trace_id("s1"));
        assert_eq!(derive_span_id("s1", "root"), derive_span_id("s1", "root"));
    }

    #[test]
    fn different_roles_yield_different_span_ids() {
        assert_ne!(
            derive_span_id("s1", "root"),
            derive_span_id("s1", "model_call")
        );
    }

    #[test]
    fn trace_id_is_never_all_zero_for_a_normal_session_id() {
        // Not a hard guarantee for arbitrary input, but true enough for the
        // fixtures this tool actually generates — a regression here would
        // mean kernel_model::TraceId rejects otherwise-valid replay data.
        assert_ne!(derive_trace_id("fraudos-session-1"), [0u8; 16]);
    }
}

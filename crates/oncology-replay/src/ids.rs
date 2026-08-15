//! Deterministic trace_id/span_id derivation from `session_id` — same
//! rationale as `crates/fraudos-replay/src/ids.rs` (replay/validation tool,
//! not a pinned interoperability contract; duplicated rather than factored
//! into a shared crate for ~30 lines used twice).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn hash_bytes(parts: &[&str]) -> [u8; 8] {
    let mut hasher = DefaultHasher::new();
    for part in parts {
        part.hash(&mut hasher);
    }
    hasher.finish().to_be_bytes()
}

pub fn derive_trace_id(session_id: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&hash_bytes(&[session_id, "trace-id-salt-a"]));
    out[8..].copy_from_slice(&hash_bytes(&[session_id, "trace-id-salt-b"]));
    out
}

pub fn derive_span_id(session_id: &str, role: &str) -> [u8; 8] {
    hash_bytes(&[session_id, "span-id", role])
}

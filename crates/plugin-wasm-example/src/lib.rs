//! WASM guest mirroring `plugin-example`'s logic exactly (same
//! `example.total_tokens` derivation, same `tool_call_id` warning) — the
//! point is proving the *loading mechanism* behaves identically to the
//! native trait plugin for the same input, not inventing new plugin
//! behavior. See docs/interfaces/wasm-plugin-loading.md for the ABI this
//! implements (`alloc`/`process`, JSON over linear memory).

use std::mem;

use plugin_wasm_wire::{WireAttributeValue, WireKernelEvent, WirePluginOutcome};

/// Allocates `len` bytes the host can write into before calling `process`.
/// Leaked deliberately — see docs/interfaces/wasm-plugin-loading.md (no
/// `dealloc` in this v0; the host creates a fresh `Store` per call, so nothing
/// accumulates across invocations).
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: i32) -> i32 {
    let mut buf = Vec::<u8>::with_capacity(len.max(0) as usize);
    let ptr = buf.as_mut_ptr();
    mem::forget(buf);
    ptr as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn process(ptr: i32, len: i32) -> i64 {
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };

    let outcome = match serde_json::from_slice::<WireKernelEvent>(input) {
        Ok(event) => inspect(&event),
        Err(e) => WirePluginOutcome {
            attributes: vec![],
            warnings: vec![format!("invalid input JSON: {e}")],
        },
    };

    let output = serde_json::to_vec(&outcome).expect("WirePluginOutcome always serializes");
    let out_ptr = alloc(output.len() as i32);
    // SAFETY: `alloc` just returned this exact pointer with capacity
    // `output.len()`, immediately below — nothing else has touched it yet.
    unsafe {
        std::ptr::copy_nonoverlapping(output.as_ptr(), out_ptr as *mut u8, output.len());
    }

    ((out_ptr as i64) << 32) | (output.len() as i64 & 0xFFFF_FFFF)
}

fn inspect(event: &WireKernelEvent) -> WirePluginOutcome {
    match event {
        WireKernelEvent::ModelCall(e) => total_tokens_outcome(e.input_tokens, e.output_tokens),
        WireKernelEvent::AgentRun(e) => total_tokens_outcome(e.input_tokens, e.output_tokens),
        WireKernelEvent::ToolCall(e) => {
            let mut outcome = WirePluginOutcome::default();
            if e.tool_call_id.is_none() {
                outcome.warnings.push(format!(
                    "tool call to `{}` has no tool_call_id",
                    e.tool_name
                ));
            }
            outcome
        }
    }
}

fn total_tokens_outcome(input: Option<u64>, output: Option<u64>) -> WirePluginOutcome {
    let mut outcome = WirePluginOutcome::default();
    if let (Some(input), Some(output)) = (input, output)
        && let Ok(total) = i64::try_from(input + output)
    {
        outcome.attributes.push((
            "example.total_tokens".to_string(),
            WireAttributeValue::Int(total),
        ));
    }
    outcome
}

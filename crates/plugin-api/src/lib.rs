//! Plugin contract v0 (dossier section 2.2 étape 5, section 2.4). Fixes only
//! the *interpretation* contract — how a plugin inspects one kernel event
//! and what it can contribute back. How a plugin gets loaded (native Rust
//! trait implementation compiled in, vs a WASM module loaded dynamically via
//! `wasmtime`) is a separate, still-open question (dossier section 5) that
//! this crate deliberately does not answer. See
//! docs/interfaces/plugin-contract-v0.md.
//!
//! Depends only on `kernel-model`, not on `otlp-receiver` — a plugin
//! interprets the kernel's internal event model, not how a given event
//! arrived (dossier section 2.4: don't over-design ahead of a real
//! vertical's needs, and don't couple the plugin boundary to the transport).

use kernel_model::{AgentRunEvent, Attribute, ModelCallEvent, ToolCallEvent};

/// Borrowing view over one of the 3 MVP event kinds.
pub enum KernelEvent<'a> {
    ModelCall(&'a ModelCallEvent),
    ToolCall(&'a ToolCallEvent),
    AgentRun(&'a AgentRunEvent),
}

/// What a plugin contributes after inspecting one event: attributes to
/// attach, and/or human-readable warnings.
///
/// Deliberately infallible for v0 — no variant for "reject this event".
/// Dossier section 2.4 warns against over-designing this contract before a
/// real vertical (fintech/fraude, section 3) informs it; giving a plugin the
/// power to break ingestion outright is a bigger commitment than a v0 dummy
/// plugin needs to prove out, and easier to add later than to walk back.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PluginOutcome {
    pub attributes: Vec<Attribute>,
    pub warnings: Vec<String>,
}

/// Contract every business plugin implements (dossier section 2.2 étape 5).
pub trait Plugin: Send + Sync {
    /// Stable identifier for logging/attribution. Not necessarily the
    /// vertical's name — a vertical may ship more than one plugin.
    ///
    /// Borrowed with `&self`'s lifetime, not `&'static str`: a compiled-in
    /// plugin can always return a `'static` literal (which still satisfies
    /// this signature), but a dynamically-loaded one
    /// (`docs/interfaces/wasm-plugin-loading.md`) only knows its name once
    /// loaded at runtime — from the file path, embedded metadata, etc. — and
    /// can't manufacture a `'static` string for it. Found by actually
    /// implementing the WASM loading path, not anticipated in the v0 draft.
    fn name(&self) -> &str;

    fn inspect(&self, event: KernelEvent<'_>) -> PluginOutcome;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoOpPlugin;
    impl Plugin for NoOpPlugin {
        fn name(&self) -> &'static str {
            "noop"
        }
        fn inspect(&self, _event: KernelEvent<'_>) -> PluginOutcome {
            PluginOutcome::default()
        }
    }

    #[test]
    fn a_minimal_plugin_satisfies_the_trait_object_safely() {
        // The trait must stay object-safe: the future loading story (dossier
        // section 5) will need `Vec<Box<dyn Plugin>>` or similar regardless
        // of whether it ends up native or WASM-backed.
        let plugins: Vec<Box<dyn Plugin>> = vec![Box::new(NoOpPlugin)];
        assert_eq!(plugins[0].name(), "noop");
    }
}

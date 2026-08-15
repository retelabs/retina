//! WASM plugin loading host (dossier section 5, exploration — not a
//! decision). See docs/interfaces/wasm-plugin-loading.md for the ABI and
//! why the Component Model was not used for this v0.

mod host;
mod wire_convert;

pub use host::WasmPlugin;

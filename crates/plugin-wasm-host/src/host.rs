//! [`WasmPlugin`] — loads a compiled `.wasm` guest and implements
//! `plugin_api::Plugin` by delegating to it, per the ABI fixed in
//! docs/interfaces/wasm-plugin-loading.md.

use std::path::Path;

use anyhow::{Context, Result, anyhow};
use plugin_api::{KernelEvent, Plugin, PluginOutcome};
use plugin_wasm_wire::WirePluginOutcome;
use wasmtime::{Engine, Instance, Module, Store};

use crate::wire_convert::{from_wire_outcome, to_wire_event};

pub struct WasmPlugin {
    name: String,
    engine: Engine,
    module: Module,
}

/// `wasmtime::Error` doesn't implement `std::error::Error` the way `anyhow`'s
/// `Context` trait requires (found by compiling, not documented anywhere
/// obvious) — converted by hand via `Display` instead of `anyhow::Context`.
fn wasm_err(context: &str, e: wasmtime::Error) -> anyhow::Error {
    anyhow!("{context}: {e}")
}

impl WasmPlugin {
    pub fn from_file(name: impl Into<String>, path: impl AsRef<Path>) -> Result<Self> {
        let engine = Engine::default();
        let module = Module::from_file(&engine, path.as_ref()).map_err(|e| {
            wasm_err(
                &format!("loading wasm module from {}", path.as_ref().display()),
                e,
            )
        })?;
        Ok(Self {
            name: name.into(),
            engine,
            module,
        })
    }

    /// Fresh `Store`/`Instance` per call (docs/interfaces/wasm-plugin-loading.md:
    /// simpler than tracking guest allocations across calls, at the cost of
    /// re-instantiating every time — not measured/optimized for this v0).
    fn try_inspect(&self, event: &KernelEvent<'_>) -> Result<PluginOutcome> {
        let wire_event = to_wire_event(event);
        let input = serde_json::to_vec(&wire_event).context("serializing KernelEvent to JSON")?;

        let mut store: Store<()> = Store::new(&self.engine, ());
        let linker = wasmtime::Linker::new(&self.engine);
        let instance: Instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| wasm_err("instantiating wasm module", e))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow!("guest does not export `memory`"))?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|e| wasm_err("guest does not export `alloc(i32) -> i32`", e))?;
        let process = instance
            .get_typed_func::<(i32, i32), i64>(&mut store, "process")
            .map_err(|e| wasm_err("guest does not export `process(i32, i32) -> i64`", e))?;

        let in_ptr = alloc
            .call(&mut store, input.len() as i32)
            .map_err(|e| wasm_err("calling guest `alloc`", e))?;
        let in_ptr = usize::try_from(in_ptr)
            .map_err(|_| anyhow!("guest `alloc` returned a negative pointer"))?;
        memory.data_mut(&mut store)[in_ptr..in_ptr + input.len()].copy_from_slice(&input);

        let packed = process
            .call(&mut store, (in_ptr as i32, input.len() as i32))
            .map_err(|e| wasm_err("calling guest `process`", e))?;
        // Packed as (ptr << 32) | len — see docs/interfaces/wasm-plugin-loading.md.
        let out_ptr =
            usize::try_from((packed >> 32) as u32).context("decoding guest output pointer")?;
        let out_len = usize::try_from((packed & 0xFFFF_FFFF) as u32)
            .context("decoding guest output length")?;

        let output = memory.data(&store)[out_ptr..out_ptr + out_len].to_vec();
        let wire_outcome: WirePluginOutcome =
            serde_json::from_slice(&output).context("parsing guest output JSON")?;

        Ok(from_wire_outcome(wire_outcome))
    }
}

impl Plugin for WasmPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn inspect(&self, event: KernelEvent<'_>) -> PluginOutcome {
        // A WASM guest is not trusted the way `plugin-example` is (dossier
        // section 2.4 / docs/interfaces/wasm-plugin-loading.md): any failure
        // here — trap, bad JSON, missing exports — becomes a warning, never
        // a panic that would take the host down with it.
        self.try_inspect(&event).unwrap_or_else(|e| PluginOutcome {
            attributes: vec![],
            warnings: vec![format!("wasm plugin `{}` failed: {e:#}", self.name)],
        })
    }
}

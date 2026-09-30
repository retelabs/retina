# wasm-plugin-loading: dynamic plugin loading through `wasmtime` (v0)

- Authoritative source: https://docs.rs/wasmtime (crate `wasmtime`, v47.0.3 at
  the time, checked on docs.rs; upgraded to 49.0.1 on 2026-09-30 for RustSec
  advisories, with no change to the API used here). This explores the open
  question of dossier section 5 ("the exact plugin loading mode: statically
  compiled Rust trait or dynamically loaded WASM modules") without settling it
  for good (see below, and ADR 0004).
- Verification date: 2026-08-15
- Scope: a v0 WASM loading mechanism, next to the already validated Rust trait
  contract (`docs/interfaces/plugin-contract-v0.md`), not a replacement.

## Decision: a "core" WASM module (home-made ABI), not the Component Model

`wasmtime` supports two ways to structure an interface:

1. **Component Model** (WIT + `wit-bindgen`): generated typed bindings, the
   "modern" approach the wasmtime project recommends for new code, but it needs
   the `cargo-component`/`wasm-tools` tooling (building a `.wasm` as a component,
   not just `cargo build --target wasm32-unknown-unknown`). **Neither tool was
   installed in this environment**, and installing them adds new tooling before
   even knowing whether the WASM approach will be kept.
2. **A "core" module**: `Engine`/`Module::from_file`/`Store`/`Linker`/
   `Instance::get_typed_func`, with a hand-made linear-memory ABI (passing
   pointers and lengths). It needs only `rustup target add
   wasm32-unknown-unknown`, among the targets installable by default (checked:
   `rustup target list`).

**Chosen for this v0: option 2.** Consistent with the dossier's warning (section
2.4) against over-designing the plugin contract before a real vertical needs it:
this validates the *mechanism* (loading a `.wasm` dynamically without recompiling
the core), not a long-term interface. If the WASM approach proves useful, moving
to the Component Model later is a tooling change, not a design change: the
`plugin-wasm-wire` DTOs defined here would stay valid.

## `wasmtime` API, verified (v47.0.3)

```rust
let engine = Engine::default();
let module = Module::from_file(&engine, "plugin.wasm")?;
let mut store: Store<()> = Store::new(&engine, ());
let linker = Linker::new(&engine);
let instance = linker.instantiate(&mut store, &module)?;

let memory = instance.get_memory(&mut store, "memory").ok_or(...)?;
memory.data_mut(&mut store)[ptr..ptr+len].copy_from_slice(bytes); // write

let alloc = instance.get_typed_func::<i32, i32>(&mut store, "alloc")?;
let process = instance.get_typed_func::<(i32, i32), i64>(&mut store, "process")?;
```

## The ABI (linear memory, JSON)

The guest exports two functions and its linear memory:

- `alloc(len: i32) -> i32`: the guest allocates `len` bytes in ITS memory and
  returns the pointer; the host then writes the input bytes there. The guest owns
  the allocation, not the host, which avoids exposing a host allocator to the
  guest.
- `process(ptr: i32, len: i32) -> i64`: the guest reads `len` JSON bytes at
  `ptr` (a `WireKernelEvent`, `crates/plugin-wasm-wire`), computes a
  `WirePluginOutcome`, writes it as JSON into its own memory (through a second
  `alloc`), and returns `(out_ptr << 32) | out_len` packed into a single `i64`.

  **Tried first, set aside**: having `process` return a Rust tuple `(i32, i32)`
  through `extern "C"` to use wasm's native multi-value return. It compiles, but
  `rustc` explicitly warns `improper_ctypes_definitions`: *"tuples have
  unspecified layout"*. The tuple → wasm return values mapping is not a language
  guarantee, only current compiler behaviour. This was checked by compiling an
  isolated case before putting it into the contract rather than relying on it.
  Manual packing into an `i64` has no layout ambiguity.

**Data format: JSON** (`serde_json`), not a compact binary format: readability
and ease of debugging win over performance for a v0 whose goal is to validate the
mechanism, not optimise it. `crates/plugin-wasm-wire` defines the
`Serialize`/`Deserialize` types (`WireAttributeValue`, `WireKernelEvent`,
`WirePluginOutcome`) separately from `kernel-model`: `kernel-model` stays free of
external dependencies (see CLAUDE.md step 1), so the WASM bridge's `serde` types
live in their own crate rather than being added to `kernel-model`.

## Safety and robustness: the difference from the native Rust trait plugin

Unlike `plugin-example` (internal, trusted code;
`docs/interfaces/plugin-contract-v0.md` explicitly notes that handling a plugin
that panics or loops "is not relevant while the only plugins are internal"), a
WASM module is meant to be able to come from a third party. The host wrapper
(`WasmPlugin::inspect`, `crates/plugin-wasm-host`) must therefore absorb any
guest failure (trap, invalid JSON, missing functions) into an empty
`PluginOutcome` plus a warning rather than propagate it. This is consistent with
the already infallible `PluginOutcome` contract, but it moves the responsibility
for "never crash" from the guest (not trusted) to the host wrapper (trusted).

**A limit not handled in this v0**: no execution time limit (`wasmtime` supports
fuel metering and epoch interruption, but that is a separate mechanism, not
enabled here) and no memory limit beyond `wasmtime`'s defaults. A guest that
loops forever would block the caller. The 2026-09-30 audit makes these limits a
prerequisite before loading any third-party module (ADR 0004).

## Deliberately ignored in this v0

- Component Model / WIT (see above).
- `wasi` (file or network access from the guest): an interpretation plugin does
  not need it; the guest only sees the bytes it is given.
- Resource limits (fuel, time, memory): see above.
- Caching compiled modules across calls: `WasmPlugin` compiles the module once at
  construction (`Module::from_file`) and reuses the `Engine`, but recreates a
  `Store`/`Instance` on every call (simpler, with no risk of guest memory leaking
  between calls; the instantiation cost is not measured or optimised here).

# 0004. Plugin loading: native Rust trait, dynamic WASM deferred

Date: 2026-08-16 (WASM exploration done on 2026-08-15; the decision not to
integrate it for now is recorded here)

Status: Accepted for the current native mode. Dynamic WASM loading is not
rejected, only deferred until a need is shown.

## Context

The design dossier (section 5) asked "statically compiled Rust trait or
dynamically loaded WASM modules?" and explicitly asked not to over-design the
plugin contract up front, letting the first real vertical inform it (section
2.4). Both real verticals (`plugin-fraudos`, `plugin-medical`) were written as
Rust types compiled into the `kernel` binary from the start. A complete WASM
exploration followed (`crates/plugin-wasm-wire`, `crates/plugin-wasm-example`,
`crates/plugin-wasm-host`, `docs/interfaces/wasm-plugin-loading.md`) and proved,
with a test comparing the WASM plugin's output bit for bit with the equivalent
native plugin, that both approaches behave the same. That exploration was never
wired into the real pipeline: `crates/plugin-sink` only loads native
`Box<dyn Plugin>` values (`FraudosPlugin`, `MedicalPlugin`).

## Decision

The plugins that actually run in `kernel` stay statically compiled Rust types,
enabled or disabled by configuration (`ENABLED_PLUGINS`,
`crates/kernel/src/main.rs`, a separate decision documented in `CLAUDE.md`).
Dynamic loading of third-party WASM code (`crates/plugin-wasm-host`) is not
adopted in the real pipeline for now.

## Consequences

Simplicity and trust: a native plugin is trusted internal code, with no need to
isolate it from a malicious panic or infinite loop beyond what
`crates/plugin-sink` already does for an unrelated reason (panic and timeout
isolation of every plugin, native ones included, added on 2026-08-15). The
accepted downside: adding a new business vertical means recompiling the kernel,
and no third-party plugin marketplace is possible without reopening this work.
How to handle a third-party WASM plugin that panics or loops forever (a real risk
for untrusted code, unlike today's internal code) stays explicitly undecided if
the work is reopened (`docs/interfaces/plugin-contract-v0.md`). The 2026-09-30
audit adds a prerequisite: fuel, memory and time limits on the WASM host before
it loads any third-party module.

## Alternatives considered

Loading `crates/plugin-wasm-host` into the real pipeline now: set aside. No real
need for a third-party or dynamically loaded plugin has appeared (only two
verticals so far, both internal and trusted). The exploration remains available,
verified equivalent to native, if the need appears: an investment that has
already paid off in understanding, not a dead end.

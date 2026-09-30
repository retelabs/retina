# plugin-contract-v0: the business plugin contract (kernel step 5)

- Authoritative source: this project's own design (dossier section 2.2 step 5,
  section 2.4), documented here because it is the contract every future vertical
  plugin will implement (fintech first, dossier section 3): a real
  interoperability point even without an external spec.
- Verification date: 2026-08-14 (updated 2026-08-15: first real plugins and
  wiring into the pipeline)
- Crates: `crates/plugin-api` (the contract), `crates/plugin-example` (the "dummy
  plugin" the dossier asks for, to validate the contract before anticipating a
  real vertical's needs), `crates/plugin-fraudos` and `crates/plugin-medical`
  (two plugins **actually informed by a real vertical**, fraudos and oncology,
  see `docs/interfaces/oncology-governance.md`), now **wired in for real**
  through `crates/plugin-sink` in `crates/kernel`.

## What this contract fixes, and what it does not

The dossier (section 2.4) is explicit: *"Do not over-design the plugin contract up
front: let the first real vertical inform it."* So `plugin-api` fixes only the
**interpretation contract**: how a plugin inspects a kernel event and what it may
return. It does **not** fix:

- **The loading mode**: a statically compiled Rust trait or a WASM module loaded
  dynamically through `wasmtime` remained an open question (dossier section 5;
  since decided in ADR 0004). The `Plugin` trait is deliberately *object-safe*
  (`Vec<Box<dyn Plugin>>` works, tested) so as to close neither door, but no
  loading mechanism is implemented here.
- **The wiring into the pipeline**: `plugin-api` itself still depends only on
  `kernel-model`, not on `otlp-receiver` or `clickhouse-sink`, which keeps the
  contract neutral. **Resolved on 2026-08-15** (informed by the oncology
  vertical, dossier section 2.4): `crates/plugin-sink`
  (`PluginSink<S: SpanSink>`) wraps any `SpanSink` (a decorator, not a hook
  directly in `otlp-receiver` or in `ClickHouseSink`) and merges `PluginOutcome`
  into `extra_attributes` before persistence. `crates/kernel` uses it for real
  (wrapping `ClickHouseSink`). Details and end-to-end verification in
  `docs/interfaces/oncology-governance.md`.
- **The fintech attributes themselves** (`transaction_id`, risk score, decision
  threshold, dossier section 3) are not in this contract. The dummy plugin
  (`ExamplePlugin`) is deliberately generic, a toy, not a draft of the fintech
  plugin.

## The contract

```rust
pub enum KernelEvent<'a> {
    ModelCall(&'a ModelCallEvent),
    ToolCall(&'a ToolCallEvent),
    AgentRun(&'a AgentRunEvent),
}

pub struct PluginOutcome {
    pub attributes: Vec<Attribute>,   // (String, AttributeValue) to attach
    pub warnings: Vec<String>,
}

pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn inspect(&self, event: KernelEvent<'_>) -> PluginOutcome;
}
```

- `KernelEvent` borrows the three `kernel-model` structs: no copy, no
  intermediate format.
- `PluginOutcome` is **infallible**: there is no variant to hard-reject an event.
  Giving a plugin the power to break ingestion is a heavier commitment than a v0
  dummy plugin needs to prove; easier to add later than to remove if set too
  early.
- `inspect` is synchronous. No need for external I/O has been identified for a v0
  plugin; to revisit if a real vertical needs it (it would also touch the WASM
  question: `wasmtime` host calls have their own asynchrony constraints, to check
  precisely when the time comes rather than assume now).

## `ExamplePlugin`: what the dummy plugin validates

- Computes `example.total_tokens` (input + output) for `ModelCallEvent` and
  `AgentRunEvent` when both counts are present. It proves the contract can
  produce a derived attribute consistent with the internal model
  (`AttributeValue::Int`, a checked `u64 -> i64` conversion, no bare cast, the
  same reflex as everywhere else in the kernel).
- Warns when a `ToolCallEvent` has no `tool_call_id`. It proves the contract
  allows business validation without blocking ingestion.
- 4 tests cover the three `KernelEvent` variants and the "missing field → no
  attribute produced" case.

## `FraudosPlugin`: the first plugin actually informed by a vertical

Unlike `ExamplePlugin`/`plugin-wasm-example` (generic, written before a real
vertical existed), `FraudosPlugin` (`crates/plugin-fraudos`) interprets the
`fraudos.*` attributes that `crates/fraudos-replay` already attaches in
`extra_attributes` on `AgentRunEvent`s (`transaction_id`, `final_decision`,
etc., never promoted to first-class `kernel-model` fields, exactly the role the
dossier section 3 anticipated for a plugin):

- **Warns** if a consequential decision (`CONFIRMED_FRAUD`, `REQUEST_BLOCK`,
  `ESCALATED_COMPLIANCE`, `CASE_OPENED`) has no `fraudos.transaction_id`: without
  that identifier, the real outcome that arrives later (dossier section 3) can no
  longer be correlated with this run.
- **Computes** `fraudos.requires_urgent_review` (bool) for the most serious
  decisions (`CONFIRMED_FRAUD`, `REQUEST_BLOCK`).
- **Does nothing** on anything that is not an `AgentRunEvent` carrying at least
  `fraudos.final_decision`: a business plugin wrongly interpreting another
  vertical's data would be worse than a plugin that does nothing (the same logic
  as `PluginOutcome`'s infallibility).
- 5 tests, including the "no `fraudos.*` attribute" case (no-op) and
  "`ModelCallEvent`/`ToolCallEvent`" (always a no-op, even with `fraudos.*`
  attributes on them: only `AgentRunEvent` carries the decision).
- **Wired in for real since 2026-08-15** in `crates/kernel` through
  `crates/plugin-sink`, next to `MedicalPlugin` (see
  `docs/interfaces/oncology-governance.md`), no longer an isolated crate.

## `MedicalPlugin`: the second real plugin, oncology governance gates

See `docs/interfaces/oncology-governance.md` for the full details (real
invariants read in the oncology pipeline's repository, OTLP mapping,
`crates/oncology-replay`). In short: it warns if the deterministic compliance
gate (HIPAA/GDPR) reports a failure without the pipeline stopping, and if the
clinical recommendation step is reached or passed without `approved_by` (the HITL
sign-off). Both invariants are quoted word for word from the source repository's
own agent rules, not inferred. 5 tests.

## Real wiring into the pipeline: `crates/plugin-sink`

Resolves the question left open above. `PluginSink<S: SpanSink>` wraps
`ClickHouseSink` in `crates/kernel`: every converted event goes through the list
of plugins before persistence, `PluginOutcome.attributes` joins
`extra_attributes`, and each warning becomes an entry
`("plugin.warning", "[<plugin name>] <text>")`. `query-api::/metrics/summary` now
exposes `spans_with_warnings` (`countIf(mapContains(extra_attributes,
'plugin.warning')) FROM spans`, a ClickHouse function checked against a real
server before use). Verified end to end: a real kernel, a fraudos/oncology
replay, and the expected warnings found in the database AND in the aggregated
metrics, not only tested in isolation.

**Consolidated on 2026-08-15**: `crates/plugin-sink/tests/integration.rs`
(`--ignored`) now tests the exact combination `crates/kernel` runs in practice
(`PluginSink` wrapping a real `ClickHouseSink`, not `InMemorySink`) with the real
`FraudosPlugin` and `MedicalPlugin` (not toy plugins). It also proves there is no
cross-vertical false positive on really persisted data: a fraudos event does not
trigger `MedicalPlugin` and vice versa, even when both plugins run together on
every event.

## Uncertainties / decisions for later, not now

- WASM loading (`wasmtime`): explored (see `docs/interfaces/wasm-plugin-loading.md`)
  and deferred by ADR 0004, not the default mode.
- ~~Where to insert the plugin call in the pipeline~~ **Resolved** (see above):
  `PluginSink` wraps the final `SpanSink`, so after OTLP validation
  (`otlp-receiver` unchanged) and just before persistence.
- Error handling if a plugin panics or loops (mostly relevant for untrusted
  third-party WASM code): not relevant yet, `FraudosPlugin` and `MedicalPlugin`
  are trusted internal code, like the dummy plugin. It becomes relevant again if a
  third-party WASM plugin is ever wired in the same way (`crates/plugin-wasm-host`
  exists but is not plugged into `PluginSink`).

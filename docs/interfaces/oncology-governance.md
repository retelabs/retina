# oncology-governance: the real shape of the oncology vertical (medical plugin)

- Authoritative source: the oncology pipeline's own repository (a separate
  project, cloned read-only, not vendored: the same treatment as
  `docs/interfaces/fraudos-agentspan.md`), plus a one-off comparison with another
  real implementation of the same (medical) domain as a second reference point.
  The credentials of the online demos provided by the owner were not used: the
  code alone was enough to ground the decisions below, as for fraudos.
- Verification date: 2026-08-15

## What this vertical brings that fraudos does not

- **A deterministic LangGraph pipeline** (7 nodes in a fixed sequence), not a
  multi-turn agentic loop where the LLM picks tools (Bedrock Converse). One real
  LLM call in the whole pipeline: `recommendation_node`
  (`ChatOpenAI(model="gpt-4o")`, structured output `ClinicalRecommendation`).
  **Consequence for the mapping**: unlike `fraudos-agentspan.md`
  (1 `AgentRunEvent` + 1 aggregated `ModelCallEvent` + N `ToolCallEvent`), the
  deterministic nodes here (`ingestion`, `preprocessing`, `modeling`,
  `evaluation`, `visualization`, `monitoring`) are **not** mapped to
  `ToolCallEvent`: they are workflow steps, not tools chosen by an agent
  (`gen_ai.execute_tool.*` does not match that semantically;
  `gen_ai.invoke_workflow.internal` exists in the pinned spec but is outside the
  MVP, `docs/interfaces/semconv-genai.md`). The reconstruction chosen:
  1 `AgentRunEvent` (the complete run) + 1 `ModelCallEvent` (the only real LLM
  call, `provider=openai`, `request_model=gpt-4o`), simpler and more honest than
  forcing a mapping that does not exist in the source.
- **Observability**: LangSmith (`src/observability/langsmith_config.py`, enabled
  by `LANGSMITH_*` environment variables, LangChain callbacks), again with no
  native OTLP. The second vertical in a row where this is the case: a pattern
  confirming itself rather than an assumption specific to fraudos.

## Governance: two real mechanisms, explicit in the code

1. **A deterministic compliance gate**
   (`src/tools/oncology_tools.py::check_hipaa_compliance`): suspicious column
   names (`ssn`, `email`, `dob`, ...) plus an NER scan of the content (Microsoft
   Presidio) if the first check finds nothing. It produces
   `hipaa_cleared`/`gdpr_cleared` (bool) and `compliance_flags` (a list of
   strings: `"PII_DETECTED:ssn,email"`, `"HIPAA_CLEARED"`, ...). If either is
   false, `route_after_ingestion` routes to `__end__`: the pipeline **cannot**
   reach modelling or the LLM. An explicit invariant of that repository: *"NEVER
   let the LLM be the final decision-maker for a compliance/regulatory block —
   deterministic checks only."*
2. **A HITL gate**: `interrupt_before=["recommendation"]` (LangGraph). The graph
   **pauses** before `recommendation_node` and resumes only on
   `POST /pipeline/{p}/{s}/approve` (`Depends(require_clinician)`,
   clinician-only RBAC). `submitted_by`/`approved_by` record who submitted and who
   approved. An explicit invariant: *"ALWAYS keep interrupt_before=[...] — no
   clinical recommendation without human sign-off."* A run can therefore
   legitimately be observed **in the intermediate state**: `current_step` still on
   `"recommendation"`, `submitted_by` set, `approved_by = None`. That is not a
   bug, it is the normal state of a run waiting for human validation: a realistic
   fixture case, not an invented one.

## Finding: two real implementations of the same vertical diverge on compliance

The other implementation has the LLM itself judge GDPR compliance: a free-form
prompt asking for an unstructured text report (*"Analyze the following text and
report: 1. Any personal data (PII)... Be concise."*), no structured bool/enum
output, no deterministic check. That is exactly what the first repository's
explicit invariant forbids. Not an assumption: a real divergence between two
production systems of the same vertical, found by reading both. The plugin below
therefore cannot assume that "a compliance check took place" means "a reliable
check": only the structured result (`hipaa_cleared`/`gdpr_cleared` bool) is used,
never free text.

## The conversion chosen (`crates/oncology-replay`)

The same reflexes as `fraudos-agentspan.md`: a best-effort reconstruction, not a
faithful one (the real LangGraph state has far more fields than we keep):

- `AgentRunEvent` (root, `invocation_kind: Internal`: LangGraph runs in-process,
  not a hosted agent): `agent_name = "oncology_pipeline"`, `oncology.*`
  attributes: `current_step`, `hipaa_cleared`, `gdpr_cleared`,
  `compliance_flags` (joined with `;`), `submitted_by`, `approved_by` (absent if
  `None`), `patient_id`.
- `ModelCallEvent` (child), only if `recommendation_node` **actually ran**:
  `current_step ∈ {"monitoring", "done"}`, **not**
  `current_step == "recommendation"`. Checked precisely in the code, not inferred
  from the name: `visualization_node` already returns
  `current_step: "recommendation"` **before** `interrupt_before=["recommendation"]`
  pauses the graph. That value means "about to call the LLM, not done yet", and
  `recommendation_node` itself only returns `current_step: "monitoring"` once the
  call is made. Confusing the two would have produced a `ModelCallEvent` span for
  a call that never happened. `provider_name = openai`,
  `request_model = "gpt-4o"`. **No token counts**: the code
  (`generate_recommendations`) does not capture the usage metadata LangChain
  returns, unlike `AgentSpan` (fraudos), which aggregates it explicitly. The
  field is left `None`, not invented.
- No `ToolCallEvent` at all for this vertical (see the mapping above).

## The plugin (`crates/plugin-medical`): rules taken directly from the invariants above

1. `oncology.hipaa_cleared == false` or `oncology.gdpr_cleared == false` **and**
   `oncology.current_step != "failed"` → a warning: the compliance gate reports a
   failure but the pipeline was not stopped. A defence-in-depth signal on the
   observability side (the kernel cannot check that `route_after_ingestion` was
   really honoured, only that the attributes it produced are consistent with each
   other).
2. `oncology.current_step` ∈ `{"recommendation", "monitoring", "done"}` **and**
   `oncology.approved_by` absent → a warning: a clinical recommendation reached or
   passed with no trace of HITL approval.
3. A derived attribute `oncology.awaiting_approval = true` when `submitted_by` is
   present but `approved_by` absent (a legitimate intermediate state, distinct
   from a violation: not a warning, just a monitoring signal to spot runs waiting
   for approval).
- A no-op on any event without `oncology.current_step` (not a run of this
  vertical): the same safeguard as `FraudosPlugin`.

## Wiring into the pipeline: settles an open question of `docs/interfaces/plugin-contract-v0.md`

`crates/plugin-sink` (`PluginSink<S: SpanSink>`) wraps any `SpanSink`: before
delegating inwards, it passes every converted event through a list of
`Box<dyn Plugin>`, merges `PluginOutcome.attributes` into `extra_attributes`, and
turns each warning into an attribute `("plugin.warning", "[<plugin name>] <text>")`,
the same generic bag as everything else, no new ClickHouse column. Chosen over a
hook directly in `otlp-receiver`: it keeps `otlp-receiver` independent of plugins
(as documented since step 5), and reusable with any `SpanSink`, not only
`ClickHouseSink`.

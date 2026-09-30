# fraudos-agentspan: the gap with the dossier, and the real shape of the fraudos signal (step 7)

- Authoritative source: the fraudos prototype's own repository (a separate
  project, cloned read-only on 2026-08-14 into a scratch directory, not vendored
  here: it is not an external spec to pin like semconv-genai or OTLP, it is the
  real code of a prototype that keeps evolving independently).
- Verification date: 2026-08-14

## A correction to the dossier, section 3

The dossier states: *"An existing prototype (fraudos) already runs on AWS
Bedrock, instrumented with botocore + ADOT"* and *"Bedrock through ADOT already
emits the OTel GenAI conventions natively... There is no need to build an
adapter."*

**Verified false for the repository's current state**: `requirements.txt` has no
`opentelemetry-*`/ADOT dependency, and no reference to `opentelemetry`, `adot`
or `otlp` exists anywhere in the code. The app uses `bedrock-runtime.converse()`
(the Converse API, multi-turn tool use, Claude Haiku/Sonnet/Opus models through
cross-region inference profiles), but its observability is **home-made**:

- `observability/span.py`: an `AgentSpan` dataclass, a **roll-up per complete
  agent run** (not one span per model or tool call), with no `trace_id`,
  `span_id` or `gen_ai.*` attribute.
- `observability/writer.py`: writes to **CloudWatch Logs** (7-year audit) and
  **DynamoDB** (business dashboards), never to an OTLP collector.

**Consequence for step 7**: "plug it in directly, with no intermediate layer to
build" (dossier) is not possible as things stand: there is nothing to plug in. An
adapter is needed either way, whether to convert `AgentSpan` after the fact (the
option chosen here) or to add real ADOT instrumentation to the prototype (set
aside for this iteration, see below).

## A second correction: the risk score comes from a tool, not from the model

The dossier assumes *"risk score and decision threshold as first-class
attributes [of the model call], not buried in the output blob"*. In reality,
`tools/data_tools.py::get_transaction_score` queries Athena (an external XGBoost
ML model) and returns `fraud_score`, `score_band` and feature contributions: it
is a **tool call result** (`gen_ai.tool.call.result`), not an attribute of the
LLM's response. In our schema `gen_ai.tool.call.result` is `opt_in` and hidden
by default (PII policy, dossier section 2.1): a future fintech plugin will have
to promote `fraud_score` to a first-class attribute explicitly rather than rely
on `gen_ai.response.*`.

## The real shape of `AgentSpan` (`observability/span.py`)

```python
@dataclass
class AgentSpan:
    session_id: str
    agent_role: str          # "fraud_orchestrator" | "fraud_investigator" |
                              # "compliance_officer" | "fraud_scorer" (agents/factory.py)
    task_summary: str
    total_turns: int
    escalated_to_opus: bool
    tools_called: list[str]  # tool names, ordered, no individual id or timing
    tools_failed: list[str]
    unique_tools: int        # derived
    success: bool
    requires_human_review: bool
    final_decision: Optional[str]   # keyword extracted by searching the text:
                                     # CASE_OPENED, DISMISSED, ESCALATED_COMPLIANCE,
                                     # ESCALATED, REQUEST_BLOCK, MONITOR,
                                     # CONFIRMED_FRAUD, FALSE_POSITIVE
    total_input_tokens: int         # aggregated over every turn, no per-turn detail
    total_output_tokens: int
    estimated_cost_usd: float
    primary_model_id: str           # model of the last turn only
    duration_seconds: float
    started_at: str                 # ISO 8601
    ended_at: str
    case_id: Optional[str]
    bank_id: Optional[str]
    transaction_id: Optional[str]
```

## The conversion chosen: AgentSpan → OTLP (a reconstruction, not faithful)

`AgentSpan` is an end-of-run aggregate, not a distributed trace, so converting it
to OTLP spans is a **best-effort reconstruction**, not a faithful mapping (the
per-turn information does not exist in the source). Implemented in
`crates/fraudos-replay`:

- 1 root span `gen_ai.invoke_agent.internal` (→ `AgentRunEvent`,
  `invocation_kind: Internal`: the app runs the loop locally, it is not a hosted
  Bedrock agent): `agent_name = agent_role`, `operation_name = InvokeAgent`,
  tokens = `total_input/output_tokens`.
- 1 child span `gen_ai.inference.client` (→ `ModelCallEvent`) folding every turn
  into one, for lack of per-turn detail: `request_model = primary_model_id`,
  `provider_name = aws.bedrock`, the same total tokens. **Accepted limitation**:
  if the run was escalated (`escalated_to_opus`), the pre-escalation turns
  (Haiku/Sonnet) and the Opus turn are merged into a single span, where real ADOT
  would capture one span per turn.
- 1 child span `gen_ai.execute_tool.internal` (→ `ToolCallEvent`) per entry of
  `tools_called`, marked as an error (`status.code = Error`) if it appears in
  `tools_failed`, with no real individual timing (a synthetic span with the
  parent's time window, documented as approximate).
- `case_id`, `bank_id`, `transaction_id`, `final_decision`,
  `estimated_cost_usd`: no matching `gen_ai.*` attribute. Sent as generic
  attributes prefixed `fraudos.*` (a namespace of this vertical's own, not in the
  pinned `gen_ai.*` registry) on the root span, landing in `extra_attributes` in
  the kernel. This is exactly what the dossier (section 3) anticipated for a
  future fintech plugin: these attributes should be promoted to first-class
  fields by a dedicated plugin rather than stay in the generic bag. Not done here,
  deliberately outside this iteration (no fintech plugin written, only the
  generic v0 contract of step 5).
- `trace_id`/`span_id`: derived deterministically from `session_id` (a hash), not
  random. A replay of the same fixture produces the same trace_id, convenient for
  re-testing.

## Option set aside for this iteration

Adding real ADOT instrumentation to the prototype (a separate repository) and
running it against real Bedrock calls: it needs valid AWS Bedrock credentials and
changes a project distinct from this one. The owner's explicit choice: replay
realistic `AgentSpan`s converted to OTLP against the real pipeline
(`otlp-receiver` → `clickhouse-sink` → `query-api`) without touching the
prototype, and revisit real ADOT instrumentation later if needed.

# Client integration guide

This guide is for people who want to **use** Retina from their own application
(send telemetry, query it), not for people developing the kernel itself. For
the verified details of each contract (exact types, how each field was
confirmed), see `docs/interfaces/`, referenced section by section below.
Nothing here is new: it is a client-oriented summary of contracts already
verified and already running in this repository.

## Overview

```
your app (OTel SDK)   --OTLP/gRPC, authenticated-->  kernel (:4317)    --> ClickHouse
dashboard / script    --HTTP, authenticated-->       query-api (:8080) <-- ClickHouse
```

Two surfaces, two tokens, and no custom OTel infrastructure to write on the
client side: a standard OpenTelemetry SDK is enough.

## Sending telemetry

### Connection and authentication

- gRPC endpoint: `TraceService.Export` (the standard OTLP proto,
  `docs/interfaces/otlp-ingestion.md`), port `4317` by default (`KERNEL_BIND`).
- Required header: the gRPC metadata `authorization: Bearer <KERNEL_API_KEY>`,
  exactly the `OTEL_EXPORTER_OTLP_HEADERS` convention an OTel SDK already emits
  with no custom code:
  ```bash
  export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
  export OTEL_EXPORTER_OTLP_HEADERS="authorization=Bearer <KERNEL_API_KEY>"
  ```
  How it works, and why this header: `docs/interfaces/kernel-auth.md`.
- Without a valid token: `UNAUTHENTICATED`. A structurally invalid span is
  counted in the response's `rejected_spans` (partial success); the rest of the
  batch is accepted.

### What the kernel recognises

Dispatch relies only on the `gen_ai.operation.name` attribute: OTLP carries no
dedicated "this is a gen_ai span" marker. Three groups of values are routed to a
kernel event type; any other value is ignored (`Unmodeled`, not counted as
rejected, simply outside the MVP's scope).

| `gen_ai.operation.name` | Kernel event | Also required |
|---|---|---|
| `chat`, `generate_content`, `text_completion` | Model call | `gen_ai.provider.name` |
| `execute_tool` | Tool call | `gen_ai.tool.name` |
| `invoke_agent` | Agent run | `gen_ai.provider.name` **if** the span's OTLP kind is `CLIENT` (a remote agent such as Bedrock Agents); not required for an in-process agent (kind `INTERNAL`, e.g. LangChain/CrewAI) |

Recommended attributes per type (full list and `required`/`recommended`/`opt_in`
levels: `docs/interfaces/semconv-genai.md`):

- **Model call**: `gen_ai.request.model`, `gen_ai.response.model`,
  `gen_ai.usage.input_tokens`/`output_tokens` (`int`; no negative `u64` on the
  wire), `gen_ai.response.finish_reasons` (array), `gen_ai.conversation.id`.
- **Tool call**: `gen_ai.tool.call.id`, `gen_ai.tool.type`,
  `gen_ai.tool.description`, `gen_ai.agent.name` (the agent running the tool).
- **Agent run**: `gen_ai.agent.name`/`id`/`description`/`version`,
  `gen_ai.request.model` (only if the agent has a single fixed model, not a
  dynamic choice), token usage.

Any attribute not listed above is kept as is in `extra_attributes`: nothing is
lost, it is just not promoted to a first-class field.

### Business attributes (for a plugin to react)

The real plugins (`crates/plugin-fraudos`, `crates/plugin-medical`) interpret
`<vertical>.*` attributes set directly on the `invoke_agent` span by the
application itself, while it runs, not injected afterwards:

- **fraudos**: `fraudos.final_decision` (`CONFIRMED_FRAUD`/`REQUEST_BLOCK`/
  `ESCALATED_COMPLIANCE`/`CASE_OPENED`/...), `fraudos.transaction_id`.
- **oncology**: `oncology.current_step`, `oncology.hipaa_cleared`/
  `gdpr_cleared` (bool), `oncology.submitted_by`/`approved_by`.

The plugin adds its own derived attributes (e.g. `fraudos.requires_urgent_review`,
`oncology.awaiting_approval`) and, when it detects an anomaly, a
`plugin.warning` entry, visible in the span's `extra_attributes` and counted in
`GET /metrics/summary`'s `spans_with_warnings` (see below). Each plugin's rules:
`docs/interfaces/plugin-contract-v0.md`, `docs/interfaces/oncology-governance.md`.

### Example (Python, standard OTel SDK)

```python
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter

provider = TracerProvider()
provider.add_span_processor(
    BatchSpanProcessor(OTLPSpanExporter(endpoint="localhost:4317", insecure=True))
    # Bearer KERNEL_API_KEY through OTEL_EXPORTER_OTLP_HEADERS, no code here.
)
trace.set_tracer_provider(provider)
tracer = trace.get_tracer("my-agent-app")

with tracer.start_as_current_span("invoke_agent my-agent", kind=trace.SpanKind.INTERNAL) as agent_span:
    agent_span.set_attribute("gen_ai.operation.name", "invoke_agent")
    agent_span.set_attribute("gen_ai.agent.name", "my-agent")
    agent_span.set_attribute("fraudos.transaction_id", transaction_id)
    agent_span.set_attribute("fraudos.final_decision", "CONFIRMED_FRAUD")

    with tracer.start_as_current_span("chat claude-sonnet", kind=trace.SpanKind.CLIENT) as model_span:
        model_span.set_attribute("gen_ai.operation.name", "chat")
        model_span.set_attribute("gen_ai.provider.name", "aws.bedrock")
        model_span.set_attribute("gen_ai.request.model", "claude-sonnet")
        # ... the real call, then:
        model_span.set_attribute("gen_ai.usage.input_tokens", usage.input_tokens)
        model_span.set_attribute("gen_ai.usage.output_tokens", usage.output_tokens)
```

An illustrative skeleton: check the exact `opentelemetry-python` SDK API when
writing it for real, the same rule as everywhere else in this project.

## Querying

Port `8080` by default (`QUERY_API_BIND`), with the header
`Authorization: Bearer <QUERY_API_KEY>` required on all three routes, `401`
otherwise. Full contract: `docs/interfaces/query-api.md`.

### `GET /traces?limit=N`: recent traces

`limit` defaults to 50, capped at 500. An array, sorted by start time, newest
first:

```json
[{ "trace_id": "<32 hex>", "span_count": 3, "start_time_unix_nano": 0, "end_time_unix_nano": 0 }]
```

### `GET /traces/{trace_id}`: a trace's spans

`trace_id` as 32 lowercase hex characters (the same encoding as `traceId` in
OTLP/JSON). `400` if malformed, `404` if there is no span. A **flat list**,
sorted by `start_time`: no nested JSON tree; rebuild it client-side from
`parent_span_id` (present on every span).

Fields per span (`SpanDto`, `crates/query-api/src/dto.rs`): `trace_id`,
`span_id`, `parent_span_id`, `kind` (`model_call`/`tool_call`/`agent_run`),
`start_time_unix_nano`, `end_time_unix_nano`, `status_code`, `status_message`,
`error_type`, `operation_name`, `provider_name`, `request_model`,
`response_model`, `input_tokens`, `output_tokens`, `cache_read_input_tokens`,
`cache_creation_input_tokens`, `finish_reasons`, `conversation_id`, `cost_usd`,
`tool_name`, `tool_call_id`, `tool_type`, `tool_description`,
`agent_invocation_kind`, `agent_name`, `agent_id`, `agent_description`,
`agent_version`, and `extra_attributes` (a `string → string` object: everything
that is not a first-class field, including `<vertical>.*` attributes and
`plugin.warning`).

`cost_usd` (added 2026-08-17): `null` unless the span sets
`gen_ai.usage.input_tokens`/`output_tokens` **and** a `gen_ai.request.model`/
`response.model` known to the static price table in `crates/pricing` (today:
part of the OpenAI and Anthropic models only; details and known gaps in
`docs/interfaces/cost-calculation.md`). Computed once at ingestion, never
recomputed after a price change.

### `GET /metrics/summary`: aggregates

```json
{
  "by_kind": [{ "kind": "agent_run", "span_count": 1, "total_input_tokens": 8420, "total_output_tokens": 1150, "total_cost_usd": 0.126 }],
  "spans_with_warnings": 0
}
```

`total_cost_usd`: `null` (not `0.0`) when no span of the group has a computed
cost, which is distinct from a real cost of zero.

`spans_with_warnings`: the number of spans carrying at least one
`plugin.warning` entry, the governance and monitoring signal produced by the
plugins wired into `kernel` (`crates/plugin-sink`).

## Retention

Spans are deleted 90 days after their `start_time` (ClickHouse TTL,
`docs/interfaces/clickhouse-retention.md`). A span sent with a `start_time`
older than that, for instance a replay of old traces, is expired on arrival and
disappears at the next background merge.

## Deploying

Not repeated here:
- `scripts/dev-stack.sh up`: the full Docker stack locally
  (`docker/docker-compose.stack.yml`).
- `crates/orchestrator`: a small control plane for the same three services,
  through an HTTP API (`POST /deploy`, `GET /status`, `POST /teardown`) rather
  than scripts: `docs/interfaces/docker-engine-api.md`.

## Going further

Each contract above has its full sheet (verified source, date, remaining
uncertainties) in `docs/interfaces/`: `otlp-ingestion.md`, `semconv-genai.md`,
`kernel-auth.md`, `query-api.md`, `plugin-contract-v0.md`,
`oncology-governance.md`, `fraudos-agentspan.md`.

# otlp-ingestion: the OTLP/traces receiver contract (kernel step 2)

- Authoritative source: https://github.com/open-telemetry/opentelemetry-proto
  (`opentelemetry/proto/collector/trace/v1/trace_service.proto`,
  `opentelemetry/proto/trace/v1/trace.proto`,
  `opentelemetry/proto/common/v1/common.proto`,
  `opentelemetry/proto/resource/v1/resource.proto`,
  `docs/specification.md`)
- Pinned version/commit: tag `v1.11.0` (`790608c4d51e6ffc12210b541e8514cbed9e91a4`),
  **a repository with real release tags**, unlike `semconv-genai`.
- Verification date: 2026-08-14
- Pinned with: `scripts/pin-otlp-proto.sh v1.11.0` → `vendor/opentelemetry-proto/`
- Scope of this contract: the **traces** signal only (dossier section 2.1: the
  three MVP events are all spans, see `docs/interfaces/semconv-genai.md`). OTLP
  logs and metrics are out of scope.
- **Authentication (added 2026-08-15)**: `TraceService.Export` requires a gRPC
  metadata header `authorization: Bearer <KERNEL_API_KEY>`, `UNAUTHENTICATED`
  otherwise. The convention matches a real OTel SDK's
  `OTEL_EXPORTER_OTLP_HEADERS`; full contract in `docs/interfaces/kernel-auth.md`.

## gRPC service

```
service TraceService {
  rpc Export(ExportTraceServiceRequest) returns (ExportTraceServiceResponse) {}
}
```

- Default OTLP/gRPC port: **4317**.
- `tonic` implements this service directly from the `.proto`: it is the step 2
  receiver's entry point.

## Shape of the received message (exact nesting)

```
ExportTraceServiceRequest
  └─ repeated ResourceSpans
       ├─ resource: Resource { repeated KeyValue attributes, uint32 dropped_attributes_count, repeated EntityRef entity_refs }
       ├─ repeated ScopeSpans
       │    ├─ scope: InstrumentationScope { name, version, repeated KeyValue attributes, dropped_attributes_count }
       │    └─ repeated Span
       └─ schema_url: string
```

A single `Export` call can therefore carry spans from several resources and
several scopes: validation and persistence at step 2 must iterate over these three
nesting levels, not assume "one request = one span".

## `Span`: exact fields and types

| Field | Protobuf type | Constraint |
|---|---|---|
| `trace_id` | `bytes` | **required**, exactly 16 bytes; all zeros or length ≠ 16 = **invalid** |
| `span_id` | `bytes` | **required**, exactly 8 bytes; all zeros or length ≠ 8 = **invalid** |
| `parent_span_id` | `bytes` | empty = root span |
| `name` | `string` | semantically required (empty = "unknown name", not a hard error) |
| `kind` | `enum SpanKind` | `UNSPECIFIED=0, INTERNAL=1 (default), SERVER=2, CLIENT=3, PRODUCER=4, CONSUMER=5` |
| `start_time_unix_nano` / `end_time_unix_nano` | `fixed64` | Unix ns; **"expected end_time >= start_time" but not guaranteed by the protocol** |
| `attributes` | `repeated KeyValue` | keys meant to be unique; **"unpredictable behaviour" if duplicated**, not specified by OTLP, the receiver's responsibility |
| `dropped_attributes_count` | `uint32` | — |
| `events` / `links` | `repeated Span.Event` / `repeated Span.Link` | not used by the three `gen_ai.*` spans kept for the MVP (content lives in attributes, not in span events) |
| `status` | `Status { message: string, code: enum{UNSET=0,OK=1,ERROR=2} }` | the span's logical status, distinct from the `error.type` attribute |

**Where it meets `semconv-genai.md`**: the `kind: client` / `kind: internal`
declared in `spans.yaml` (e.g. `gen_ai.inference.client` vs
`gen_ai.execute_tool.internal`) maps directly to this `Span.kind` field: literally
the same concept expressed in both specs, not a naming coincidence to re-check.

## `AnyValue` / `KeyValue` (attributes)

```protobuf
message AnyValue {
  oneof value {
    string string_value = 1;
    bool bool_value = 2;
    int64 int_value = 3;
    double double_value = 4;
    ArrayValue array_value = 5;
    KeyValueList kvlist_value = 6;
    bytes bytes_value = 7;
    int32 string_value_strindex = 8; // [Alpha] — Profiling signal only
  }
}
message KeyValue {
  string key = 1;
  AnyValue value = 2;
  int32 key_strindex = 3; // [Alpha] — Profiling signal only
}
```

- `int_value` confirms what `semconv-genai.md` had noted as an uncertainty:
  **signed `i64`**, not `u64`.
- `string_value_strindex` and `key_strindex` are explicitly reserved for the
  Profiling signal (*Alpha* status). The spec says it plainly: a receiver of
  another signal (us, traces) **must treat their presence as a non-fatal
  anomaly**: log it and proceed as if the field were absent, not try to interpret
  it.

## HTTP transport (if the receiver exposes it next to gRPC)

- Default path: `POST /v1/traces`, default port **4318**.
- Two possible encodings, told apart by `Content-Type` (client and server MUST use
  the same):
  - `application/x-protobuf`: binary, the same schema as gRPC.
  - `application/json`: the standard Protobuf JSON mapping **with 3
    exceptions**: `traceId`/`spanId` in hex (not base64), enums as integers (not
    names), and **64-bit integers encoded as JSON strings** (so
    `gen_ai.usage.input_tokens` in JSON is not a `number` but a string such as
    `"100"`; a naive JSON parser expecting a native numeric type gets it wrong).
  - Unknown JSON fields MUST be ignored silently (forward compatibility), not
    rejected.
- **Recommended** request size limit: 64 MiB (after decompression), `HTTP 413`
  beyond. Recommended response limit: 4 MiB.
- `Content-Encoding: gzip` optionally supported.

## Response semantics: partial success

```protobuf
message ExportTraceServiceResponse { ExportTracePartialSuccess partial_success = 1; }
message ExportTracePartialSuccess { int64 rejected_spans = 1; string error_message = 2; }
```

The protocol explicitly provides for **partial success**: `HTTP 200` / gRPC OK
even when some spans were rejected, with `rejected_spans` set and an optional
`error_message`. So "validate and persist raw" (step 2) must handle a batch span
by span and count rejections, rather than accept or reject the whole
`ExportTraceServiceRequest` at once.

## Deliberately ignored in the MVP (and why)

- OTLP logs and metrics signals (`logs_service.proto`, `metrics_service.proto`):
  out of scope; only the traces signal carries the three `gen_ai.*` spans kept.
- `Span.events` / `Span.links`: not referenced by `gen_ai.inference.client`,
  `gen_ai.execute_tool.internal`, `gen_ai.invoke_agent.{client,internal}` in the
  pinned spec; to revisit if a future gen_ai increment uses them.
- `EntityRef` (on `Resource`): *Development* status in opentelemetry-proto
  itself, not needed for the three MVP spans.
- OTLP/HTTP+JSON encoding: the dossier plans `tonic` (gRPC) for step 2. Binary
  protobuf over HTTP can easily be added later (same schema); JSON needs an extra
  conversion layer (int64 as strings, hex ids) that is not needed until a real
  OTLP/HTTP+JSON client is identified.

## Uncertainties / decisions to make explicitly before coding the receiver

- **Duplicate attribute keys in one `KeyValue[]`**: the protocol says
  "unpredictable behaviour", which is a permission, not a spec. The kernel must
  pick a policy (e.g. last occurrence wins, with an anomaly counter logged) and
  document it, rather than let ordering decide by chance.
- **Invalid `trace_id`/`span_id`** (all zeros or wrong length): the spec says
  "considered invalid" but does not dictate the receiver's behaviour. To decide:
  reject the span individually (consistent with the partial-success mechanism
  above) rather than the whole batch.
- **Duplicated whole spans**: the spec documents explicitly, in its "Known
  Limitations", that OTLP clients may resend the same spans after a network cut
  without acknowledgement ("*this is a deliberate choice*"). It is NOT a bug on
  the sender's side. Storage (step 3) must decide whether it is idempotent on
  `(trace_id, span_id)` or accepts duplicates for the MVP; do not discover this
  behaviour in production and take it for an anomaly.
- **gRPC message size limit**: unlike HTTP (64 MiB explicitly recommended in this
  spec), the maximum gRPC message size is not defined here: it is a
  `tonic`/`grpc` configuration parameter, not part of this contract. Check it
  separately in the `tonic` docs before fixing a value, rather than assume a
  default.

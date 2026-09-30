# query-api: the minimal query API contract (kernel step 4)

- Authoritative source: this project's own design (no external spec), but it is
  a real interoperability contract (consumed by future dashboards and the
  analysis layer, dossier section 2.3), documented with the same rigour as the
  external boundaries.
- HTTP framework: `axum` 0.8.9 (already resolved in the workspace through
  `tonic`). Checked before coding: **axum 0.8 uses `{param}` in routes, not
  `:param`**. The old syntax *panics* when the router starts instead of failing
  silently (official changelog, checked on 2026-08-14).
- Verification date: 2026-08-14 (updated 2026-08-15: authentication)
- Scope: 2-3 minimal endpoints (dossier section 2.2 step 4), no rich dashboard.
- **Authentication (added 2026-08-15)**: all three endpoints require an
  `Authorization: Bearer <QUERY_API_KEY>` header, `401 Unauthorized` otherwise.
  Full contract in `docs/interfaces/kernel-auth.md`.

## Endpoints

### `GET /traces?limit=N`

"List recent traces." There is no `traces` table: a trace is derived on the fly
by grouping `spans` by `trace_id` (`min(start_time)`, `max(end_time)`,
`count()`), sorted by start time, newest first. `limit` defaults to 50, capped at
500 on the server. At the time there was no retention policy to bound the scan
otherwise (dossier section 4), so this cap was the only protection against an
absurd `?limit=`.

Response: an array of
```json
{ "trace_id": "<32 hex>", "span_count": 3, "start_time_unix_nano": 0, "end_time_unix_nano": 0 }
```

### `GET /traces/{trace_id}`

"Fetch a trace's tree." `trace_id` as 32 lowercase hex characters, the same
encoding as `traceId` in OTLP/JSON (docs/interfaces/otlp-ingestion.md), not a
convention invented here. Validated (length, not all zeros) through
`kernel_model::TraceId` before any ClickHouse query → `400` if invalid. `404` if
no span has this `trace_id`.

**Decision**: returns a **flat list** of spans (sorted by `start_time`), not a
nested JSON tree. Every span already carries `parent_span_id`: rebuilding the
tree client-side is enough for "no rich dashboard needed" (dossier section 2.2).
Building and validating a real nested structure server-side (multiple roots,
orphaned parents, cycles in malformed data) is more than the MVP asks for.

### `GET /metrics/summary`

"Aggregate a few basic metrics." Span count and token totals per `kind`
(`model_call`/`tool_call`/`agent_run`). No time filter in the MVP.

**`total_cost_usd` (added 2026-08-17)**: the sum of `cost_usd` per `kind`,
`null` (not `0.0`) when no span of the group has a computed cost, a deliberate
distinction between "no priced span" and "a real cost of zero". Full details
(price table, cache accounting per provider, known gaps):
`docs/interfaces/cost-calculation.md`.

## ClickHouse binding decisions (following the uncertainty left at step 3)

`docs/interfaces/clickhouse-schema.md` noted that binding a `[u8; 16]` directly
to a `?` fails (the driver serialises it as a `Tuple`, which ClickHouse refuses to
compare with `FixedString`). Checked against a real server before writing
`queries.rs`: the query `WHERE trace_id = unhex(?)`, binding the **hex
representation (`String`)** rather than the raw bytes, works. `unhex()` converts
server-side, and binding a `String` is a standard driver case, not an exotic
type. This is the approach for any future lookup parameterised by
`trace_id`/`span_id`.

## Deliberately ignored in the MVP

- Pagination beyond a simple `limit` on `/traces` (no cursor or offset).
- A time filter on `/metrics/summary`.
- A search or filter-by-attribute endpoint, outside the "2-3 minimal endpoints"
  asked for.

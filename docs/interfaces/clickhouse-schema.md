# clickhouse-schema: the Rust driver and the storage schema (kernel step 3)

- Authoritative sources:
  - driver: https://docs.rs/clickhouse (crate `clickhouse`, v0.15.1 at the time,
    checked on docs.rs; 0.15.2 since 2026-09-30)
  - schema: this project's own design (not an external spec), but documented
    here with the same rigour because it is the kernel-model ↔ storage boundary
- Verification date: 2026-08-14
- Scope: a single ClickHouse instance (dossier section 2.2 step 3), local at the
  time (`docker/docker-compose.clickhouse.yml`). The production choice
  (self-hosted vs BigQuery/ADX, GCP vs Azure) was still open (dossier section 5;
  since decided in ADR 0003); building against the local ClickHouse did not
  prejudge it.
- **Server version**: pinned to `clickhouse/clickhouse-server:26.8.15.10`
  everywhere since 2026-09-30. ClickHouse 26.9 breaks the `clickhouse` 0.15 client
  during migrations (`Decompression("incorrect magic number")`); change the
  version only after `scripts/test-integration.sh` passes against the new one.

## Rust driver (`clickhouse` crate)

- HTTP transport (port 8123, the one `docker-compose.clickhouse.yml` already
  exposes), not the native protocol (port 9000).
- Construction: `Client::default().with_url(...).with_user(...).with_password(...).with_database(...)`.
- A row struct = `#[derive(Row, Serialize, Deserialize)]` + `serde`.
- Insertion: `client.insert::<Row>("table").await?`, then `.write(&row).await?` for
  each row, then **`.end().await?`, mandatory**: *"If `end()` isn't called, the
  `INSERT` is aborted."* No row-by-row guarantee inside a batch: either the whole
  batch is committed (`end()` succeeds) or none of it is. **Direct
  consequence**: the `SpanSink` cannot stay "one event at a time, infallible" like
  step 2's `InMemorySink` stub; it must become **async, batched (per `Export`
  request), and fallible**.
- Type mapping (the driver's table):

  | ClickHouse type | Rust type |
  |---|---|
  | `(U)Int(8-64)` | `(u)i(8-64)` |
  | `String` | `String` / `&str` |
  | `FixedString(N)` | `[u8; N]` (and `Option<[u8; N]>` for `Nullable(FixedString(N))`) |
  | `DateTime64(_)` | `i64` (raw ticks) or `chrono::DateTime<Utc>` |
  | `Array(_)` | `Vec<_>` |
  | `Map(K, V)` | `HashMap<K, V>` or `Vec<(K, V)>` |
  | `Nullable(_)` | `Option<_>` |

## The schema: a single `spans` table (not one per event type)

**Why one table rather than three**: kernel step 4 must be able to "fetch a
trace's tree", and one `trace_id` often mixes `ModelCallEvent`/`ToolCallEvent`/
`AgentRunEvent` spans as parent and child. A wide table with nullable columns for
each type's specific fields, plus a discriminating `kind` column, avoids a
three-table join to rebuild a tree. It is also standard practice for
OTel-native ClickHouse exporters.

```sql
CREATE TABLE spans
(
    trace_id        FixedString(16),
    span_id         FixedString(8),
    parent_span_id  Nullable(FixedString(8)),
    kind            LowCardinality(String), -- 'model_call' | 'tool_call' | 'agent_run'

    start_time      DateTime64(9, 'UTC'),
    end_time        DateTime64(9, 'UTC'),
    status_code     LowCardinality(String), -- 'unset' | 'ok' | 'error'
    status_message  String,
    error_type      Nullable(String),

    operation_name  LowCardinality(String),
    provider_name   LowCardinality(Nullable(String)),
    request_model   Nullable(String),
    response_model  Nullable(String),
    input_tokens                 Nullable(UInt64),
    output_tokens                Nullable(UInt64),
    cache_read_input_tokens      Nullable(UInt64),
    cache_creation_input_tokens  Nullable(UInt64),
    finish_reasons  Array(String),
    conversation_id Nullable(String),

    tool_name        Nullable(String),
    tool_call_id      Nullable(String),
    tool_type         Nullable(String),
    tool_description  Nullable(String),

    agent_invocation_kind  LowCardinality(Nullable(String)), -- 'client' | 'internal'
    agent_name        Nullable(String),
    agent_id          Nullable(String),
    agent_description Nullable(String),
    agent_version     Nullable(String),

    extra_attributes  Map(String, String)
)
ENGINE = MergeTree
PARTITION BY toYYYYMMDD(start_time)
ORDER BY (trace_id, start_time, span_id)
```

- `ORDER BY (trace_id, start_time, span_id)`: optimises "every span of a trace"
  (step 4); no secondary index needed for that case in the MVP.
- `PARTITION BY toYYYYMMDD(start_time)`: costs nothing to set now even with "no
  fine-grained retention in the MVP" (dossier section 4), and avoids a schema
  migration the day a per-partition retention policy is added. **That day came
  (2026-08-15)**: a 90-day TTL now exists on `start_time`, see
  `docs/interfaces/clickhouse-retention.md`. This partitioning is precisely what
  makes it cheap (whole partitions dropped, not row by row).
- `agent_name` is shared between `ToolCallEvent` (the agent running the tool) and
  `AgentRunEvent` (the agent itself): the same column, with consistent semantics
  in both cases (the dossier does not distinguish them).
- **`cost_usd Nullable(Float64)` (migration `0003_add_cost_usd.sql`,
  2026-08-17)**: the $ cost computed once at ingestion (`crates/pricing`), never
  recomputed afterwards. Full details (price table, cache accounting per
  provider): `docs/interfaces/cost-calculation.md`.

## `extra_attributes`: `Map(String, String)`, not `Map(String, AnyValue)`

`AttributeValue` (kernel-model) can be nested (`Array`, `KeyValueList`), but a
ClickHouse `Map` does not take a practical recursive value type here.
**Decision**: stringify each value (numbers/bools with `to_string()`, bytes in
hex, nested values as a readable but non-reparsable debug representation). This
is **lossy** for nested types, acceptable for the MVP because the only
potentially nested or complex `gen_ai.*` attributes
(`gen_ai.tool.call.arguments`/`result`, type `any`) are `opt_in` and disabled by
default by our own PII policy (dossier section 2.1). To revisit (a native `JSON`
column or a real JSON serialisation) if nested attributes become common.

## Risky conversions (the same reflexes as for ingestion)

- `start_time_unix_nano`/`end_time_unix_nano` (`u64`, OTLP wire) →
  `DateTime64(9)` stored as `i64` by the driver: a **checked** `u64 -> i64`
  conversion (`i64::try_from`), not a bare cast. Safe until 2262, but the cast
  must stay explicit and fallible rather than assumed always correct.
- **All-or-nothing persistence per batch** (see above): a structurally valid span
  that fails to persist must be counted in the OTLP response's `rejected_spans`
  (partial success), like a malformed span. The OTLP client does not need to tell
  "rejected at validation" from "rejected at write": both mean "not persisted".

## Deliberately ignored in the MVP

- No `ReplacingMergeTree`/deduplication: the span duplicates the OTLP protocol
  explicitly allows (docs/interfaces/otlp-ingestion.md, "Known Limitations") are
  not deduplicated in the MVP, consistent with "no fine-grained retention or
  advanced logic" (dossier section 4).
- No separate table for provider extensions (e.g. `aws.bedrock.*`): they live in
  `extra_attributes` like everything else, in line with dossier section 2.1
  ("stored separately... so as not to force a normalisation that would lose the
  information").
- No TLS/rustls connection: plain HTTP to ClickHouse, which sits on an internal
  Docker network in every deployment so far.

## Errors found by testing against a real server (not guessable from the docs)

The initial schema used `Nullable(LowCardinality(String))` for
`provider_name`/`agent_invocation_kind`. ClickHouse 26.7.3 refuses it when
creating the table: *"Nested type LowCardinality(String) cannot be inside Nullable
type (ILLEGAL_TYPE_OF_ARGUMENT)"*. The correct order is
`LowCardinality(Nullable(String))`. Neither the Rust driver's docs nor the dossier
mentioned this constraint; only the real integration test
(`crates/clickhouse-sink/tests/integration.rs`, `--ignored`) revealed it. A
reminder that documenting a contract from the docs does not replace checking it
in real conditions when possible.

A second finding from the same test: binding a `[u8; 16]` (trace_id) directly to a
`?` in a parameterised query serialises it as `Tuple(UInt8, ...)` in the driver,
not as a `FixedString` literal, and ClickHouse answers `NO_COMMON_TYPE` when
comparing with the column. Resolved at step 4: bind the hex string and compare
with `unhex(?)` (see `docs/interfaces/query-api.md`).

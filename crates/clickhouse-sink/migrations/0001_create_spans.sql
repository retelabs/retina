-- Schema decisions documented in docs/interfaces/clickhouse-schema.md — read
-- that before changing this file, especially the "why one table, not three"
-- and "why Map(String, String) for extra_attributes" sections.

CREATE TABLE IF NOT EXISTS spans
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

    tool_name         Nullable(String),
    tool_call_id       Nullable(String),
    tool_type          Nullable(String),
    tool_description   Nullable(String),

    agent_invocation_kind  LowCardinality(Nullable(String)), -- 'client' | 'internal'
    agent_name        Nullable(String),
    agent_id          Nullable(String),
    agent_description Nullable(String),
    agent_version     Nullable(String),

    extra_attributes  Map(String, String)
)
ENGINE = MergeTree
PARTITION BY toYYYYMMDD(start_time)
ORDER BY (trace_id, start_time, span_id);

-- Retention (docs/interfaces/clickhouse-retention.md): 90 days, chosen with
-- the owner, long enough to investigate an incident after the fact without
-- accumulating forever. `partition by toYYYYMMDD(start_time)` was already in
-- place since migration 0001 precisely so that this costs a single command:
-- ClickHouse can drop whole partitions once expired, not row by row (checked
-- against the real ClickHouse documentation before writing this migration).
--
-- Deletion is not instantaneous at expiry: ClickHouse purges expired rows
-- during background merges, not synchronously.
ALTER TABLE spans
    MODIFY TTL start_time + INTERVAL 90 DAY DELETE;

# clickhouse-retention: retention and schema evolution

- Authoritative source: the ClickHouse documentation on table TTLs
  (https://clickhouse.com/docs/engines/table-engines/mergetree-family/mergetree),
  checked on 2026-08-15 before writing the migration: `TTL expr DELETE` syntax at
  creation, `ALTER TABLE ... MODIFY TTL expr` on an existing table. The behaviour
  of `max()` on an empty table (`0`, not `NULL`, for a `UInt32` column) was
  verified empirically against a real local server (no firm answer found in the
  docs themselves).
- Retention period (90 days): decided with the owner, long enough to investigate
  an incident after the fact without accumulating forever.
- Verification date: 2026-08-15

## Retention: a TTL on `spans`

`crates/clickhouse-sink/migrations/0002_spans_retention_ttl.sql`:

```sql
ALTER TABLE spans
    MODIFY TTL start_time + INTERVAL 90 DAY DELETE;
```

Cheap precisely because `PARTITION BY toYYYYMMDD(start_time)` already existed
since migration 0001 (`docs/interfaces/clickhouse-schema.md` anticipated it
explicitly): ClickHouse can drop whole partitions once expired rather than
deleting row by row.

**What the TTL does not guarantee**: deletion is not synchronous with expiry.
"Data with an expired TTL is removed when ClickHouse merges data parts" (official
docs). An expired row can stay visible until the next background merge
(adjustable with `merge_with_ttl_timeout`, left at the server default here).
`OPTIMIZE TABLE spans FINAL` would force an immediate purge if ever needed in
operation, but it is not automated.

**Consequence found on 2026-09-30**: a span whose `start_time` is already more
than 90 days old is expired on insert and vanishes at the next merge. Test
fixtures dated 1970 made one integration test race that merge; fixtures now use
recent timestamps and unique trace ids (see `scripts/test-integration.sh`).

## Schema evolution: `schema_migrations` plus numbered files

Before this step there was a single migration file, applied at every start
through `CREATE TABLE IF NOT EXISTS`: idempotent by luck, not by design. That no
longer held once a second migration (`ALTER TABLE ... MODIFY TTL`, and perhaps
`ADD COLUMN` one day) had to be applied exactly once, not replayed blindly at
every start.

`crates/clickhouse-sink/src/migrate.rs` (`run_migrations`, exported by the
crate):
- A `schema_migrations (version UInt32, name String, applied_at DateTime
  DEFAULT now()) ENGINE = MergeTree ORDER BY version` table, created if missing.
- `SELECT max(version)` (→ `0` on a new table) gives the current version; every
  migration with a higher version is applied in order, then recorded.
- The migrations themselves are a static `(version, name, sql)` list in
  `migrate.rs`, with `sql` loaded through `include_str!` from
  `migrations/NNNN_*.sql`: one place knows the order, and `include_str!` is no
  longer duplicated across `crates/kernel` and three integration test suites (as
  it was before this step).
- A unit test (`migrate::tests::migrations_are_numbered_sequentially_from_one`)
  checks that the list stays `1, 2, 3, ...` with no gap or duplicate: an
  author's mistake caught before any deployment, not only in production.

**Single-instance assumption, accepted**: two processes applying the same
migration at the same time are not handled (no distributed lock), consistent
with the design dossier section 4 (no high availability in the MVP). Only one
kernel process exists today.

## How it was verified

Against a real local ClickHouse (`scripts/dev-clickhouse.sh up`), not only by
reading the docs:
- `SELECT max(version) FROM <empty table>` → confirmed `0`, not `NULL`.
- A completely fresh database (`spans`/`schema_migrations` dropped):
  `cargo test --workspace -- --ignored` green, both migrations applied and
  recorded (`schema_migrations` holds versions 1 and 2), and
  `system.tables.engine_full` confirms `TTL start_time + toIntervalDay(90)` on
  `spans`.
- **A real upgrade scenario**: `spans` recreated without a TTL and
  `schema_migrations` dropped (simulating a deployment older than this feature),
  then `cargo run -p kernel` actually started against that database. Migration 2
  applies automatically at startup with no manual step, and `system.tables`
  confirms the TTL afterwards.

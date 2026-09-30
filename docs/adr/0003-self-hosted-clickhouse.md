# 0003. Self-hosted ClickHouse, no managed service

Date: 2026-08-16 (built this way since step 3 of the kernel, 2026-08-14;
recorded here)

Status: Accepted

## Context

The design dossier (section 5) gave a volume reference for the trade-off:
managed storage (BigQuery/ADX) wins on total cost below ~100k requests a day,
self-hosting becomes attractive above ~1M a day if ops bandwidth follows. The
kernel was built and validated from step 3 against a local self-hosted ClickHouse
(`docker/docker-compose.clickhouse.yml`), and the choice was never questioned
since: steps 3 to 7, retention (an implicit ADR in
`docs/interfaces/clickhouse-retention.md`) and the control plane
(`crates/orchestrator`) are all built on it.

## Decision

Self-hosted ClickHouse (a Docker container we manage, orchestrated by
`crates/orchestrator`), not ClickHouse Cloud, BigQuery or ADX. Retention is
managed by us (90-day TTL, versioned migration,
`docs/interfaces/clickhouse-retention.md`).

## Consequences

`crates/cost-model` has since costed this choice for real, not only in theory: at
the dossier's reference volume (100k spans a day), the storage accumulated over
the retention window (90 days) stays under the free tier of a backup object store
(`docs/cost-model.md`), far below what a managed service would bill per
instance, whatever the real usage. The dossier's volume reference still holds in
theory, but the criterion that actually decides is different (ADR-0002): a
managed service bills provisioned capacity even at low usage, which the project
now avoids on principle, not only because the current volume is under a
threshold.

The accepted downside: no managed scaling or replication, no provider support,
and scaling to handle ourselves if the volume ever exceeds what one instance can
take (dossier reference: above ~1M a day, to revisit).

## Alternatives considered

- **BigQuery / ADX** (managed): set aside for the zero-usage cost criterion
  (ADR-0002): billed per instance or per reserved slot depending on the product,
  not strictly per use.
- **ClickHouse Cloud**: same reason, plus a direct dependency on one specific
  provider, against the spirit of ADR-0002.

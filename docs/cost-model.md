# cost-model: price sources and method (`crates/cost-model`)

Not a technical contract like `docs/interfaces/` (no API, no protocol), but the
same discipline: every figure carries its source and date, nothing is guessed.
The second "cloud learning" piece of work (design dossier section 5), after
`crates/orchestrator`. The goal: make the criterion set on 2026-08-15 (compare by
**cost at zero usage**) checkable with real numbers, not only stated.

## Bytes per span: measured, not estimated

An analytic estimate from the column types (`docs/interfaces/clickhouse-schema.md`)
would ignore ClickHouse's real compression (`LowCardinality`, default column
compression) and overestimate by far. `crates/cost-model/src/measure.rs` queries
`system.parts` (`sum(data_compressed_bytes) / sum(rows)`, filtered on the
`spans` table, active parts) against a real server; the columns were confirmed
with `DESCRIBE TABLE system.parts` before writing the query, not guessed.

Measured on 2026-08-16 against 15 real spans (a full replay of the
`fraudos-replay` and `oncology-replay` fixtures, after `OPTIMIZE TABLE spans
FINAL` for a representative compression state rather than freshly inserted,
unmerged parts): **≈304 compressed bytes per span**. The sample is deliberately
small (the only real data available in that environment): a verified order of
magnitude, not a statistically solid average. `cargo run -p cost-model`
re-measures on every run against the ClickHouse instance it is connected to;
the number is not hard-coded.

## Prices verified on 2026-08-16 against the official pages

A first search through aggregator sites gave contradictory figures for Hetzner
(between €3.79 and €5.49 a month depending on the source); they were set aside
in favour of the official documentation.

- **VM: Hetzner Cloud CX23** (2 vCPU / 4 GB RAM / 40 GB disk included),
  €5.49 a month excluding IPv4 and VAT. Source:
  docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/.
  The plan was called CX22 before the mid-2026 price adjustment renamed it CX23,
  exactly the kind of drift that justifies re-checking before any real decision
  rather than trusting a figure read once.
- **Object storage (ClickHouse backups): Backblaze B2**: $6.95 per TB a month
  (≈$0.00695 per GB a month), 10 GB free, free egress up to 3x the stored volume,
  then $0.01 per GB. Source: backblaze.com/cloud-storage/pricing. Egress is not
  modelled here: this work only computes storage cost.
- **CDN/edge: Cloudflare**: free plan, unmetered CDN (unlike Workers/compute,
  which are metered): €0 at any realistic volume for this project, not only at
  zero volume. Source: cloudflare.com/plans.
- **Container registry: GitHub Container Registry** (`ghcr.io/retelabs`, since
  the 2026-09-30 migration): free for public packages; for private ones, the
  organisation Free plan includes 500 MB of storage and 1 GB of transfer a
  month, blocked beyond that without a payment method. Source: docs.github.com,
  billing, "GitHub Packages". CI pushes an image only on a `v*` tag or by hand,
  to stay within that quota.

## What the model computes

For a given volume (spans a day), at the steady state of the real retention
window (90 days, `crates/clickhouse-sink/migrations/0002_spans_retention_ttl.sql`):

- VM cost: the same for "all self-built" and "hybrid", and dominant while the
  volume stays low (see `cargo run -p cost-model` with no argument, which prints
  the reference points `0`/`1 000`/`100 000`/`1 000 000` spans a day; `100 000` is
  the dossier's reference for the managed vs self-hosted ClickHouse trade-off).
- Days before the VM's included disk fills up at that volume, derived from the
  same measured bytes per span: a real operational question that can now be
  costed rather than guessed.
- The cost of the only hybrid addition whose price depends on volume (object
  storage): `$0` below Backblaze's free tier, which comfortably covers the
  volumes realistic for this project today.

Verified at two levels: unit tests on `report.rs` (pure functions, no Docker or
ClickHouse: zero volume cancels the hybrid extras, growth is monotonic, an
included-disk overflow is detected); and an `--ignored` test that really measures
against a ClickHouse server (`crates/cost-model/tests/integration.rs`),
confirming a positive, plausible result, not only that the query does not fail.

## What it does not cover

- Object-storage egress (downloads): beyond 3x the stored volume at Backblaze,
  not modelled.
- The compute (CPU, bandwidth) actually used by `kernel`, `query-api` and
  `orchestrator` themselves: the VM is treated as a single fixed cost, not broken
  down by service.
- Other VM providers (OVH, Scaleway, DigitalOcean, mentioned in the 2026-08-15
  discussion): Hetzner was picked as a first verified reference point, not as a
  final choice of provider.

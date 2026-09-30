# 0001. Multi-tenancy out of the MVP's scope

Date: 2026-08-16 (written after the fact: the decision itself dates from the
very start of the project, design dossier section 4; recorded here to close the
gap between the planned `/adr` tooling and its actual use so far)

Status: Accepted

## Context

The design dossier (section 5) asked from the start: "single-tenant to validate
one vertical, or multi-tenancy designed in from the start of storage and plugin
isolation?" The kernel had to be built and validated quickly against a real
vertical (fraudos, step 7), without diverting effort into a multi-tenant
architecture whose need was not, and still is not, demonstrated by a second real
client or tenant.

## Decision

Single-tenant for the whole MVP (design dossier section 4, explicitly listed as
"deliberately out of scope"). No tenant isolation in the storage schema (table
`spans`, no `tenant_id` column), no segmentation in plugin execution
(`crates/plugin-sink` treats every event the same way), one set of
authentication secrets per surface (`KERNEL_API_KEY`/`QUERY_API_KEY`/
`ORCHESTRATOR_API_KEY`, not one token per tenant).

## Consequences

It simplifies everything built since: the ClickHouse schema, static shared-secret
authentication rather than a real identity system (JWT/OAuth), one kernel
instance per deployment. The accepted downside: moving to multi-tenancy later
means revisiting the storage schema (a tenant partition column plus `ORDER BY`),
authentication (one token per tenant, or real identity infrastructure), and
possibly plugin isolation (a plugin must not see another tenant's data). It is
not a wall: it is a deferred cost, taken on knowingly rather than discovered
afterwards.

## Alternatives considered

Designing multi-tenant storage and authentication from the start: set aside. No
second client or tenant exists yet, and guessing the right shape of isolation
without a second real use case would probably have produced the wrong
abstraction, the classic risk of over-designing before a second concrete example
can validate it.

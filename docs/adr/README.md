# Architecture Decision Records

One ADR per structural decision, in particular the open questions listed in
section 5 of [design-dossier.md](../../design-dossier.md).

## Index

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-multi-tenant-out-of-mvp-scope.md) | Multi-tenancy out of the MVP's scope | Accepted |
| [0002](0002-hosting-model.md) | Hosting model: self-hosted on bare VMs, provider deferred | Accepted (principle); provider not decided |
| [0003](0003-self-hosted-clickhouse.md) | Self-hosted ClickHouse, no managed service | Accepted |
| [0004](0004-native-plugin-loading.md) | Plugin loading: native Rust trait, WASM deferred | Accepted (native mode); WASM deferred, not rejected |

All four were written on 2026-08-16, after the fact: the decisions themselves
had already been made and implemented over the course of the project (see
`CLAUDE.md` for the chronology of each). Writing them down decided nothing new.

Use `/adr <title>` to create a new entry from [0000-template.md](0000-template.md).
Number sequentially (`0005-...`, ...). Decisions with a significant product or
cost impact (target cloud, multi-tenancy) are presented with their trade-offs
and wait for confirmation before being marked "Accepted".

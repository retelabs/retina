# Interface contract sheets

Every external or inter-component boundary of the kernel has a sheet here,
written **after reading the real documentation** (not from memory, not by
assumption) and **before** writing the code that implements it. This is the
concrete application of the project's standing rule (see
[CLAUDE.md](../../CLAUDE.md)).

Use `/contract <topic>` to produce a sheet.

## Sheet format

```markdown
# <topic>

- Authoritative source: <repo/spec/doc, with URL>
- Pinned version/commit: <exact ref, or "N/A" if the spec is not versioned>
- Verification date: <YYYY-MM-DD>

## Fields/behaviours used by the kernel

<precise list: exact name, type, required/optional, default value>

## Deliberately ignored (and why)

## Remaining uncertainties / to re-validate before production
```

## Sheets

| Sheet | Boundary |
|---|---|
| [`otlp-ingestion.md`](otlp-ingestion.md) | gRPC requests accepted by the receiver |
| [`semconv-genai.md`](semconv-genai.md) | `gen_ai.*` attributes kept for the MVP |
| [`fraudos-agentspan.md`](fraudos-agentspan.md) | The fraudos AgentSpan format and its replay |
| [`clickhouse-schema.md`](clickhouse-schema.md) | Storage schema and write guarantees |
| [`clickhouse-retention.md`](clickhouse-retention.md) | Retention TTL and schema migrations |
| [`query-api.md`](query-api.md) | The HTTP query API |
| [`kernel-auth.md`](kernel-auth.md) | Bearer-token authentication on every surface |
| [`plugin-contract-v0.md`](plugin-contract-v0.md) | The native plugin trait |
| [`oncology-governance.md`](oncology-governance.md) | The medical governance plugin's rules |
| [`triage-eval-plugin.md`](triage-eval-plugin.md) | The triage evaluation plugin |
| [`wasm-plugin-loading.md`](wasm-plugin-loading.md) | The WASM plugin host (exploration, not wired in) |
| [`cost-calculation.md`](cost-calculation.md) | Per-span cost from a static price table |
| [`docker-engine-api.md`](docker-engine-api.md) | The Docker Engine API used by the orchestrator |
| [`caddy-reverse-proxy.md`](caddy-reverse-proxy.md) | Caddy as the TLS front |

The list is not fixed: add a sheet for any other boundary met along the way.

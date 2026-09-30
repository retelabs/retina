<p align="center"><img src="UI/assets/logos/retina-mark-dark.svg" width="160" alt="Retina logo: an R drawn as a music staff wound into a spiral galaxy"></p>

# Retina

Observability for LLM agent workflows, written in Rust. Retina ingests standard
OTLP/gRPC traces, stores them in ClickHouse, and runs domain plugins that check
real governance invariants on every span (fraud investigation, HIPAA/GDPR
compliance, triage quality). Any OpenTelemetry SDK works on the client side:
no custom code.

- **Use Retina from your application** → [`docs/client-integration.md`](docs/client-integration.md)
- **Product and architecture decisions** → [`design-dossier.md`](design-dossier.md) and [`docs/adr/`](docs/adr/)
- **Verified technical contracts** (OTLP, ClickHouse, auth, plugins...) → [`docs/interfaces/`](docs/interfaces/)
- **Deploy an instance** → `scripts/dev-stack.sh up`, or `crates/orchestrator` (a small control plane with an HTTP API)

## Quick start

```bash
scripts/dev-stack.sh up      # ClickHouse + kernel + query-api, in containers
scripts/demo.sh              # replays real scenarios, then queries them end to end
```

Kernel (OTLP/gRPC): `localhost:4317`. Query API (HTTP): `localhost:8080`.
Both require a token ([`docs/client-integration.md`](docs/client-integration.md#sending-telemetry)).

## Terminal UI (`crates/tui`)

A strict mirror of the three `query-api` endpoints: recent traces, one trace in
detail (span tree rebuilt client-side), and a metrics summary (tokens, cost,
warnings). No new endpoint, only an interface over what already exists. A paged
intro presents Retina on first launch (skippable at any time); the same content
stays available in the "Help" tab.

```bash
QUERY_API_URL=http://localhost:8080 QUERY_API_KEY=<your token> \
  cargo run -p tui
```

Keys: `Tab` switches view (Traces/Detail/Metrics/Help), `↑`/`↓` (or `j`/`k`)
selects a trace, `Enter` opens it, `←`/`→` pages through help, `r` refreshes the
current view, `Esc` goes back to the list, `q` quits.

## Development

```bash
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/test-integration.sh [--with-docker]   # every integration test, from scratch
```

## Licence

Apache-2.0, see [LICENSE](LICENSE) and [NOTICE](NOTICE).

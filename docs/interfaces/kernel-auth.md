# kernel-auth: authentication on the exposed HTTP/gRPC surfaces

- Authoritative source: two contracts verified separately before writing the
  code, because each surface has its own native auth mechanism, not a single
  external contract.
  - OTLP client header convention: the OpenTelemetry spec
    (https://opentelemetry.io/docs/specs/otel/protocol/exporter/), checked on
    2026-08-15. `OTEL_EXPORTER_OTLP_HEADERS` (and the `_TRACES_HEADERS`
    variants) accept `key=value` pairs in the W3C Baggage style, e.g.
    `OTEL_EXPORTER_OTLP_HEADERS="authorization=Bearer <token>"`. A real OTel SDK
    therefore needs no custom code to authenticate against our receiver, only
    this environment variable.
  - The `tonic::service::Interceptor` API and `TraceServiceServer::with_interceptor`:
    checked against the real generated code (`tonic`/`tonic-prost` 0.14.6,
    inspected in `target/.../out/opentelemetry.proto.collector.trace.v1.rs`
    after a real `cargo build`, not from the docs alone):
    ```rust
    pub trait Interceptor {
        fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status>;
    }
    impl<T> TraceServiceServer<T> {
        pub fn with_interceptor<F>(inner: T, interceptor: F) -> InterceptedService<Self, F>
        where F: tonic::service::Interceptor;
    }
    ```
  - The `axum::middleware::from_fn_with_state` API (axum 0.8.9): checked against
    docs.rs for that exact version. Signature `FnMut(State<S>, Request, Next) ->
    impl Future<Output = impl IntoResponse>`, applied to the router through
    `.route_layer(...)`.
- Verification date: 2026-08-15 (updated 2026-08-16: extended to
  `crates/orchestrator`, the same axum mechanism as `query-api`, nothing new to
  verify)

## Decision: a static shared secret, not JWT/OAuth

The dossier explicitly excludes multi-tenancy from the MVP (section 4). One
static shared token per surface is proportionate to a single-tenant kernel; real
identity infrastructure (JWT, OAuth, key rotation) would be an extra dependency
to operate for a need that does not exist yet. Decided with the owner; to
revisit if and when multi-tenancy is decided.

**Separate secrets, not one**, because ingestion (write) and the query API (read)
are not the same level of trust: a service pushing spans does not need to read
back all the persisted data, and the reverse.

- `KERNEL_API_KEY` protects `crates/otlp-receiver` (the gRPC service
  `TraceService.Export`, listening on `:4317` by default).
- `QUERY_API_KEY` protects `crates/query-api` (the three HTTP endpoints,
  `:8080`).
- `ORCHESTRATOR_API_KEY` protects `crates/orchestrator` (`POST /deploy`,
  `GET /status`, `POST /teardown`, `:9000` by default on `127.0.0.1` only). A
  third secret for the same reason as the first two: this surface can stop
  containers, yet another level of trust than reading (`query-api`) or writing
  spans (`kernel`).

## Mechanism

Every surface expects the same header, `authorization: Bearer <token>`: gRPC
through the request metadata, HTTP through the standard header. The same
convention on both sides keeps configuration symmetric
(`OTEL_EXPORTER_OTLP_HEADERS` for an OTLP client, a plain HTTP header for a
query-api client).

- **Fail closed**: if `KERNEL_API_KEY`/`QUERY_API_KEY`/`ORCHESTRATOR_API_KEY` is
  unset at startup, **or set but empty or blank** (`KERNEL_API_KEY=` in a
  `.env`, which would accept a bare `Bearer `), the binary refuses to start
  (`primary_api_key`, an explicit panic) rather than run without authentication.
  A security control that can be left off by default is not one.
- **Constant-time comparison**: comparing the received token with the expected
  one must not leak timing information (byte by byte with an early return is a
  classic side channel). A small `constant_time_eq` function local to each crate
  rather than an external dependency for a single comparison.
- **Rejection**: `tonic::Status::unauthenticated(...)` for gRPC,
  `StatusCode::UNAUTHORIZED` (401) for HTTP.

## Several valid tokens at once: done (2026-09-09)

A second real client arrived: the same reasoning as `ENABLED_PLUGINS`/
`TRIAGE_KNOWN_SERVICES` elsewhere in this kernel, generalise when a second real
case arrives, not before. `KERNEL_API_KEY`/`QUERY_API_KEY` remain the
**required** tokens (fail-closed unchanged); `KERNEL_API_KEYS_EXTRA`/
`QUERY_API_KEYS_EXTRA` (optional, comma-separated) add further valid tokens, one
per client, each revocable on its own without breaking the others. Still no
fine-grained authorisation (see below): any token in the list gives the same full
access to the surface, a distinct credential identity, not a different scope.

`ApiKeyInterceptor`/`ExpectedBearer` now compare against a `Vec<String>` rather
than a single `String`: each expected token is compared in constant time,
**without short-circuiting on the first match** (the `fold` walks the whole list
even after a success), so as not to leak which token of the list matched.

`crates/orchestrator` is deliberately not extended: an internal tool
(`127.0.0.1` by default), with no second external client to give a separate token
to for now.

## What it does not cover

- Token rotation (deploying a new token that replaces an old one without
  downtime): still not needed; `KERNEL_API_KEYS_EXTRA`/`QUERY_API_KEYS_EXTRA`
  cover adding a token, not hot replacement.
- Fine-grained authorisation (which client can read which traces): not relevant
  while there is a single tenant.
- Channel encryption (TLS): outside this contract, orthogonal to
  authentication. Without TLS the token travels in clear on the network,
  acceptable for `docker-compose.stack.yml` (local dev/demo, one VM,
  inter-container traffic on the internal Docker network). **Resolved for public
  deployment** (2026-08-20): `docker/docker-compose.prod.yml` and
  `docker/Caddyfile` terminate TLS (automatic Let's Encrypt) in front of
  `kernel`/`query-api`, which no longer publish a port on the host; see
  `docs/interfaces/caddy-reverse-proxy.md`.

## How it was verified

`cargo test --workspace` (unit) and `cargo test -p query-api -- --ignored`
against a real ClickHouse cover rejection and acceptance through
`tower::ServiceExt::oneshot`. Real end to end: `scripts/dev-stack.sh up` (fixed
dev tokens in `docker/docker-compose.stack.yml`, the same approach as the
existing `CLICKHOUSE_PASSWORD: dev`), then `scripts/demo.sh`, which sends the
`Authorization` header on every `curl` call and every
`fraudos-replay`/`oncology-replay` run.

**`crates/orchestrator` (2026-08-16)**: `cargo test -p orchestrator -- --ignored`
(`tests/api_integration.rs`) covers the same rejection and acceptance against the
real `Router`. Real end to end: `cargo run -p orchestrator` started without
`ORCHESTRATOR_API_KEY` panics before any Docker connection; started with it,
`curl` without the header confirmed `401`, then with the right header a complete
`/deploy`→`/status`→`/teardown` cycle, a real gRPC replay (`fraudos-replay`) and a
real `query-api` request confirmed that the stack deployed behind the auth really
works, not only `status: "Healthy"`.

**Several tokens (2026-09-09)**: new unit tests in
`crates/otlp-receiver/src/auth.rs` (accepts any token of a set, rejects a token
outside it) and `crates/kernel/src/main.rs`/`crates/query-api/src/main.rs`
(`parse_extra_tokens`: unset, empty string, commas with spaces). A new
`query-api` integration test (`a_second_client_with_its_own_token_can_also_authenticate`,
against a real ClickHouse): two distinct tokens given to `build_app`, both
accepted independently, a third one still rejected.

**Blank keys (2026-09-30)**: `primary_api_key` in each binary, with a unit test
each (unset, empty, whitespace-only refused).

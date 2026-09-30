# Design dossier: a hybrid observability system for agentic workflows

**Status**: scoping done, ready to start the kernel MVP.
**Audience**: the development agent.
**Purpose of this dossier**: give the agent all the decision context it needs to start the kernel without reopening the choices already made.

> Written before development started; the answers to its open questions are in
> [`docs/adr/`](docs/adr/). It is kept as the original brief.

---

## 1. Product vision

A hybrid observability system inspired by OpenTelemetry and Datadog, written in Rust, along two combined axes:

- **Axis A: observability for agentic systems.** Trace and monitor LLM agent workflows themselves (model calls, tool calls, reasoning chains, token costs, latency per step), with an analysis layer that itself uses AI agents for correlation and diagnosis (root cause analysis, anomaly detection).
- **Axis C: ready-to-use business plugins.** A generic core (kernel) plus plugins specialised by domain (fintech first, then other verticals) that know how to interpret domain-specific metrics and attributes.

Target production deployment: GCP or Azure (not decided yet, see section 6).

---

## 2. Validated architecture decisions

### 2.1 Data model

- **Aligned on the OTel GenAI semantic conventions**, but **pinned to a precise version** (a commit or tag given in the code), because these conventions are still in *Development* status and keep changing on `main` in the dedicated repository `open-telemetry/semantic-conventions-genai`. Do not track `main` continuously.
- A mandatory two-layer schema:
  1. **Generic `gen_ai.*` attributes** (common to every provider and framework).
  2. **Provider-specific attributes** (e.g. the Bedrock extensions where `gen_ai.provider.name = "aws.bedrock"`), stored separately (a table or a JSON column) so as not to force a normalisation that would lose the specific information.
- Scope of the model for the MVP: cover only 3–4 essential event types at first: model call, tool call, agent run. The rest (retrieval, memory, etc.) waits for the next increment.
- A mapping/adapter layer must separate "what arrives over OTLP" from "what the kernel stores internally", to contain the impact of upstream OTel schema changes.
- Do not capture prompt and response content by default (the OTel GenAI PII principle); enable it explicitly if needed, with the compliance implications that come with it.

### 2.2 Pipeline (kernel MVP)

Build order, each step testable on its own:

1. **Pinned data model** (see 2.1).
2. **Minimal OTLP ingestion**: a gRPC/HTTP receiver in Rust (`tonic`) that accepts, validates and persists raw data. No advanced batching or smart sampling at this stage.
3. **Single-instance storage**: one backend to start with (self-hosted ClickHouse, or managed BigQuery on GCP / Azure Data Explorer on Azure). No multi-tenancy or fine-grained retention in the MVP.
4. **Minimal query API**: 2-3 endpoints: list recent traces, fetch a trace's tree, aggregate a few basic metrics. No rich dashboard needed to validate the kernel.
5. **Plugin contract v0**: a Rust trait (or a WASM interface via `wasmtime`) that business plugins will implement. Write a dummy plugin to validate the contract before anticipating the needs of verticals not met yet.
6. **Skeleton deployment on a single cloud**: one region, no high availability. Goal: a working CI/CD pipeline, not robust production infrastructure.
7. **Validation loop**: run the kernel against a real telemetry flow (see section 3, the fraudos case).

### 2.3 Agentic analysis layer

- The analysis agents (RCA, anomaly detection) generate LLM traces themselves while running: plan from the start for this layer to instrument itself through the same pipeline (meta-observability), so as not to be blind to one's own diagnosis system.
- This layer is **not** in the kernel MVP's scope: it comes after the base pipeline is validated.

### 2.4 Plugin architecture

- A core of Rust traits; WASM modules (`wasmtime`) considered to isolate client- or vertical-specific plugins and allow dynamic loading without recompiling the core.
- Do not over-design the plugin contract up front: let the first real vertical inform it (fintech/fraud, see section 3).

---

## 3. First vertical: fintech (fraudos)

- An existing prototype (**fraudos**) already runs on **AWS Bedrock**, instrumented with botocore + ADOT (AWS Distro for OpenTelemetry).
- **Key point**: Bedrock through ADOT already emits the OTel GenAI conventions natively (`gen_ai.*`, with Bedrock-specific extensions). There is **no need to build a CloudWatch → OTLP adapter**: CloudWatch is only one possible destination of this instrumentation, not the only path.
- **Recommended integration**: point ADOT's OTLP exporter at the kernel's ingestion endpoint (an environment variable on the ADOT side), alongside CloudWatch during the transition if a comparison is needed.
- **Fraud-specific attributes to plan for in the fintech plugin**:
  - A transaction identifier, to correlate the model's decision with the real outcome (confirmed fraud or not). The outcome often arrives after inference, so plan a way to update an existing trace after the fact.
  - Risk score and decision threshold as first-class attributes, not buried in the model's output blob.
  - Retention and immutability constraints for the regulatory auditability of automated decisions. This potentially affects the choice of storage, not only the plugin.
- The fraudos case is **the concrete validation case for step 7 of the kernel** (section 2.2): plug it in directly, with no intermediate layer to build.

---

## 4. What the kernel MVP must NOT cover (deliberately out of scope)

- Multi-tenancy.
- High availability / multi-region.
- Exhaustive coverage of the OTel GenAI conventions (retrieval, memory, etc.).
- The agentic analysis layer (RCA, anomalies).
- A rich dashboard.
- Several clouds at once.

---

## 5. Open questions to settle with the agent / along the way

- Single-tenant to validate one vertical, or a multi-tenant architecture designed in from the start of storage and plugin isolation? (Answered: [ADR 0001](docs/adr/0001-multi-tenant-out-of-mvp-scope.md).)
- GCP or Azure for production (still open: GKE has a more mature Kubernetes ecosystem for this kind of load, AKS integrates better if target clients already live in the Microsoft/365 ecosystem; Pub/Sub vs Event Hubs to decouple ingestion from processing). (Reframed: [ADR 0002](docs/adr/0002-hosting-model.md).)
- Self-hosted ClickHouse or managed storage (BigQuery/ADX), to decide by expected volume (reference: below ~100k requests a day, managed nearly always wins on total cost; above ~1M a day, self-hosting becomes attractive if ops bandwidth follows). (Answered: [ADR 0003](docs/adr/0003-self-hosted-clickhouse.md).)
- The exact plugin loading mode: statically compiled Rust trait or dynamically loaded WASM modules. (Answered: [ADR 0004](docs/adr/0004-native-plugin-loading.md).)

---

## 6. Technical pointers (Rust)

- OTLP ingestion: `tonic` (gRPC) + `prost`.
- Isolated plugins: `wasmtime`.
- Storage: a Rust ClickHouse driver, or the matching cloud SDK (BigQuery/ADX) depending on the trade-off.
- The existing OpenTelemetry Rust SDK can serve as a reference implementation for OTLP protocol conformance, even if the kernel does not reuse it as is.

---

*This dossier reflects the decisions made before the scoping conversation. Every decision listed in section 5 must be settled before or during the construction of the matching kernel step.*

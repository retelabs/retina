# semconv-genai: the `gen_ai.*` semantic conventions kept for the MVP

- Authoritative source: https://github.com/open-telemetry/semantic-conventions-genai
  (`model/gen-ai/spans.yaml`, `model/gen-ai/registry.yaml`, `model/aws-bedrock/registry.yaml`)
- Pinned version/commit: `30182acd5ed78ab5f619041eaec5e95a4eb83a48` (branch `main`;
  **no tag published** in that repository at pin time; overall status of the
  repository: *Development*, no *Stable* span or attribute)
- Verification date: 2026-08-14
- Pinned with: `scripts/pin-semconv.sh 30182acd5ed78ab5f619041eaec5e95a4eb83a48`
  → `vendor/semconv-genai/`

## The 3 spans kept for the MVP (dossier section 2.1: "model call, tool call, agent run")

### 1. Model call → span `gen_ai.inference.client`

`kind: client`. Attributes kept for the internal model:

| Attribute | Spec type | Level |
|---|---|---|
| `gen_ai.provider.name` | closed enum (`members`): `openai`, `anthropic`, `aws.bedrock`, `gcp.vertex_ai`, `gcp.gemini`, `gcp.gen_ai`, `azure.ai.inference`, `azure.ai.openai`, `cohere`, `ibm.watsonx.ai`, `perplexity`, `x_ai`, `deepseek`, `groq`, `mistral_ai`, `moonshot_ai` | **required**, sampling_relevant |
| `gen_ai.operation.name` | closed enum (17 values, see below) | **required** |
| `gen_ai.request.model` | string | conditionally_required (if available) |
| `gen_ai.response.model` | string | recommended |
| `server.address` / `server.port` | string / int | recommended / conditionally_required |
| `gen_ai.usage.input_tokens` | **int** | recommended |
| `gen_ai.usage.output_tokens` | **int** | recommended |
| `gen_ai.usage.cache_read.input_tokens` | int | recommended, already included in `input_tokens` |
| `gen_ai.usage.cache_creation.input_tokens` | int | recommended, already included in `input_tokens` |
| `gen_ai.response.finish_reasons` | **string[]** | recommended |
| `gen_ai.conversation.id` | string | conditionally_required |
| `error.type` | (defined outside this repository, see "Uncertainties") | conditionally_required on error |

An important note found in the spec: token counting **differs by provider**. For
example Anthropic excludes cache tokens from `input_tokens` (they must be added
back), while OpenAI/Azure already include them. The mapping/adapter therefore
cannot treat `gen_ai.usage.input_tokens` as a universal raw value without knowing
`gen_ai.provider.name`.

### 2. Tool call → span `gen_ai.execute_tool.internal`

`kind: internal`.

| Attribute | Spec type | Level |
|---|---|---|
| `gen_ai.operation.name` | closed enum, value `execute_tool` | **required** |
| `gen_ai.tool.name` | string | **required** |
| `gen_ai.tool.call.id` | string | recommended if available |
| `gen_ai.tool.type` | free string (examples: `function`, `extension`, `datastore`; **not** a closed enum, unlike `operation.name`/`provider.name`) | recommended |
| `gen_ai.tool.description` | string | recommended if available |
| `gen_ai.agent.name` | string | conditionally_required if applicable |
| `gen_ai.tool.call.arguments` / `gen_ai.tool.call.result` | **`any`** (a referenced JSON schema, free object) | **opt_in**, marked "may contain sensitive information" in the spec |
| `error.type` | — | conditionally_required on error |

### 3. Agent run → spans `gen_ai.invoke_agent.client` (remote agent) and `gen_ai.invoke_agent.internal` (in-process agent)

The key difference between the two variants, not to be missed:
`gen_ai.provider.name` is **required on the `client` variant** (e.g. AWS Bedrock
Agents, OpenAI Assistants) but **absent from the `internal` variant's attribute
list** (e.g. LangChain, CrewAI): an in-process agent has no "provider" in the
transport sense.

| Attribute | Spec type | Level |
|---|---|---|
| `gen_ai.operation.name` | closed enum, value `invoke_agent` | **required** |
| `gen_ai.agent.name` | string | conditionally_required if available |
| `gen_ai.agent.id` | string (a stable identifier from the provider, e.g. a Bedrock ARN) | conditionally_required, mostly the client variant |
| `gen_ai.agent.description` | string | conditionally_required |
| `gen_ai.agent.version` | string | conditionally_required, client variant |
| `gen_ai.request.model` | string | recommended, **only if the agent has a single fixed model** (do not set it with dynamic selection) |
| `gen_ai.provider.name` | closed enum | **required (client variant only)** |
| `gen_ai.usage.input_tokens` / `output_tokens` / caches | int | recommended |
| `gen_ai.conversation.id` | string | conditionally_required |
| `error.type` | — | conditionally_required on error |

## Two enums, closed but evolving: what it means for Rust typing

`gen_ai.operation.name` and `gen_ai.provider.name` are defined with a closed
`members` list in the YAML, but the repository is in *Development* status and
new values (new providers, new operations) are added regularly without a stable
version bump. **Recommendation for the internal model**: do not encode these
values as a strict Rust `enum` that would fail to deserialise an unknown value.
Use a "known enum plus an `Other(String)` variant" type, to tolerate upstream
changes without losing data; revisit if and when the spec reaches *Stable*.

## Deliberately ignored in the MVP (and why)

- **Out-of-scope spans**: `gen_ai.embeddings.client`, `gen_ai.retrieval.client`,
  `gen_ai.fetch_response.client`, `gen_ai.memory.client`,
  `gen_ai.create_agent.client`, `gen_ai.invoke_workflow.internal`,
  `gen_ai.plan.internal`. Retrieval, memory and workflows are explicitly outside
  the MVP (dossier section 4).
- **Every content attribute** (`gen_ai.input.messages`, `gen_ai.output.messages`,
  `gen_ai.system_instructions`, `gen_ai.tool.definitions`,
  `gen_ai.tool.call.arguments`, `gen_ai.tool.call.result`,
  `gen_ai.retrieval.query.text`): already marked `opt_in` by the spec itself, and
  disabled by default anyway by our own PII policy (dossier section 2.1).
- **Fine-grained request parameters** (`top_k`, `top_p`, `frequency_penalty`,
  `presence_penalty`, `seed`, `stop_sequences`, `reasoning.level`, `output.type`,
  `stream`, `prompt.name/version/variable`): recommended but not central to the
  MVP's two-layer generic schema. If they arrive they land in the bag of generic
  attributes not promoted to first-class fields, not in the provider extensions
  (they are not provider-specific).
- **Non-fintech Bedrock extensions** (`aws.bedrock.guardrail.id`,
  `aws.bedrock.knowledge_base.id`, defined in `model/aws-bedrock/registry.yaml`,
  only 2 attributes in that repository): stored in the provider-specific
  attribute layer (dossier section 2.1) but not promoted to first-class kernel
  fields. Conversely, `transaction_id` / `risk_score` / decision threshold
  (section 3) do NOT come from this spec: they are the fintech plugin's own
  attributes, defined in the plugin contract v0 (step 5), not here.

## Remaining uncertainties / to re-validate before production

- ~~The exact width of the `int` type~~ **Resolved**:
  `vendor/opentelemetry-proto@v1.11.0`,
  `opentelemetry/proto/common/v1/common.proto` confirms `int64 int_value = 3;` in
  the `AnyValue` `oneof`. So `gen_ai.usage.input_tokens`/`output_tokens`/etc. are
  **signed `i64`** on the OTLP wire: not `i32`, not `u64`. For the internal
  model: either keep `i64`, strictly consistent with the wire (and reject or log
  negative values as an anomaly rather than casting them to `u64`), or convert to
  `u64` **after** an explicit `>= 0` check. Never a bare `as u64` on a received
  value, which would silently wrap a malformed negative `int_value` into a huge
  positive number.
- **`error.type`** is NOT defined in `semantic-conventions-genai`: it is a
  reference (`ref: error.type`) to the generic attribute of the base repository
  `open-telemetry/semantic-conventions`. Pinning that second repository is not
  needed for the MVP: the local note ("SHOULD match... or another low-cardinality
  error identifier") is enough to treat it as a free low-cardinality `String` in
  the kernel. If its precise definition is ever needed (format constraints,
  examples), pin that second repository separately.
- **Repository without tags**: the current pin is a `main` commit, not a tag. It
  needs closer watching than a pin on a tag, and a re-check
  (`scripts/check-pins.sh` plus a fresh read) before any version change, since
  there is no formal release changelog to follow between two pins.

# cost-calculation: the $ cost per span (`crates/pricing`)

- Authoritative sources:
  - behaviour of the `gen_ai.usage.*` fields per provider:
    `docs/interfaces/semconv-genai.md` (already verified at kernel step 1)
  - OpenAI prices: https://developers.openai.com/api/docs/pricing
    (`platform.openai.com/docs/pricing` redirects there), fetched on 2026-08-17
  - Anthropic prices: https://platform.claude.com/docs/en/about-claude/pricing,
    fetched on 2026-08-17
  - Anthropic models `claude-sonnet-5`/`claude-opus-5`/`claude-fable-5`/
    `claude-haiku-4-5-20251001`: exact identifiers taken from the system context
    of the development session (itself running on a Claude model)
- Verification date: 2026-08-17
- Scope: `crates/pricing`, called once at ingestion from
  `crates/clickhouse-sink/src/row.rs`, stored in `spans.cost_usd` (migration
  `0003_add_cost_usd.sql`).

## Scope decisions, made with the owner

Three questions asked before coding (discussion of 2026-08-17):

1. **Where does the price table live?** Static, versioned in this repository
   (`crates/pricing/src/table.rs`), not supplied by the client at ingestion.
   Consistent with ADR-0001 (single-tenant): one real client today, no
   demonstrated need for per-tenant prices.
2. **Computed at ingestion or at query time?** At ingestion. The cost is frozen
   at the price in force when the row is inserted.
3. **Price changes over time?** A price change is a commit to `table.rs`, never a
   retroactive recomputation: spans already ingested keep the cost computed with
   the price actually in force at the time. The accepted downside: a span
   ingested before a model was added to the table stays `cost_usd = NULL`
   forever, with no automatic back-fill.

## A structural finding: cache-token accounting differs by provider

`docs/interfaces/semconv-genai.md` (documented before this work) noted: *"token
counting differs by provider: e.g. Anthropic excludes cache tokens from
`input_tokens` (they must be added back), OpenAI/Azure already include them."*
A single universal cost formula would have been wrong for one of the two
providers. This was checked before writing `estimate_cost_usd`, not discovered
later by comparing with a real invoice.

`CacheAccounting` (`crates/pricing/src/lib.rs`) encodes the difference:

- **`IncludedInInput`** (OpenAI, Azure OpenAI): `input_tokens` already contains
  the cache-read tokens. Cost = `(input_tokens - cache_read_tokens)` at the
  normal rate + `cache_read_tokens` at the cache rate + `output_tokens`.
- **`AdditionalToInput`** (Anthropic): `input_tokens` excludes cache tokens.
  Cost = `input_tokens` at the normal rate + `cache_read_tokens` +
  `cache_creation_tokens` (writes), each at its own rate, + `output_tokens`.

A documented approximation, not a silent guess: Anthropic bills cache writes
differently depending on their TTL (5 minutes vs 1 hour), but
`ModelCallEvent`/`AgentRunEvent` carry no field to tell which one was used.
`crates/pricing` always uses the 5-minute rate (Anthropic's default prompt-cache
behaviour).

## Which model identifier is used for the price lookup

`response_model` first, `request_model` as a fallback (`ModelCallEvent` has both;
`AgentRunEvent` only has `request_model`): the model that actually served the
request is the one whose price applies. The lookup is an **exact string match**,
no prefix or fuzzy matching: a wrong match would silently price a span at the
wrong rate, worse than an unpriced span.

## Known gap, not yet closed

**No real telemetry existed to confirm the exact format of the
`request_model`/`response_model` strings** sent in practice: the first client set
neither field at the time (the finding that started this work). An alias
(`"gpt-4o"`) and a dated snapshot (`"gpt-4o-2024-08-06"`) are two different
strings for `crates/pricing`'s exact lookup; which one a real SDK sends has not
been checked against a real API response. Check it as soon as a client sets these
fields for real, before trusting the table's coverage beyond its per-token
numbers.

## Providers deliberately left unpriced

Groq (asked for explicitly by the owner): the official page (`groq.com/pricing`)
returned no usable price table, and `console.groq.com/docs/pricing` returned
`404`, checked on 2026-08-17. Only third-party aggregators had figures, set aside
for the same reason as the Hetzner discrepancy already met in `docs/cost-model.md`
(unreliable, contradictory figures). AWS Bedrock, IBM watsonx, GCP
Vertex/Gemini, Azure AI, Cohere, Perplexity, xAI, DeepSeek, Mistral, Moonshot:
unpriced too; none was asked for explicitly and none was verified in that
session. A span from one of these providers stays `cost_usd = NULL`: not an
error, just "not priced yet". To add a provider: check its real official price
page before adding an entry to `table.rs`, never from memory (the project's
standing rule, CLAUDE.md).

## `spans.cost_usd`: aggregation behaviour, verified

`cost_usd` is `Nullable(Float64)`. Verified against a real ClickHouse
(2026-08-17, not assumed): `sum(cost_usd)` returns `NULL` both on an empty group
and on a group where every value is `NULL`, unlike `max()` on a non-nullable
column, already documented in `docs/interfaces/clickhouse-retention.md` (`0`, not
`NULL`, on an empty table). `query-api::MetricsSummaryDto.by_kind[].total_cost_usd`
is therefore `Option<f64>`, deliberately **not** turned into `0.0` the way
`total_input_tokens`/`total_output_tokens` are: a `0.0` total would suggest a real
cost of zero rather than "no priced span in this group", two different situations
the aggregate has to tell apart.

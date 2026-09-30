//! Static per-model price table. Versioned in this repo rather than fetched
//! at runtime (docs/interfaces/cost-calculation.md: "where does the price
//! table live?": static, not supplied by the client) — a price change means a
//! commit here, not a runtime config change, so historical `cost_usd`
//! values already written stay correct for the price that was actually in
//! effect when the span was ingested (`crates/pricing` is only ever called
//! at ingestion, see `crates/clickhouse-sink/src/row.rs`).
//!
//! Every entry cites where its numbers came from and when — same discipline
//! as `crates/cost-model/src/pricing.rs` for VM/storage prices. Entries not
//! independently re-verified this session (older/retired Claude models,
//! known from general model-release knowledge rather than a freshly fetched
//! page) are marked explicitly; don't extend this table by guessing a new
//! entry the same way — fetch the real page first (project's standing
//! rule, CLAUDE.md).

use crate::{CacheAccounting, ModelPricing};

/// (provider, exact model id as it would appear in `gen_ai.request.model` /
/// `gen_ai.response.model`, pricing). Matching is exact-string — no prefix
/// or fuzzy matching, since a wrong match would silently misprice a span.
///
/// **Known gap, not yet closed**: no real client/OpenAI/Anthropic telemetry
/// existed to confirm which exact string (alias like `"gpt-4o"` vs a
/// dated snapshot like `"gpt-4o-2024-08-06"`) actually lands in
/// `request_model`/`response_model` in practice — the first client doesn't populate
/// these fields at all yet (see the conversation that led to this crate).
/// Verify against real captured spans before trusting this table's
/// coverage, not just its per-token numbers.
pub const PRICE_TABLE: &[(&str, &str, ModelPricing)] = &[
    // --- OpenAI ---
    // Source: https://developers.openai.com/api/docs/pricing (platform.openai.com/docs/pricing
    // redirects there), fetched 2026-08-17. `input_tokens` already includes
    // cache-read tokens for OpenAI (docs/interfaces/semconv-genai.md §33),
    // hence `CacheAccounting::IncludedInInput`. OpenAI has no separate
    // cache-write charge — `cache_write_per_million_usd: None`.
    (
        "openai",
        "gpt-4o",
        ModelPricing {
            input_per_million_usd: 2.50,
            output_per_million_usd: 10.00,
            cache_read_per_million_usd: Some(1.25),
            cache_write_per_million_usd: None,
            accounting: CacheAccounting::IncludedInInput,
        },
    ),
    (
        "openai",
        "gpt-4o-mini",
        ModelPricing {
            input_per_million_usd: 0.15,
            output_per_million_usd: 0.60,
            cache_read_per_million_usd: Some(0.075),
            cache_write_per_million_usd: None,
            accounting: CacheAccounting::IncludedInInput,
        },
    ),
    (
        "openai",
        "gpt-4.1",
        ModelPricing {
            input_per_million_usd: 2.00,
            output_per_million_usd: 8.00,
            cache_read_per_million_usd: Some(0.50),
            cache_write_per_million_usd: None,
            accounting: CacheAccounting::IncludedInInput,
        },
    ),
    (
        "openai",
        "gpt-4.1-mini",
        ModelPricing {
            input_per_million_usd: 0.40,
            output_per_million_usd: 1.60,
            cache_read_per_million_usd: Some(0.10),
            cache_write_per_million_usd: None,
            accounting: CacheAccounting::IncludedInInput,
        },
    ),
    (
        "openai",
        "gpt-4.1-nano",
        ModelPricing {
            input_per_million_usd: 0.10,
            output_per_million_usd: 0.40,
            cache_read_per_million_usd: Some(0.025),
            cache_write_per_million_usd: None,
            accounting: CacheAccounting::IncludedInInput,
        },
    ),
    (
        "openai",
        "o1",
        ModelPricing {
            input_per_million_usd: 15.00,
            output_per_million_usd: 60.00,
            cache_read_per_million_usd: Some(7.50),
            cache_write_per_million_usd: None,
            accounting: CacheAccounting::IncludedInInput,
        },
    ),
    (
        "openai",
        "o3",
        ModelPricing {
            input_per_million_usd: 2.00,
            output_per_million_usd: 8.00,
            cache_read_per_million_usd: Some(0.50),
            cache_write_per_million_usd: None,
            accounting: CacheAccounting::IncludedInInput,
        },
    ),
    (
        "openai",
        "o3-mini",
        ModelPricing {
            input_per_million_usd: 1.10,
            output_per_million_usd: 4.40,
            cache_read_per_million_usd: Some(0.55),
            cache_write_per_million_usd: None,
            accounting: CacheAccounting::IncludedInInput,
        },
    ),
    (
        "openai",
        "gpt-5",
        ModelPricing {
            input_per_million_usd: 1.25,
            output_per_million_usd: 10.00,
            cache_read_per_million_usd: Some(0.125),
            cache_write_per_million_usd: None,
            accounting: CacheAccounting::IncludedInInput,
        },
    ),
    // --- Anthropic ---
    // Source: https://platform.claude.com/docs/en/about-claude/pricing,
    // fetched 2026-08-17. `input_tokens` EXCLUDES cache tokens for Anthropic
    // (docs/interfaces/semconv-genai.md §33) — cache reads/writes are billed
    // *on top of* input_tokens, hence `CacheAccounting::AdditionalToInput`.
    // Cache-write price uses the 5-minute-TTL rate (the default cache
    // behavior); a 1-hour-TTL write costs more but `AgentRunEvent`/
    // `ModelCallEvent` carry no field to distinguish which TTL was used —
    // documented approximation, not a silent guess.
    //
    // Model id "claude-sonnet-5" and "claude-haiku-4-5-20251001" are the
    // exact API model ids from this session's own system context (Claude
    // model identifiers), not fetched — as authoritative as it gets since
    // this assistant *is* a Claude model. "claude-opus-5"/"claude-fable-5"
    // likewise.
    (
        "anthropic",
        "claude-fable-5",
        ModelPricing {
            input_per_million_usd: 10.00,
            output_per_million_usd: 50.00,
            cache_read_per_million_usd: Some(1.00),
            cache_write_per_million_usd: Some(12.50),
            accounting: CacheAccounting::AdditionalToInput,
        },
    ),
    (
        "anthropic",
        "claude-opus-5",
        ModelPricing {
            input_per_million_usd: 5.00,
            output_per_million_usd: 25.00,
            cache_read_per_million_usd: Some(0.50),
            cache_write_per_million_usd: Some(6.25),
            accounting: CacheAccounting::AdditionalToInput,
        },
    ),
    (
        "anthropic",
        "claude-sonnet-5",
        ModelPricing {
            input_per_million_usd: 2.00,
            output_per_million_usd: 10.00,
            cache_read_per_million_usd: Some(0.20),
            cache_write_per_million_usd: Some(2.50),
            accounting: CacheAccounting::AdditionalToInput,
        },
    ),
    (
        "anthropic",
        "claude-haiku-4-5-20251001",
        ModelPricing {
            input_per_million_usd: 1.00,
            output_per_million_usd: 5.00,
            cache_read_per_million_usd: Some(0.10),
            cache_write_per_million_usd: Some(1.25),
            accounting: CacheAccounting::AdditionalToInput,
        },
    ),
    // Not re-verified this session — well-known public model ids from
    // Anthropic's release history (stable, unlikely to have changed), kept
    // because the first client's actual deployment may well predate the Claude 5
    // family. Spot-check against a real Anthropic API response before
    // relying on the id string matching, per the gap noted above the table.
    (
        "anthropic",
        "claude-sonnet-4-5-20250929",
        ModelPricing {
            input_per_million_usd: 3.00,
            output_per_million_usd: 15.00,
            cache_read_per_million_usd: Some(0.30),
            cache_write_per_million_usd: Some(3.75),
            accounting: CacheAccounting::AdditionalToInput,
        },
    ),
    (
        "anthropic",
        "claude-opus-4-5-20251101",
        ModelPricing {
            input_per_million_usd: 5.00,
            output_per_million_usd: 25.00,
            cache_read_per_million_usd: Some(0.50),
            cache_write_per_million_usd: Some(6.25),
            accounting: CacheAccounting::AdditionalToInput,
        },
    ),
    (
        "anthropic",
        "claude-3-5-haiku-20241022",
        ModelPricing {
            input_per_million_usd: 0.80,
            output_per_million_usd: 4.00,
            cache_read_per_million_usd: Some(0.08),
            cache_write_per_million_usd: Some(1.00),
            accounting: CacheAccounting::AdditionalToInput,
        },
    ),
    // --- Not yet priced, deliberately absent rather than guessed ---
    // Groq: official pricing page (groq.com/pricing, console.groq.com/docs/pricing)
    // returned no extractable price table and a 404 respectively when
    // checked 2026-08-17; only third-party aggregator numbers were found,
    // which this project's own precedent (docs/cost-model.md, Hetzner
    // aggregator discrepancy) treats as untrustworthy. Add Groq once the
    // real console pricing page is captured directly, not from a blog.
];

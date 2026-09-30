//! Estimates a span's LLM API cost in USD from its token counts, using a
//! static versioned price table (`table.rs`) — scoping decided with the
//! user: computed once, at ingestion (called from
//! `crates/clickhouse-sink/src/row.rs`, stored as `spans.cost_usd`), not
//! recomputed at query time. A price change is a new commit to this crate;
//! it never rewrites the `cost_usd` already stored for spans ingested under
//! the old price — that's the point (docs/interfaces/cost-calculation.md).
//!
//! Unknown provider/model or missing token counts yield `None`, not an
//! error or a zero — same "optional, don't fail the span" posture as the
//! token fields themselves (docs/interfaces/otlp-ingestion.md).

mod table;

use kernel_model::ProviderName;

/// How cache tokens relate to `input_tokens`, verified against
/// docs/interfaces/semconv-genai.md (§33: "token counting differs
/// par provider") — a real per-provider divergence, not a detail this crate
/// invented. Getting this wrong doesn't just shift a number, it silently
/// double-counts or drops cache tokens entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheAccounting {
    /// `input_tokens` already includes cache-read tokens (OpenAI, Azure
    /// OpenAI). Billable regular input is `input_tokens - cache_read_tokens`.
    IncludedInInput,
    /// `input_tokens` excludes cache tokens entirely (Anthropic). Cache
    /// reads/writes are billed *in addition to* `input_tokens`.
    AdditionalToInput,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    pub input_per_million_usd: f64,
    pub output_per_million_usd: f64,
    /// `None` means "no distinct cache-read rate published" — falls back to
    /// the regular input rate rather than being treated as free.
    pub cache_read_per_million_usd: Option<f64>,
    /// `None` means "no distinct cache-write rate" (e.g. OpenAI, which
    /// doesn't bill cache writes separately at all).
    pub cache_write_per_million_usd: Option<f64>,
    pub accounting: CacheAccounting,
}

/// Exact-string lookup — see the caveat in `table.rs` about why this isn't
/// fuzzy/prefix matching.
pub fn find_pricing(provider: &ProviderName, model: &str) -> Option<&'static ModelPricing> {
    table::PRICE_TABLE
        .iter()
        .find(|(p, m, _)| *p == provider.as_str() && *m == model)
        .map(|(_, _, pricing)| pricing)
}

fn cost_of(tokens: u64, price_per_million_usd: f64) -> f64 {
    (tokens as f64) * price_per_million_usd / 1_000_000.0
}

/// `model` should be the most specific model identifier available — callers
/// pass `response_model` if present, falling back to `request_model`
/// (row.rs), since the response model is the one that actually served the
/// request and therefore the one whose price applies.
///
/// Returns `None` if the provider/model combination isn't in the price
/// table, if `model` is `None`, or if the input or output token count is
/// missing: a span that reports no usage, or only half of it, is "unpriced",
/// not free and not undercounted. Never a wrong number, only "unpriced".
/// Cache counts may be absent (most providers omit them when unused) and
/// count as zero.
pub fn estimate_cost_usd(
    provider: &ProviderName,
    model: Option<&str>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    cache_creation_tokens: Option<u64>,
) -> Option<f64> {
    let pricing = find_pricing(provider, model?)?;

    let input = input_tokens?;
    let output = output_tokens?;
    let cache_read = cache_read_tokens.unwrap_or(0);
    let cache_write = cache_creation_tokens.unwrap_or(0);

    let cache_read_price = pricing
        .cache_read_per_million_usd
        .unwrap_or(pricing.input_per_million_usd);

    let cost = match pricing.accounting {
        CacheAccounting::IncludedInInput => {
            // cache_read is a subset of input already — don't double-bill it
            // at the full input rate.
            let regular_input = input.saturating_sub(cache_read);
            cost_of(regular_input, pricing.input_per_million_usd)
                + cost_of(cache_read, cache_read_price)
                + cost_of(output, pricing.output_per_million_usd)
        }
        CacheAccounting::AdditionalToInput => {
            let cache_write_price = pricing
                .cache_write_per_million_usd
                .unwrap_or(pricing.input_per_million_usd);
            cost_of(input, pricing.input_per_million_usd)
                + cost_of(cache_read, cache_read_price)
                + cost_of(cache_write, cache_write_price)
                + cost_of(output, pricing.output_per_million_usd)
        }
    };

    Some(cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_model_yields_none_not_zero() {
        assert_eq!(
            estimate_cost_usd(
                &ProviderName::OpenAi,
                Some("not-a-real-model"),
                Some(100),
                Some(50),
                None,
                None
            ),
            None
        );
    }

    #[test]
    fn a_known_model_without_token_counts_yields_none_not_zero() {
        // Seen for real on 2026-09-30: an oncology replay's gpt-4o call
        // carries no usage at all and was stored with cost_usd = 0.
        assert_eq!(
            estimate_cost_usd(
                &ProviderName::OpenAi,
                Some("gpt-4o"),
                None,
                None,
                None,
                None
            ),
            None
        );
    }

    #[test]
    fn a_partial_usage_report_yields_none_rather_than_an_undercount() {
        for (input, output) in [(Some(1_000), None), (None, Some(1_000))] {
            assert_eq!(
                estimate_cost_usd(
                    &ProviderName::OpenAi,
                    Some("gpt-4o"),
                    input,
                    output,
                    None,
                    None
                ),
                None,
                "input {input:?}, output {output:?}"
            );
        }
    }

    #[test]
    fn zero_tokens_reported_is_a_real_zero_cost() {
        assert_eq!(
            estimate_cost_usd(
                &ProviderName::OpenAi,
                Some("gpt-4o"),
                Some(0),
                Some(0),
                None,
                None
            ),
            Some(0.0)
        );
    }

    #[test]
    fn no_model_yields_none() {
        assert_eq!(
            estimate_cost_usd(&ProviderName::OpenAi, None, Some(100), Some(50), None, None),
            None
        );
    }

    #[test]
    fn openai_prices_input_and_output_at_published_rates() {
        // gpt-4o-mini: $0.15/$0.60 per million, no cache tokens involved.
        let cost = estimate_cost_usd(
            &ProviderName::OpenAi,
            Some("gpt-4o-mini"),
            Some(1_000_000),
            Some(1_000_000),
            None,
            None,
        )
        .unwrap();
        assert!((cost - 0.75).abs() < 1e-9);
    }

    #[test]
    fn openai_cache_read_tokens_are_not_double_billed() {
        // gpt-4o-mini: input_tokens already includes the 600 cache-read
        // tokens. Expected: 400 regular @ $0.15/M + 600 cached @ $0.075/M.
        let cost = estimate_cost_usd(
            &ProviderName::OpenAi,
            Some("gpt-4o-mini"),
            Some(1_000),
            Some(0),
            Some(600),
            None,
        )
        .unwrap();
        let expected = cost_of(400, 0.15) + cost_of(600, 0.075);
        assert!((cost - expected).abs() < 1e-12);
    }

    #[test]
    fn anthropic_cache_tokens_are_billed_on_top_of_input() {
        // claude-sonnet-5: input_tokens excludes cache tokens entirely.
        let cost = estimate_cost_usd(
            &ProviderName::Anthropic,
            Some("claude-sonnet-5"),
            Some(1_000),
            Some(0),
            Some(600),
            Some(200),
        )
        .unwrap();
        let expected = cost_of(1_000, 2.00) + cost_of(600, 0.20) + cost_of(200, 2.50);
        assert!((cost - expected).abs() < 1e-12);
    }

    #[test]
    fn every_table_entry_is_reachable_by_provider_as_str() {
        // Catches a typo'd provider string in table.rs that `as_str()`
        // would never actually produce — the entry would silently never
        // match anything.
        let known_providers = [
            ProviderName::OpenAi,
            ProviderName::Anthropic,
            ProviderName::GcpGenAi,
            ProviderName::GcpVertexAi,
            ProviderName::GcpGemini,
            ProviderName::Cohere,
            ProviderName::AzureAiInference,
            ProviderName::AzureAiOpenAi,
            ProviderName::IbmWatsonxAi,
            ProviderName::AwsBedrock,
            ProviderName::Perplexity,
            ProviderName::XAi,
            ProviderName::DeepSeek,
            ProviderName::Groq,
            ProviderName::MistralAi,
            ProviderName::MoonshotAi,
        ];
        let known_strs: Vec<&str> = known_providers.iter().map(ProviderName::as_str).collect();
        for (provider_str, model, _) in table::PRICE_TABLE {
            assert!(
                known_strs.contains(provider_str),
                "table.rs entry for model `{model}` uses provider string `{provider_str}` \
                 which doesn't match any ProviderName::as_str() value"
            );
        }
    }
}

-- $ cost per span (docs/interfaces/cost-calculation.md), computed once at
-- ingestion (crates/pricing, called from src/row.rs) and never recomputed
-- afterwards: a price change must not rewrite the cost already stored for
-- older spans.
--
-- Nullable, like the other fields derived from gen_ai.usage.*: NULL means
-- "not priced" (model or provider missing from the price table, or an event
-- type without tokens such as tool_call), never a wrong cost.
ALTER TABLE spans
    ADD COLUMN cost_usd Nullable(Float64);

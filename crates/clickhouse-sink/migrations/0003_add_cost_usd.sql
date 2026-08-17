-- Coût $ par span (docs/interfaces/cost-calculation.md) — calculé une seule
-- fois à l'ingestion (crates/pricing, appelé depuis src/row.rs), jamais
-- recalculé après coup : un changement de tarif ne doit pas réécrire le
-- coût déjà stocké pour des spans plus anciens.
--
-- Nullable, comme les autres champs dérivés de gen_ai.usage.* : NULL veut
-- dire "non tarifé" (modèle/fournisseur absent de la table de prix, ou type
-- d'événement sans tokens comme tool_call), jamais un coût erroné.
ALTER TABLE spans
    ADD COLUMN cost_usd Nullable(Float64);

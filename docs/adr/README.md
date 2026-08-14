# Architecture Decision Records

Une ADR par décision structurante, notamment les questions ouvertes listées
en section 5 de
[dossier-observabilite-agentique.md](../../dossier-observabilite-agentique.md) :

- Mono-tenant pour valider le premier vertical, ou conception multi-tenant
  dès le stockage/l'isolation des plugins ?
- GCP vs Azure pour la prod (GKE/Pub-Sub vs AKS/Event Hubs).
- ClickHouse auto-hébergé vs stockage managé (BigQuery/ADX) — repère :
  managé gagnant sous ~100k req/jour, auto-hébergé intéressant au-dessus de
  ~1M/jour si la bande passante ops suit.
- Chargement de plugins : trait Rust compilé statiquement vs modules WASM
  chargés dynamiquement.

Utilise `/adr <titre>` pour créer une nouvelle entrée à partir de
[0000-template.md](0000-template.md). Numérote séquentiellement
(`0001-...`, `0002-...`). Les décisions à impact produit/coût significatif
(cloud cible, multi-tenant) se présentent avec leurs trade-offs et attendent
confirmation avant d'être marquées "Accepté".

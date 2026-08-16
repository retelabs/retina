# trellis

Kernel Rust d'observabilité (inspiré OTel/Datadog) pour workflows d'agents
LLM, avec plugins métiers interprétant des invariants de gouvernance réels
(fraude, conformité HIPAA/GDPR). Ingestion OTLP/gRPC standard — un vrai SDK
OpenTelemetry suffit côté client, zéro code custom.

- **Utiliser trellis depuis votre application** → [`docs/client-integration.md`](docs/client-integration.md)
- **Décisions produit et architecture** → [`dossier-observabilite-agentique.md`](dossier-observabilite-agentique.md)
- **Contrats techniques vérifiés** (OTLP, ClickHouse, auth, plugins...) → [`docs/interfaces/`](docs/interfaces/)
- **Déployer une instance** → `scripts/dev-stack.sh up`, ou `crates/orchestrator` (control plane maison avec une vraie API HTTP)

## Démarrage rapide

```bash
scripts/dev-stack.sh up      # ClickHouse + kernel + query-api, en conteneurs
scripts/demo.sh              # rejeu de scénarios réels + requêtes de bout en bout
```

Kernel (OTLP/gRPC) : `localhost:4317`. Query API (HTTP) : `localhost:8080`.
Les deux exigent un jeton (`docs/client-integration.md#envoyer-de-la-télémétrie`).

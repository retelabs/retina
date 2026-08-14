# Observabilité agentique — contexte harnais

Kernel Rust d'observabilité (inspiré OTel/Datadog) pour workflows d'agents LLM,
avec plugins métiers (fintech en premier). Les décisions produit et
architecture sont tranchées dans
[dossier-observabilite-agentique.md](dossier-observabilite-agentique.md) —
c'est la source de vérité, lis-le avant toute décision structurante et ne
duplique pas son contenu ailleurs.

## Règle permanente

**Avant d'écrire du code touchant une interface externe ou inter-composant,
lire la documentation réelle et fixer précisément le contrat — jamais coder
sur la base d'un souvenir ou d'une supposition.** C'est la seule règle
spéciale de ce projet. Elle s'applique à toute frontière : format OTLP reçu
par l'ingestion, attributs `gen_ai.*` et extensions provider, schéma
ClickHouse, trait du plugin v0, réponse de l'API de requête, etc.

En pratique :
- Utilise `/contract <sujet>` avant de coder une intégration — voir
  [.claude/commands/contract.md](.claude/commands/contract.md).
- Les specs externes versionnées (conventions sémantiques GenAI, proto OTLP)
  sont épinglées dans `vendor/` via `scripts/pin-semconv.sh` et
  `scripts/pin-otlp-proto.sh` — jamais suivies sur `main` en continu (voir
  dossier section 2.1). Vérifie l'état des pins avec `scripts/check-pins.sh`.
- Chaque contrat vérifié est documenté dans `docs/interfaces/<sujet>.md`
  (source, version pinnée, champs réellement utilisés, incertitudes).
- Les questions ouvertes du dossier (section 5) se tranchent via une ADR
  dans `docs/adr/` — utilise `/adr <titre>`. Ne tranche pas seul les
  décisions à impact produit/coût significatif (cloud cible, multi-tenant) :
  présente les options et leurs trade-offs, demande confirmation.

## Ordre de construction du kernel MVP (dossier section 2.2)

1. Modèle de données verrouillé (schéma `gen_ai.*` + extensions provider) —
   `crates/kernel-model` (workspace Cargo racine) contient les 3
   structs d'événement (`ModelCallEvent`, `ToolCallEvent`, `AgentRunEvent`),
   `AttributeValue`/`TokenCount`/`TraceId`/`SpanId`, et les enums semi-ouvertes
   `OperationName`/`ProviderName` — directement dérivés de
   `docs/interfaces/semconv-genai.md` et `docs/interfaces/otlp-ingestion.md`.
   Zéro dépendance externe pour l'instant. `cargo test`/`cargo clippy -- -D
   warnings`/`cargo fmt --check` doivent rester au vert.
2. Ingestion OTLP minimale (`tonic`/`prost`, accepte/valide/persiste brut)
3. Stockage à instance unique (ClickHouse auto-hébergé ou BigQuery/ADX)
4. API de requête minimale (2-3 endpoints)
5. Contrat de plugin v0 (trait Rust, WASM `wasmtime` envisagé)
6. Déploiement squelette (un seul cloud, une région, CI/CD via SaaS factory)
7. Boucle de validation contre le cas fraudos (section 3)

Étape 2 : `crates/otlp-receiver` compile `vendor/opentelemetry-proto`
(`tonic-prost-build` + `protoc-bin-vendored`, pas de dépendance système à
`protoc`), implémente `TraceService::export`, valide chaque span
(trace_id/span_id, champs requis `gen_ai.*`) et convertit vers `kernel-model`
via la couche de mapping `convert.rs`. Succès partiel géré (spans rejetés
comptés, spans hors périmètre MVP non comptés comme rejets).

Étape 3 : `crates/clickhouse-sink` implémente `SpanSink`
(devenu async/par lot/faillible — le driver `clickhouse` commit tout un
`Insert` ou rien) contre une table unique `spans` (schéma et driver
documentés dans `docs/interfaces/clickhouse-schema.md`,
`crates/clickhouse-sink/migrations/0001_create_spans.sql`). Testé contre un
vrai ClickHouse local (`scripts/dev-clickhouse.sh up`,
`cargo test -p clickhouse-sink -- --ignored`) — deux erreurs non prévisibles
depuis la doc seule ont été trouvées et documentées ce faisant (voir la fiche
: ordre `LowCardinality(Nullable(_))`, et le bind de `[u8;N]` en paramètre de
requête). Construit contre le ClickHouse local ; le choix du backend prod
(auto-hébergé vs managé, GCP vs Azure) reste ouvert (dossier section 5).

Étape 4 — **en cours** : `crates/query-api` (`axum` 0.8, attention à la
syntaxe de route `{param}` et pas `:param`) expose les 3 endpoints minimaux
(`GET /traces`, `GET /traces/{trace_id}`, `GET /metrics/summary`), documentés
dans `docs/interfaces/query-api.md`. `/traces/{trace_id}` retourne une liste
plate de spans (pas un arbre JSON imbriqué — le client reconstruit via
`parent_span_id`). Résout l'incertitude laissée par l'étape 3 sur le
paramétrage des requêtes par `trace_id` : binder la représentation hex
(`String`) et utiliser `unhex(?)` côté SQL plutôt que binder `[u8;16]`
directement. Testé à la fois via `tower::ServiceExt::oneshot` (5 tests
d'intégration, `cargo test -p query-api -- --ignored`) et en lançant
réellement le binaire (`cargo run -p query-api`) contre le ClickHouse local.

Étape 5 — **en cours** : `crates/plugin-api` fixe le contrat d'interprétation
v0 (trait `Plugin`, `KernelEvent<'a>` empruntant `kernel-model`,
`PluginOutcome` infaillible — attributs + warnings, pas de rejet dur),
documenté dans `docs/interfaces/plugin-contract-v0.md`. `crates/plugin-example`
est le "plugin factice" que le dossier demande d'écrire pour valider le
contrat (4 tests). Ne dépend pas de `otlp-receiver`/`clickhouse-sink` — pas
encore câblé dans le pipeline, et volontairement générique (pas une ébauche
du plugin fintech). Deux questions restent ouvertes pour plus tard : la
modalité de chargement (trait Rust vs WASM `wasmtime`, dossier section 5) et
où insérer l'appel plugin dans le pipeline — les deux dépendent du premier
vertical réel (étape 7), pas à deviner maintenant.

Chaque étape doit être testable indépendamment et fermée par une fiche de
contrat dans `docs/interfaces/` si elle touche une frontière externe. Utilise
`/kernel-status` pour un état des lieux.

## Hors périmètre volontaire du MVP (dossier section 4)

Multi-tenancy, haute disponibilité/multi-région, couverture exhaustive des
conventions GenAI (retrieval, mémoire…), couche d'analyse agentique
(RCA/anomalies), dashboard riche, multi-cloud simultané. Ne pas anticiper ces
besoins dans le code du kernel MVP.

## Repères techniques

- Ingestion OTLP : `tonic` + `prost`.
- Plugins isolés : `wasmtime`.
- Stockage : driver ClickHouse Rust, ou SDK cloud (BigQuery/ADX) selon
  arbitrage (voir ADR à venir).
- Le SDK Rust d'OpenTelemetry est une référence de conformité protocole, pas
  une dépendance du kernel.

## Scripts disponibles

- `scripts/pin-semconv.sh <ref>` — épingle `vendor/semconv-genai` sur un
  commit/tag exact de `open-telemetry/semantic-conventions-genai`.
- `scripts/pin-otlp-proto.sh <ref>` — épingle `vendor/opentelemetry-proto`
  sur un commit/tag exact de `open-telemetry/opentelemetry-proto`.
- `scripts/check-pins.sh` — rapporte l'état des pins vendorés.
- `scripts/dev-clickhouse.sh up|down` — instance ClickHouse locale pour le
  développement (`docker/docker-compose.clickhouse.yml`).

## Slash commands disponibles

- `/contract <sujet>` — vérifie et documente le contrat d'une interface
  avant d'en coder l'intégration.
- `/kernel-status` — avancement du kernel MVP par rapport aux 7 étapes.
- `/adr <titre>` — nouvelle Architecture Decision Record.

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

Étape 5 : `crates/plugin-api` fixe le contrat d'interprétation
v0 (trait `Plugin`, `KernelEvent<'a>` empruntant `kernel-model`,
`PluginOutcome` infaillible — attributs + warnings, pas de rejet dur),
documenté dans `docs/interfaces/plugin-contract-v0.md`. `crates/plugin-example`
est le "plugin factice" que le dossier demande d'écrire pour valider le
contrat (4 tests). Ne dépend pas de `otlp-receiver`/`clickhouse-sink` — pas
encore câblé dans le pipeline, et volontairement générique (pas une ébauche
du plugin fintech). Deux questions restaient ouvertes : la modalité de
chargement (trait Rust vs WASM `wasmtime`, dossier section 5) et où insérer
l'appel plugin dans le pipeline.

Exploration WASM (dossier section 5) : `crates/plugin-wasm-wire` (DTO JSON
partagés hôte/guest, `kernel-model` reste sans dépendance externe),
`crates/plugin-wasm-example` (même logique que `plugin-example`, compilée en
`wasm32-unknown-unknown` via `scripts/build-wasm-plugins.sh`), et
`crates/plugin-wasm-host` (`WasmPlugin`, implémente `Plugin` en chargeant le
`.wasm` via `wasmtime` 47, module "core" + ABI mémoire linéaire maison — pas
le Component Model, voir `docs/interfaces/wasm-plugin-loading.md` pour le
pourquoi). Décision retenue : module WASM "core", pas le Component Model
(outillage `cargo-component`/`wasm-tools` absent de l'environnement, et
sur-designer avant de savoir si WASM est retenu durablement serait
contraire à la logique de l'étape 5). **Deux trouvailles réelles en cours de
route** : `extern "C" fn(...) -> (i32, i32)` compile mais `rustc` avertit
`improper_ctypes_definitions` (layout de tuple non garanti) — remplacé par un
retour `i64` empaqueté, sans ambiguïté ; et `Plugin::name() -> &'static str`
ne convient pas à un plugin chargé dynamiquement (le nom n'existe qu'à
l'exécution) — relâché en `&str` (changement rétrocompatible, vérifié).
Testé en chargeant le vrai `.wasm` compilé via `wasmtime` et en comparant
bit à bit sa sortie à celle du plugin natif `ExamplePlugin` (4 tests
`--ignored`, `cargo test -p plugin-wasm-host -- --ignored`) — pas juste "ça
ne plante pas", une vraie preuve d'équivalence comportementale entre les deux
modalités de chargement.

Deuxième vertical réel + câblage du plugin dans le pipeline (2026-08-15) :
`crates/plugin-medical` interprète les invariants de gouvernance
**réellement lus** dans `the oncology pipeline repository` (oncologie,
cloné en lecture seule) — gate de conformité HIPAA/GDPR déterministe
(Presidio/NER) et gate HITL (`interrupt_before`), tous deux cités mot pour
mot depuis le `CLAUDE.md` du repo source. Trouvaille notable : une deuxième
implémentation du même vertical (`client-project`) fait juger la conformité
RGPD **par le LLM lui-même** (texte libre, pas de sortie structurée) —
divergence réelle entre deux systèmes de prod, pas supposée. Détails et
mapping dans `docs/interfaces/oncology-governance.md`, `crates/oncology-replay`
(3 fixtures). **`crates/plugin-sink`** (`PluginSink<S: SpanSink>`) résout la
question laissée ouverte depuis l'étape 5 ("où insérer l'appel plugin dans
le pipeline") : décorateur autour de n'importe quel `SpanSink`, câblé pour de
vrai dans `crates/kernel` (enveloppe `ClickHouseSink`, avec `FraudosPlugin`
et `MedicalPlugin`) — première fois qu'un plugin tourne dans l'ingestion
réelle, pas seulement en test isolé. `query-api::/metrics/summary` expose
`spans_with_warnings`. Vérifié de bout en bout : kernel réel + rejeu
fraudos/oncologie + avertissements attendus retrouvés en base ET dans les
métriques.

Étape 7 : validé contre le cas fraudos réel (`the fraudos prototype repository`, cloné
en lecture seule, pas vendoré). **Correction au dossier section 3** :
`fraudos-prototype` n'a en réalité aucune instrumentation ADOT/OTel — observabilité
maison (`AgentSpan` → CloudWatch/DynamoDB) — et le score de fraude vient d'un
appel outil (`get_transaction_score`), pas de la sortie du LLM. Détails et
mapping complet dans `docs/interfaces/fraudos-agentspan.md`. Décision prise
avec l'utilisateur : plutôt que de modifier `fraudos-prototype` (repo séparé,
credentials AWS requises), `crates/fraudos-replay` convertit des `AgentSpan`
réalistes (fixtures dans `fixtures/`, ancrées sur les vrais noms de rôles/
outils/modèles du repo) en OTLP et les rejoue en gRPC réel contre
`crates/kernel` (nouveau bin qui câble `otlp-receiver` + `clickhouse-sink` —
premier binaire du kernel réellement exécutable). Vérifié de bout en bout à
la main : `scripts/dev-clickhouse.sh up` → `cargo run -p kernel` → `cargo run
-p fraudos-replay -- <fixture>` → requêtes réelles contre `query-api`,
arbre de trace et agrégats corrects. `otlp-receiver` génère maintenant aussi
le client gRPC (`TraceServiceClient`), pas seulement le serveur.

Étape 6 — **en cours** : squelette de déploiement, cloud cible volontairement
**non tranché** (dossier section 5) — décision explicite avec l'utilisateur
de tout construire soi-même par-dessus du hardware nu plutôt que des services
managés spécifiques à un cloud (voir échange du 2026-08-14 sur GCP/Azure).
`docker/kernel.Dockerfile` et `docker/query-api.Dockerfile` (build multi-stage
`rust:1.97.1-slim-bookworm` → `debian:bookworm-slim`, aucun paquet système
requis à la compilation — tout est en Rust pur) produisent les mêmes images
quel que soit le cloud choisi plus tard ; seul le provisionnement de la VM en
dépendra. `docker/docker-compose.stack.yml` (`scripts/dev-stack.sh`) assemble
ClickHouse + `kernel` + `query-api` avec ces images — **vérifié réellement
construit et lancé en conteneurs**, rejeu fraudos + requêtes `query-api`
correctes de bout en bout (pas seulement `cargo run`). `.gitlab-ci.yml`
(stages `check`/`test`/`build-images`) fait tourner `fmt`/`clippy`/`cargo
test --workspace` + les tests `--ignored` contre un vrai service ClickHouse
en CI, et pousse les images vers le Container Registry GitLab seulement sur
`main` — écrit d'après la doc GitLab CI vérifiée (variables prédéfinies
`CI_REGISTRY*`, pattern `docker:dind`), mais **pas encore vérifié sur un vrai
runner** : rien n'a été poussé sur `origin` (accord requis avant tout push,
voir mémoire git-workflow).

Chaque étape doit être testable indépendamment et fermée par une fiche de
contrat dans `docs/interfaces/` si elle touche une frontière externe. Utilise
`/kernel-status` pour un état des lieux.

## Après le MVP — combler les limites produit (2026-08-15, en cours)

Post-étape 7, avec l'utilisateur : le kernel tourne et est validé contre un
vrai vertical, mais plusieurs lacunes empêchent d'en faire un vrai produit
(pas de multi-tenant exclu volontairement — voir section suivante — mais
authentification, chargement dynamique des plugins, découplage de
l'exécution des plugins du chemin critique, stratégie de rétention).
Priorité choisie avec l'utilisateur : l'authentification d'abord (la seule
qui expose vraiment le kernel dès qu'il sort de `localhost`).

**Authentification — fait.** Contrat vérifié et documenté dans
`docs/interfaces/kernel-auth.md` avant d'écrire le code (API
`tonic::service::Interceptor`/`TraceServiceServer::with_interceptor`
inspectée dans le code généré réel, pas depuis la doc seule ; API
`axum::middleware::from_fn_with_state` vérifiée contre docs.rs pour la
version exacte 0.8.9 ; convention de header `authorization: Bearer <token>`
alignée sur `OTEL_EXPORTER_OTLP_HEADERS`, le mécanisme standard qu'un vrai
SDK OTel utilise déjà sans code custom). Secret partagé statique par
surface (pas de JWT/OAuth — proportionné à un kernel mono-tenant,
dossier section 4) : `KERNEL_API_KEY` pour `otlp-receiver`/`crates/kernel`,
`QUERY_API_KEY` pour `crates/query-api` — deux jetons distincts parce
qu'écriture (ingestion) et lecture (query) ne sont pas le même niveau de
confiance. Échec fermé : les deux binaires refusent de démarrer si la
variable d'environnement correspondante est absente (vérifié en lançant
réellement les deux binaires sans la variable — panic immédiat, pas un
serveur qui tourne sans protection). Comparaison en temps constant pour
éviter une fuite de timing sur le jeton. `fraudos-replay`/`oncology-replay`
attachent désormais le header à chaque appel gRPC réel.

Vérifié à trois niveaux : tests unitaires de l'intercepteur/du middleware
(8 tests, y compris rejet sans header et avec mauvais jeton) ; tests
d'intégration `query-api` contre un vrai ClickHouse via
`tower::ServiceExt::oneshot`, incluant un nouveau test qui prouve le rejet
401 (`cargo test -p query-api -- --ignored`) ; bout en bout réel via
`scripts/demo.sh` contre la pile Docker (`scripts/dev-stack.sh up`, jetons
dev fixes dans `docker-compose.stack.yml`, même posture que
`CLICKHOUSE_PASSWORD: dev` déjà en place) — rejeu fraudos accepté par le
kernel réel via gRPC authentifié, requêtes `query-api` authentifiées
retournant les traces/métriques attendues.

Reste à faire, pas encore commencé : chargement dynamique des plugins
(actuellement codés en dur dans `crates/kernel/src/main.rs`), découpler
`PluginSink` du chemin synchrone d'ingestion, stratégie de rétention/évolution
de schéma ClickHouse.

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

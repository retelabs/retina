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

**Activation des plugins par config — fait.** Portée délibérément réduite,
tranchée avec l'utilisateur : activer/désactiver par configuration les
plugins natifs déjà compilés (`FraudosPlugin`, `MedicalPlugin`), pas un
vrai chargement dynamique de code arbitraire — ça, c'est la question WASM
(`crates/plugin-wasm-host`, déjà construite et vérifiée équivalente au
plugin natif mais toujours pas branchée dans le pipeline réel), qui reste
ouverte et volontairement pas attaquée ici parce qu'elle soulève une
question non tranchée en plus (gestion d'un plugin WASM tiers qui panique/
boucle, voir `docs/interfaces/plugin-contract-v0.md`).

`ENABLED_PLUGINS` (`crates/kernel/src/main.rs`, fonction pure
`select_enabled` testée séparément de la lecture d'env) : liste de noms
séparés par des virgules parmi les noms réels retournés par
`Plugin::name()` (`fraudos-plugin`, `medical-plugin`). Non défini = tous les
plugins tournent (comportement identique à avant, aucune config nouvelle
requise pour `scripts/demo.sh` ou un déploiement existant) ; chaîne vide
explicite = aucun plugin ; un nom inconnu fait paniquer le démarrage plutôt
que d'être ignoré silencieusement (une faute de frappe qui désactiverait un
plugin sans avertissement serait pire qu'un crash au démarrage). Résolu
avant toute connexion ClickHouse, au même endroit que la validation de
`KERNEL_API_KEY` — une erreur de config doit échouer immédiatement, pas
après un aller-retour réseau.

Vérifié : 5 tests unitaires sur `select_enabled` (non défini, chaîne vide,
sous-ensemble valide, espaces tolérés, nom inconnu → panic) ; et les 4
scénarios lancés pour de vrai (`cargo run -p kernel` contre un vrai
ClickHouse) — nom invalide paniqué avant tout accès réseau, chaîne vide
démarrée avec 0 plugin, non défini démarré avec tous les plugins, sous-
ensemble valide démarré normalement.

**Découplage de l'exécution des plugins — fait.** Portée tranchée avec
l'utilisateur : isoler panic + borner le temps d'exécution, sans détacher
l'écriture elle-même du chemin synchrone (la garantie "une réponse de
succès OTLP veut dire que c'est en base" ne change pas — la détacher
vraiment aurait été un chantier séparé et plus lourd, avec sa propre
question ouverte : que faire des spans en file si le worker tombe avant
d'écrire, pas de queue durable aujourd'hui).

Le vrai bug corrigé : avant, un plugin qui panique faisait perdre tout le
batch — le panic remontait à travers `accept_batch` *avant* que
`inner.accept_batch` (l'écriture ClickHouse) ne soit jamais appelé, pas
seulement la contribution de ce plugin. `crates/plugin-sink/src/lib.rs`
exécute maintenant chaque plugin via `tokio::task::spawn_blocking` sous un
timeout (`PLUGIN_TIMEOUT`, 100ms — généreux pour un plugin synchrone sans
I/O comme le contrat l'exige aujourd'hui, docs/interfaces/plugin-contract-v0.md) :
`spawn_blocking` isole le panic à la frontière de la tâche (tokio le
transforme en `JoinError`, pas un unwind qui remonte jusqu'ici) ; le
timeout borne le temps qu'un plugin peut retenir un batch (le thread
natif ne peut pas être tué de force, il continue en arrière-plan et son
résultat est simplement jeté). Les deux cas dégradent en
`plugin.warning` sur l'événement concerné, pas en perte du batch.
`Vec<Box<dyn Plugin>>` devenu `Vec<Arc<dyn Plugin>>` en interne (clonable
dans la tâche bloquante, `Plugin: Send + Sync` déjà garanti par le contrat) ;
`ConvertedEvent` a gagné `Clone` (nécessaire pour donner à la tâche bloquante
sa propre copie `'static`, l'emprunt `KernelEvent<'a>` habituel ne survit
pas à la frontière de tâche).

Vérifié : 4 tests unitaires dont un plugin qui panique volontairement
(preuve que le reste du batch et les autres plugins survivent) et un
plugin qui dort 2s (preuve que `accept_batch` revient en moins de 500ms,
pas 2s) ; les tests d'intégration `plugin-sink`/`query-api` contre un vrai
ClickHouse toujours au vert avec le nouveau chemin async ; bout en bout
réel via `scripts/demo.sh` contre la pile Docker reconstruite.

**Rétention/évolution de schéma ClickHouse — fait.** Durée tranchée avec
l'utilisateur : 90 jours (assez pour investiguer un incident a posteriori
sans accumuler indéfiniment). Syntaxe TTL ClickHouse vérifiée contre la doc
réelle avant d'écrire la migration ; comportement de `max()` sur table vide
(`0`, pas `NULL`) vérifié empiriquement contre un vrai serveur (la doc seule
ne le précisait pas).

Avant cette étape, une seule migration existait, appliquée à chaque
démarrage via un `CREATE TABLE IF NOT EXISTS` — idempotent par chance, pas
par conception, et dupliquée (`include_str!` recopié dans `crates/kernel` et
3 suites de tests d'intégration). `crates/clickhouse-sink/src/migrate.rs`
(`run_migrations`, exportée) résout les deux problèmes en même temps :
table `schema_migrations` (version/nom/date), migrations numérotées
appliquées dans l'ordre et enregistrées, un seul endroit qui connaît la
liste. `migrations/0002_spans_retention_ttl.sql` (`ALTER TABLE spans MODIFY
TTL start_time + INTERVAL 90 DAY DELETE`) est la première migration réelle
au-delà de la création initiale — exactement ce que le mécanisme devait
prouver. Détails complets dans `docs/interfaces/clickhouse-retention.md`.

Hypothèse mono-instance assumée (dossier section 4, pas de HA au MVP) :
deux processus qui appliqueraient la même migration en même temps ne sont
pas gérés — il n'existe qu'un seul kernel aujourd'hui.

Vérifié contre un vrai ClickHouse, pas seulement en lisant la doc : base
entièrement fraîche → les deux migrations appliquées et enregistrées,
`system.tables.engine_full` confirme le TTL sur `spans` ; et surtout le
**scénario de mise à niveau réel** — `spans` recréée sans TTL et
`schema_migrations` supprimée pour simuler un déploiement antérieur à cette
fonctionnalité, puis `cargo run -p kernel` réellement lancé contre cette
base : la migration 2 s'est appliquée automatiquement au démarrage sans
intervention manuelle.

Avec ceci, les quatre limites produit identifiées le 2026-08-15 sont
comblées (authentification, activation des plugins par config, isolation de
l'exécution des plugins, rétention/évolution de schéma).

## Exploration cloud/infra — objectif d'apprentissage, pas un choix de fournisseur (2026-08-15, en cours)

Dossier section 5 laissait "GCP vs Azure" ouvert. Discussion avec
l'utilisateur : ce qui compte vraiment n'est pas le logo du cloud mais
acquérir, en codant en direct, les concepts et la méthode pour construire —
et chiffrer — ses propres briques d'infrastructure plutôt que de consommer
des services managés tout faits. Le choix du fournisseur reste ouvert,
volontairement secondaire à cet objectif.

**Critère précisé le 2026-08-15 (fin de journée)** : le rejet des services
managés n'est pas catégorique — c'est spécifiquement le coût **facturé
indépendamment de l'usage** qui doit être évité (frais de control plane
Kubernetes managé même à zéro pod, bases managées facturées à l'instance
provisionnée type RDS/ClickHouse Cloud, capacité réservée type DynamoDB
provisioned/NAT gateway à l'heure). Un vrai pay-per-use (facturé à l'appel,
zéro usage = zéro facture) ou un coût fixe déjà minimal et accepté (une
VM/VPS pas chère qu'on paie de toute façon, cf. le choix "hardware nu +
Docker" déjà fait) restent acceptables — le "gâchis" y est plafonné et
connu d'avance, pas une surprise de facturation. Implication directe pour
le chantier "modèle de coût" à venir : calculer le **coût à usage zéro** de
chaque option, pas seulement à volume attendu — c'est ce chiffre qui doit
dominer la comparaison tant que le volume réel reste proche de zéro.

**Proposition d'architecture validée comme point de départ (2026-08-16)** :
cœur auto-hébergé (VM + `orchestrator` + ClickHouse/kernel/query-api)
commun aux deux options envisagées, extensions "hybrides" pay-per-use
(registre de conteneurs, edge/CDN, stockage objet pour les backups) ajoutées
une par une seulement si elles comblent un vrai manque — diagramme dans
`docs/interfaces/` à venir si le choix se stabilise. Testé et ajusté au fil
des découvertes, pas figé. **Deuxième critère explicite** : même pour une
extension pay-per-use légitime (coût nul à usage nul), préférer la coder
nous-mêmes quand la valeur d'apprentissage le justifie — le critère de coût
(section précédente) reste ce qui tranche quand l'effort de réécriture
dépasse ce que ça enseigne (ex. la durabilité du stockage objet est un vrai
chantier d'ingénierie, pas juste un exercice).

`crates/orchestrator` — un control plane "maison" en Rust, contre l'API
Engine de Docker directement (crate `bollard` 0.21.0), pas une enveloppe de
`docker compose`. Contrat vérifié en lisant le vrai code source de
`bollard`/`bollard-stubs` depuis le registre Cargo local (pas seulement
docs.rs, qui n'a pas donné les noms de champs exacts) — documenté dans
`docs/interfaces/docker-engine-api.md`. V0 volontairement réduit à un seul
service (`ClickHouse`, le plus simple des trois du dossier étape 6 — pas de
build d'image maison) : `ensure_running`/`status`/`wait_healthy`/`teardown`,
tous **idempotents** (propriété centrale d'un vrai control plane — une
boucle de réconciliation doit pouvoir reconverger sans se soucier de l'état
de départ). `wait_healthy` est une boucle observer/comparer/attendre en
miniature, le même principe qu'un contrôleur Kubernetes/Nomad réduit à
l'essentiel.

Trois trouvailles réelles en lisant le code source plutôt qu'en devinant :
`create_image` (pull) retourne un `Stream` paresseux, pas un `Future` — rien
ne se passe tant qu'il n'est pas consommé ; les types de bollard ont changé
de nom entre versions (`ContainerCreateBody` pas `Config<String>`) ; et
distinguer "conteneur jamais créé" de "existe mais arrêté" demande
`list_containers(all: true)` filtré par nom, pas `inspect_container` seul.

Vérifié contre le vrai démon Docker local : cycle de vie complet
(`cargo run -p orchestrator`) et test d'intégration d'idempotence
(`cargo test -p orchestrator -- --ignored`) — `ensure_running` rappelé sur
un conteneur déjà sain ne casse rien, `teardown` rappelé sur un conteneur
déjà absent non plus.

**Étendu à `kernel`/`query-api` — fait.** `ManagedService` gagne
`image_source` (`Registry` vs `Local` — `kernel`/`query-api` doivent déjà
être construits via `docker/docker-compose.stack.yml`, ce control plane ne
construit pas d'image lui-même), `ports`, `depends_on`. `deploy_all` trie
les services par dépendance (tri topologique, algorithme de Kahn,
déterministe) puis, dans cet ordre, `ensure_running` + `wait_healthy` avant
le suivant — au moment où `kernel`/`query-api` démarrent, ClickHouse est
déjà sain, exactement ce que `depends_on: condition: service_healthy` donne
sous `docker compose`, reconstruit depuis l'API.

Trouvaille structurante : les conteneurs du réseau `bridge` par défaut de
Docker ne se résolvent **pas** par nom — seul un réseau défini par
l'utilisateur le permet. `ensure_network` crée un tel réseau et
`ensure_running` y attache chaque conteneur (`HostConfig.network_mode`)
pour que `kernel` joigne `CLICKHOUSE_URL=http://<nom du conteneur>:8123`.
**Deuxième trouvaille, une vraie race, pas construite exprès** : la propre
suite de tests du crate (deux tests qui appellent `ensure_network` sur le
même nom, tournant en parallèle par défaut) a fait échouer le
vérifier-puis-créer initial avec un `409` — corrigé en traitant "already
exists" comme un succès, même logique que le `304` déjà géré pour
`start_container`. Reproduit et corrigé, pas juste contourné dans le test.

Vérifié bout en bout réel, pas seulement `docker inspect` :
`cargo run -p orchestrator -- --keep-running` déploie les 3 services, un
vrai rejeu gRPC (`fraudos-replay`) contre `localhost:4317` et une vraie
requête HTTP authentifiée contre `localhost:8080/metrics/summary`
confirment que `kernel` a réellement écrit dans ClickHouse via le réseau
partagé et que `query-api` relit les mêmes données. Plus 4 tests unitaires
(`topological_order`, sans Docker) et 2 tests d'intégration `--ignored`
contre un vrai démon, chacun relancé plusieurs fois pour confirmer que le
fix de la race n'était pas un coup de chance.

**API HTTP — fait.** `crates/orchestrator/src/main.rs` est maintenant un
vrai service (`axum::serve`, `POST /deploy`/`GET /status`/`POST /teardown`),
même layering que `crates/query-api` (routes/DTOs séparés du client Docker)
pour la cohérence du workspace. `deploy`/`teardown` restent idempotents à
travers la couche HTTP — même garantie que `docker_client`, pas perdue en
l'enveloppant. Mapping d'erreur distingué par code HTTP (400 dépendance
invalide, 422 image locale manquante, 504 timeout de santé, 500 erreur
Docker générique) plutôt que tout renvoyer en 500.

**Pas d'authentification, délibérément** : outil d'apprentissage local,
`ORCHESTRATOR_BIND` par défaut sur `127.0.0.1` (pas `0.0.0.0` comme
kernel/query-api) — noté explicitement que s'il tourne un jour sur un
réseau atteignable, il lui faut le même traitement que kernel/query-api
d'abord, vu qu'il peut arrêter des conteneurs.

Vérifié à deux niveaux : `tower::ServiceExt::oneshot` contre le vrai
`Router` (cycle `/status` → `/deploy` → `/deploy` de nouveau (idempotence)
→ `/teardown`, contre la vraie pile à 3 services) ; et manuellement,
serveur réellement lancé, `curl` contre les 3 routes puis un vrai rejeu
gRPC (`fraudos-replay`) et une vraie requête `query-api` confirmant que les
conteneurs déployés par l'API HTTP fonctionnent pour de vrai, pas
seulement `status: "Healthy"`.

**Construction d'image via l'API — fait.** `kernel`/`query-api` utilisent
`ImageSource::Build { context, dockerfile }` — plus de `docker build`
externe requis avant `deploy_all`. `crates/orchestrator/src/image_build.rs`
construit un tar du contexte (racine du repo, résolue via
`CARGO_MANIFEST_DIR`) en mémoire, respectant `.dockerignore` mais
volontairement réduit (matching par composant à n'importe quelle
profondeur, pas d'ancrage `/` ni de négation `!` — le vrai `.dockerignore`
de ce repo n'a besoin ni de l'un ni de l'autre). `ensure_image` envoie ce
tar à `POST /build` (`bollard::Docker::build_image`) et consomme le flux de
progression jusqu'à la fin ou une erreur.

Construit à chaque déploiement, pas seulement si l'image est absente — même
sémantique que `docker build`/`docker compose build`, le cache de couches
de Docker rend un contexte inchangé rapide à reconstruire. Limite connue,
documentée : un conteneur déjà démarré depuis une image plus ancienne n'est
pas recréé automatiquement après un rebuild, il faut le détruire d'abord.

Trouvaille réelle : `BuildInfo` n'a pas de champ `error` plat comme
`CreateImageInfo` — seulement `error_detail: Option<ErrorDetail>`. Deviné
faux par analogie avec `create_image` en écrivant le code une première
fois, corrigé en relisant le vrai struct dans `bollard-stubs`.

Vérifié contre un vrai démon Docker : un test dédié construit réellement
`docker/kernel.Dockerfile` via l'API (`cargo build --release -p kernel`
tourne pour de vrai dans le conteneur builder, ~64s à froid) et confirme
que l'image produite a le bon `ENTRYPOINT`. Bout en bout réel :
`POST /deploy` construit maintenant `kernel`/`query-api` lui-même avant de
les démarrer, suivi d'un vrai rejeu gRPC (`fraudos-replay`) et d'une vraie
requête `query-api` confirmant que l'image construite par notre propre code
fonctionne réellement — plus seulement testé, le prérequis externe
`scripts/dev-stack.sh build` a disparu pour de vrai.

**Authentification sur `crates/orchestrator` — fait (2026-08-16).** Même
mécanisme que `kernel`/`query-api` (`docs/interfaces/kernel-auth.md`) :
`ORCHESTRATOR_API_KEY`, échec fermé au démarrage, header `authorization:
Bearer <token>`. Décision délibérée : `src/auth.rs` dupliqué depuis
`crates/query-api/src/auth.rs` plutôt que factorisé dans un crate partagé —
deuxième service axum à en avoir besoin, pas un troisième, chaque crate
reste auto-suffisant. `ORCHESTRATOR_BIND` reste par défaut sur `127.0.0.1` —
l'authentification s'ajoute à cette prudence, ne la remplace pas.

Vérifié à deux niveaux : `cargo test -p orchestrator -- --ignored`
(nouveau test de rejet 401, test HTTP existant mis à jour avec le header)
contre le vrai `Router` ; et bout en bout réel — lancé sans
`ORCHESTRATOR_API_KEY` → panic avant toute connexion Docker, lancé avec →
`curl` sans header confirmé `401`, avec le bon header un cycle
`/deploy`→`/status`→`/teardown` complet, suivi d'un vrai rejeu gRPC
(`fraudos-replay`) et d'une vraie requête `query-api` confirmant que la
pile déployée derrière l'auth fonctionne réellement.

Avec ceci, les deux lacunes de sécurité identifiées sur `crates/orchestrator`
(pas d'auth, prérequis d'image externe) sont comblées.

**Modèle de coût réel — fait (2026-08-16).** `crates/cost-model` rend
vérifiable avec de vrais nombres le critère posé la veille (comparer par le
**coût à usage zéro**). Deux catégories d'entrées, chacune vérifiée à sa
façon : octets/span **mesurés** contre un vrai ClickHouse
(`system.parts.data_compressed_bytes`, pas une estimation analytique
depuis le schéma qui ignorerait la compression réelle) ; prix VM/stockage
objet/CDN **vérifiés contre les pages officielles réelles**, pas des
agrégateurs (une première recherche via agrégateurs a donné des chiffres
Hetzner contradictoires, écartés). Détail complet des sources dans
`docs/cost-model.md`.

Résultat concret, pas juste une méthode : à `0` spans/jour les extensions
hybrides tombent exactement à `0` — la preuve numérique du critère
d'hier. Au repère du dossier pour l'arbitrage ClickHouse managé/auto-hébergé
(100 000 spans/jour), le stockage accumulé sur 90 jours reste sous le
palier gratuit Backblaze B2 — le coût qui domine reste la VM (identique
entre "100% perso" et "hybride"), pas les extensions.

Vérifié à deux niveaux : tests unitaires sur `report.rs` (fonctions pures,
sans Docker ni ClickHouse) et un test `--ignored` qui mesure réellement
contre un vrai ClickHouse contenant les fixtures `fraudos-replay`/
`oncology-replay` déjà rejouées. `cargo run -p cost-model` lu à la main
contre les données réelles de la session — le stockage croît avec le
volume, la VM reste fixe, comme attendu.

Pas encore fait, notes explicites dans `docs/cost-model.md` : egress du
stockage objet au-delà du palier gratuit, décomposition du coût de calcul
par service (la VM est traitée comme un coût fixe unique), comparaison à
d'autres fournisseurs VM (OVH, Scaleway, DigitalOcean) — Hetzner est un
premier point de repère vérifié, pas une décision de fournisseur.

## Hors périmètre volontaire du MVP (dossier section 4)

Multi-tenancy, haute disponibilité/multi-région, couverture exhaustive des
conventions GenAI (retrieval, mémoire…), couche d'analyse agentique
(RCA/anomalies), dashboard riche, multi-cloud simultané. Ne pas anticiper ces
besoins dans le code du kernel MVP.

## Documentation client (2026-08-16)

Cartographie de l'état du projet avec l'utilisateur : aucun `README.md` à
la racine n'existait, et `docs/interfaces/` documente des contrats vérifiés
pour nous (source, date de vérification) — pas une doc orientée "comment
utiliser Venice depuis mon application". `README.md` (racine, nouveau) et
`docs/client-integration.md` comblent ça : synthèse orientée client de
contrats déjà vérifiés (mapping `gen_ai.operation.name` → événement kernel
avec ses champs requis exacts, tiré du vrai code de dispatch
`crates/otlp-receiver/src/convert.rs` plutôt que reformulé de mémoire ;
convention d'attributs `fraudos.*`/`oncology.*` ; les 3 endpoints
`query-api` avec la liste complète des champs de `SpanDto`), rien de
nouveau tranché ici. Pointeurs vers `docs/interfaces/` pour qui veut le
détail vérifié complet, pas de duplication.

Autre constat de la cartographie, traité le jour même : `docs/adr/` n'avait
que le template, aucune ADR n'avait jamais été écrite malgré `/adr` — les
4 questions ouvertes listées dans `docs/adr/README.md` (dossier section 5)
étaient en réalité déjà tranchées, juste jamais formalisées en ADR.
Rédigées rétroactivement : `0001` (multi-tenant hors périmètre),
`0002` (modèle d'hébergement — auto-hébergé, fournisseur toujours différé),
`0003` (ClickHouse auto-hébergé), `0004` (plugins natifs, WASM différé pas
rejeté). Aucune nouvelle décision tranchée par l'exercice — une
rétro-documentation, pas une nouvelle négociation.

## Premier client réel branché : client-project (the-client) (2026-08-17, en cours)

Venice sert désormais de kernel d'observabilité pour un vrai projet client
(`a separate client project`,
SaaS santé .NET, agents triage/résumé/conformité/enrichissement d'appel,
plugin `MedicalPlugin` branché temporairement dessus — le nom "oncology"
sera généralisé plus tard). Le câblage OTLP/gRPC fonctionne en conditions
réelles. En creusant l'écart avec ce qu'un outil comme LangSmith donne, deux
manques sont ressortis et scopés avec l'utilisateur avant tout code (règle
permanente du projet) : coût $ par span, et suivi de conversation/thread.

**Suivi de conversation** : `conversation_id` existe déjà dans le schéma
mais rien ne le peuplait côté the-client. Exploration réelle du code the-client (pas
supposée) : `AgentOrchestrator.RunAsync` est le seul point d'ouverture du
span `invoke_agent` pour les 4 agents ; seul l'enrichissement d'appel
(`EnrichCallWithAiCommand`) porte un id métier réel (`CallId`) sur ce
chemin — triage/résumé/conformité sont des endpoints "playground" texte
libre (`RunAgentQuery`/`AiController`), sans identifiant de domaine.
Fausse piste éliminée : `OutboxEntry.CorrelationId` existe dans leur modèle
mais n'est jamais peuplé. **Décision côté the-client (leur équipe, pas Venice)**,
vérifiée contre leur frontend aussi (`AiComponent`, aucun `callId` sur ce
chemin) : on laisse tel quel — construire le lien manquant serait une vraie
feature de navigation, hors scope pour l'instant. Conséquence côté kernel :
seul l'agent d'enrichissement d'appel portera jamais un `conversation_id`
non-null pour the-client dans l'état actuel, pas une limite à corriger côté
Venice.

**Calculateur de coût $ — fait.** Trois questions scopées avec l'utilisateur
avant de coder (`docs/interfaces/cost-calculation.md`, détail complet) :
table de prix statique versionnée dans le repo (pas fournie par le client),
calcul une seule fois à l'ingestion (pas à la requête), un changement de
tarif ne recalcule jamais l'historique déjà stocké. Nouveau crate
`crates/pricing` (dépend seulement de `kernel-model` pour `ProviderName`),
appelé depuis `crates/clickhouse-sink/src/row.rs` — le point où
`provider_name`/modèle/tokens sont déjà rassemblés par type d'événement,
donc kernel-model reste une dérivation pure de semconv sans logique
business, et `otlp-receiver::convert.rs` reste un mapping protocole pur.
Nouvelle colonne `spans.cost_usd Nullable(Float64)`
(migration `0003_add_cost_usd.sql`), exposée dans `SpanDto.cost_usd` et
`MetricsSummaryDto.by_kind[].total_cost_usd`.

**Trouvaille structurante, vérifiée contre `docs/interfaces/semconv-genai.md`
avant de coder (déjà documentée à l'étape 1, pas redécouverte)** : la
comptabilité des tokens de cache diffère par fournisseur — Anthropic exclut
les tokens de cache d'`input_tokens` (à rajouter), OpenAI/Azure les
incluent déjà. Une formule de coût unique aurait été fausse pour l'un des
deux. `CacheAccounting` (`IncludedInInput`/`AdditionalToInput`) encode
cette différence explicitement plutôt que de deviner une formule
universelle.

Prix vérifiés contre les vraies pages officielles le 2026-08-17 (pas
depuis la mémoire, règle permanente du projet) : OpenAI
(`developers.openai.com/api/docs/pricing`), Anthropic
(`platform.claude.com/docs/en/about-claude/pricing`). Groq (demandé
explicitement par l'utilisateur) **non tarifé, délibérément** : la page
officielle n'a renvoyé aucun tableau exploitable et la doc console a
renvoyé 404 — seuls des agrégateurs tiers avaient des chiffres, écartés
pour la même raison que la divergence Hetzner déjà rencontrée dans
`docs/cost-model.md` (chiffres non fiables). AWS Bedrock/watsonx/GCP/Azure/
Cohere/Perplexity/xAI/DeepSeek/Mistral/Moonshot : non tarifés non plus, non
demandés et non vérifiés cette session — un span de ces fournisseurs reste
`cost_usd = NULL`, jamais un mauvais chiffre.

Lacune connue, documentée plutôt que masquée : aucune télémétrie réelle
n'existait pour confirmer le format exact des chaînes `request_model`/
`response_model` envoyées en pratique (the-client ne peuple aujourd'hui aucun des
deux) — la table de prix fait un lookup par correspondance exacte, à
vérifier contre de vraies réponses d'API avant de faire confiance à sa
couverture au-delà des montants par token eux-mêmes.

Vérifié à deux niveaux : 7 tests unitaires `crates/pricing` (comptabilité
de cache par fournisseur, modèle/provider inconnu → `None` pas une erreur,
table de prix cohérente avec `ProviderName::as_str()`) et 4 nouveaux tests
`clickhouse-sink`/`query-api` ; **`sum(cost_usd)` vérifié empiriquement
contre un vrai ClickHouse** (pas supposé) : `NULL` sur un groupe vide *et*
sur un groupe entièrement `NULL` — d'où `total_cost_usd: Option<f64>`,
délibérément pas ramené à `0.0` comme les totaux de tokens, pour ne pas
confondre "aucun span tarifé" avec "coût réellement nul". Bout en bout réel
via `cargo test -p clickhouse-sink -p query-api -- --ignored` contre
`scripts/dev-clickhouse.sh up` : un span `gpt-4o-mini` réellement inséré,
relu via `GET /traces/{trace_id}` et `GET /metrics/summary`, coût exact
au centime près.

**Câblage `gen_ai.usage.*`/`request.model` côté the-client — fait, vérifié en
conditions réelles (2026-08-17).** Une fois `dev` poussé (`926e6a5`), the-client a
câblé ses 3 clients IA pour poser `gen_ai.provider.name`/`request.model`/
`usage.input_tokens`/`usage.output_tokens` sur l'`Activity` (leur commit
`d1c632f`, noms vérifiés contre `AGENT_RUN_KNOWN_KEYS` réel plutôt que
devinés) puis reconstruit `kernel`/`query-api` depuis leur propre clone.
Test réel via `/api/ai/triage` : `provider_name=anthropic`,
`request_model=claude-sonnet-5`, `input_tokens=229`, `output_tokens=7` →
`cost_usd=0.000528`, vérifié à la main (`229×$2/M + 7×$10/M`) et exact.
`GET /metrics/summary` agrège correctement `total_cost_usd` pour
`agent_run` (le seul kind que the-client émet) ; `model_call`/`tool_call` restent
`null`, attendu puisque the-client ne les émet pas. Premier chiffrage de coût $
réel de bout en bout depuis un vrai appel client, pas seulement un span
synthétique — clôt le chantier ouvert en début de journée (l'écart constaté
avec LangSmith).

**Évals sans LangSmith — volet 1 (déterministe) fait, volet 2 en attente
(2026-08-17).** Deuxième manque identifié par l'utilisateur the-client face à
LangSmith : des évals dans le même esprit déterministe que `MedicalPlugin`
plutôt que le pattern LLM-as-judge de LangSmith — argument économique
explicite (des boucles d'eval en CI avec juge LLM répété coûtent cher à
chaque run, maximiser le déterministe réduit ce coût directement).

Cas réels demandés et reçus de the-client (pas inventés) pour les 3 agents
(triage/summary/compliance) avant de designer quoi que ce soit — un seul
avait un référentiel canonique comparable à une sortie catégorielle
(`crates/plugin-triage-eval`, détail complet et sources dans
`docs/interfaces/triage-eval-plugin.md`) : le prompt de triage a un
vocabulaire ouvert ("for example: ..."), mais le vrai référentiel
`Service` que the-client utilise ailleurs n'a que 6 valeurs — dérive déjà
confirmée dans leurs données de seed (`biologie`/`neurologie` sans
`Service` correspondant). Summary/compliance restent de la prose libre,
sans équivalent déterministe.

`TriageEvalPlugin` (même substrat que `MedicalPlugin` : lit un attribut,
applique une règle, écrit attributs/warnings) pose `eval.triage.tag_known`
à partir de `oncology.triage.tag`, comparé (normalisé comme the-client le fait
déjà) à un référentiel configurable (`TRIAGE_KNOWN_SERVICES`, défaut = les
6 vraies valeurs the-client — pas une liste vide, qui ferait échouer tous les
tags). Câblé dans `crates/kernel` comme les deux autres plugins
(`ENABLED_PLUGINS`). 7 + 2 tests unitaires.

**Vérifié en conditions réelles (2026-08-17, même jour).** the-client a câblé
`oncology.triage.tag` (`AgentOrchestrator.RunAsync`, commit `8a47b80`),
reconstruit sa stack depuis `dev` (`b422db3`), et testé un vrai appel
("chute à vélo, genou gonflé" → tag `traumatologie`) — **un vrai défaut
déjà en base, pas un cas fabriqué pour l'occasion** (`traumatologie` ne
matche aucun des 6 `Service` connus). Résultat exact : `eval.triage.tag_known
= "false"` + `plugin.warning` explicite dans `extra_attributes`,
`spans_with_warnings` incrémenté sur `/metrics/summary`, coexistant
proprement avec le warning HITL déjà présent sur le même span. Détecté du
premier coup, sans ajustement après coup.

**Volet 2 (juge sémantique summary/compliance) — tranché, sans code côté
Venice.** Un juge LLM ne rentre pas dans le contrat de plugin actuel
(synchrone, sans I/O, borné à 100ms dans `crates/plugin-sink`) — première
piste envisagée avec l'utilisateur, un binaire séparé (`eval-worker`)
relisant transcripts/sorties dans ClickHouse, **écartée** : irait à
l'encontre de la politique PII déjà posée (`docs/interfaces/clickhouse-schema.md`,
attributs sensibles opt-in/désactivés par défaut) — le cas réel
`ComplianceAgent` traite nom/date de naissance/NIR/statut VIH en clair,
faire transiter et stocker ce texte dans Venice (même 90 jours de
rétention) aurait été un vrai risque de conformité, pas théorique.
**Décision retenue** : le jugement tourne côté client (the-client ou tout futur
client), avec son propre texte/sa propre clé API, jamais transmis à
Venice — seul le verdict structuré (`eval.summary.*`/`eval.compliance.*`,
typé, jamais de texte libre) est posté en attribut, absorbé par
`extra_attributes` exactement comme `oncology.*` aujourd'hui. Généralise
mieux qu'un worker centralisé : zéro couplage Venice à un fournisseur LLM
ou un format par client. Détail complet dans
`docs/interfaces/triage-eval-plugin.md`.

## Renommage en Venice (2026-08-17)

Nom définitif choisi avec l'utilisateur avant le chantier cloud : Venice —
ville connue pour ses canaux, cohérente avec l'architecture réelle du
kernel (pipeline principal + plugins qui se greffent dessus sans le
bloquer, comme un réseau de canaux interconnectés plutôt qu'un canal
unique). Logo retenu après comparaison de deux propositions
(`UI/assets/logos/logo_venice_v{1,2}.png`) : v1, dont le canal principal
dessine un V — silhouette plus nette à petite taille (favicon) que le S de
v2, qui référence pourtant plus fidèlement le tracé du Grand Canal.

Portée du renommage : tous les identifiants fonctionnels (conteneurs/
réseau/images Docker dans `crates/orchestrator`, container ClickHouse dev)
et toute la prose (`CLAUDE.md`, `README.md`, `docs/`). Aucun crate n'était
nommé "trellis" littéralement, pas de renommage de package Cargo
nécessaire. **Volontairement pas fait** : renommage du repo GitLab
(casserait le remote `origin` que la session the-client utilise déjà) et du
répertoire local — reportés avec l'accord explicite de l'utilisateur, notés
en mémoire pour ne pas être oubliés d'une session à l'autre.

## Interface terminal (`crates/tui`) — fait (2026-08-17)

Avant le chantier cloud : un front demandé par l'utilisateur, tranché en
TUI plutôt qu'une SPA web (pas de nouveau toolchain JS/Node à construire
juste avant le déploiement) ou du Rust/WASM (pas de bénéfice d'apprentissage
"infra" ici, contrairement à `crates/orchestrator` — un choix de framework
front, pas un concept système). `ratatui` 0.30.2 + `crossterm` 0.29.0,
versions réelles résolues via `cargo search`/`cargo info`, pas devinées.

Périmètre v1 tranché avec l'utilisateur : miroir strict des 3 endpoints
`query-api`, rien de nouveau côté API. `crates/tui` réutilise directement
`query_api::dto` (`SpanDto`/`TraceSummaryDto`/`MetricsSummaryDto`) en leur
ajoutant `Deserialize` (+`PartialEq` sur `SpanDto` pour les tests) — une
seule définition du format JSON partagée entre le serveur qui l'émet et le
client qui le relit, pas une deuxième copie qui pourrait diverger.

Reconstruction de l'arbre de spans faite côté client (`app::span_tree`),
exactement comme `docs/interfaces/query-api.md` le prescrit déjà pour tout
consommateur de `GET /traces/{trace_id}` (liste plate, pas un JSON
imbriqué) — gérée avec garde anti-cycle (spans visités trackés) plutôt que
de faire confiance à la forme des données, même prudence que le serveur qui
ne valide pas non plus un arbre à racine unique.

**Intro stylisée ajoutée après premier retour utilisateur** ("très
minimaliste") : écran de démarrage (passable sur n'importe quelle touche),
logo calculé par arithmétique ligne/colonne plutôt que tapé à la main en
ASCII art (garantit la symétrie quelle que soit la hauteur, pas de risque
de désalignement à l'œil). Couleur teal (`Color::Rgb`) approximant celle du
vrai logo, appliquée aussi aux bordures/titres de toutes les vues
(`ui::venice_block`) pour une identité visuelle cohérente, pas seulement
l'écran d'intro.

**Deuxième retour** ("un peu plus long", "reproduis le logo à l'identique")
: durée portée à 4s ; `ui::venice_glyph_lines` (juste le V) remplacé par
`ui::venice_badge_lines`, qui recompose les mêmes éléments que le vrai
logo — anneau circulaire (équation d'ellipse par ligne, corrigée de
l'aspect ratio des caractères terminal ~2:1 pour ne pas rendre un ovale),
4 nœuds aux coins, un lattice de canaux fins avec nœuds circulaires en
arrière-plan, le V en premier plan avec sa petite queue/nœud au point bas
— composé en couches sur un `Canvas` (grille de caractères) plutôt qu'un
seul motif calculé d'un coup. **Précision honnête donnée à l'utilisateur** :
"à l'identique" au pixel près n'a pas vraiment de sens ici — le lattice du
PNG source est un tracé organique généré par DALL-E, pas une forme
paramétrique reproductible exactement en ASCII ; ce qui est livré est une
interprétation stylisée fidèle à la composition (mêmes éléments, mêmes
proportions relatives), pas une copie pixel par pixel.

Vérifié à quatre niveaux : 5 tests unitaires (`span_tree`/`humanize_ago`,
y compris un cas de cycle à 2 nœuds et une référence de parent hors trace)
sans terminal ; 4 tests d'intégration `--ignored` contre le vrai
`query-api` déjà en service avec de vraies données the-client (5 traces, 3
kinds, 7 `spans_with_warnings` — mêmes chiffres que la vérification the-client
plus tôt dans la session) ; un vrai lancement du binaire dans un
pseudo-terminal confirmant un cycle démarrage/arrêt propre ; et **le rendu
visuel réel vérifié pour de vrai** (correction d'une limite annoncée trop
tôt) — pseudo-terminal avec taille explicite (`TIOCSWINSZ`) + émulation
d'écran via `pyte` (Python) pour reconstruire ce qui s'affiche réellement,
pas juste le flux ANSI brut : le glyphe V symétrique et centré, les 3 vues
avec bordures/tabs qui s'affichent correctement, les vraies traces/coûts/
warnings the-client visibles à l'écran.

**Troisième retour** ("y'a que ascii art ?") : question posée à
l'utilisateur plutôt que tranchée seule — vrai choix technique
(`ratatui-image` 11.0.6, vérifié réel via `cargo info`) entre rester en
ASCII calculé, passer en half-blocks Unicode (l'image réelle, encodée en
blocs de couleur, aucune dépendance système), ou les protocoles graphiques
natifs (Kitty/iTerm2/Sixel, quasi pixel-parfait mais nécessite `chafa`
— **vérifié absent de cette machine**, `pkg-config --exists chafa` échoue
— cohérent avec le principe déjà appliqué ailleurs dans ce projet de zéro
dépendance système à la compilation). Utilisateur a choisi half-blocks.

`crates/tui/src/logo.rs` charge le vrai PNG (`include_bytes!`, pas un
chemin runtime — le logo doit s'afficher peu importe le répertoire de
lancement du binaire), encodé via `Picker::halfblocks()` forcé
explicitement (pas d'auto-détection sixel/kitty/iterm2).
`default-features = false` sur `ratatui-image` pour exclure `chafa-dyn`.

**Vrai bug trouvé en vérifiant, pas juste en lisant la doc** : le mapping
pixel-à-cellule natif de l'image (1254×1254px) donne ~126×63 cellules
terminal — bien plus grand que la plupart des terminaux, ce qui effondrait
silencieusement la zone de layout du splash à une hauteur nulle (rien ne
s'affichait, ni l'image ni l'ASCII de repli). Diagnostiqué via un
`eprintln!` temporaire capturé sur un canal stderr séparé du pty (le
premier essai de debug, stderr mélangé au pty puis `terminate()` immédiat,
n'a rien montré — le process n'avait pas eu le temps d'atteindre un point
de flush). Corrigé en ciblant une taille d'affichage fixe et raisonnable
(40×20 cellules) plutôt que la résolution native, `Resize::Fit` réduisant
l'image dans cette cible plutôt que de tenter du 1:1 pixel-parfait.

Repli en cascade si le chargement échoue à n'importe quelle étape
(décodage, encodage) : `Option<Logo>` — `None` fait retomber sur le badge
ASCII calculé plutôt que de faire planter tout le TUI pour un logo qui n'a
pas pu charger.

Vérifié visuellement (même méthode pseudo-terminal + `pyte`) : le vrai
logo s'affiche, correctement dimensionné et centré, structure reconnaissable
(anneau, V) une fois le bug de taille corrigé.

**Quatrième retour, un vrai bug cette fois** ("le logo n'apparaît même
pas") : la taille cible 40×20 était fixe, pas adaptée à la taille réelle
du terminal — sur un terminal standard **80×24** (la taille par défaut la
plus courante, pas un cas extrême), 20 lignes d'image + 5 de légende
dépassaient les 24 lignes disponibles, donc rien ne s'affichait. Vérifié
en le reproduisant délibérément à 80×24 (pas juste supposé) avant de
corriger. `logo::target_size` calcule maintenant une taille qui tient
compte de la vraie taille du terminal (`terminal.size()?`, appelé avant le
chargement puisqu'un `Protocol` ne se redimensionne pas après coup),
plafonnée à 40×20 sur un grand terminal, réduite sur un petit. 3 tests
unitaires (tient dans un 80×24, plafonne sur un très grand terminal, jamais
de dimension nulle sur un très petit). Revérifié à 80×24 : le logo
s'affiche correctement.

**Cinquième retour** ("fond plat, effet pixelisé, image trop petite") :
deux vrais leviers trouvés en lisant le vrai code source du crate
(`picker.rs`), pas la doc résumée — `chafa` (déjà écarté) ne conditionne
en réalité pas du tout le support Sixel/Kitty/iTerm2, ces protocoles sont
compilés inconditionnellement. `Picker::halfblocks()` forcé explicitement
remplacé par `Picker::from_query_stdio()` : interroge le vrai terminal
(séquences d'échappement de capacité, lues sur stdin) et choisit le
meilleur protocole qu'il supporte réellement — rendu quasi pixel-parfait
sur Kitty/WezTerm/iTerm2/terminaux Sixel, repli sur half-blocks seulement
si rien ne répond mieux. Doit tourner après `ratatui::init()` mais avant
la boucle d'événements (contrainte documentée dans le crate lui-même,
déjà respectée par l'emplacement d'appel dans `main.rs`).

Deuxième levier, indépendant du protocole : la vraie image a une marge
blanche mesurée (pas devinée, script Python dédié) — le contenu occupe
les lignes ~97–1136 et colonnes ~109–1143 d'une toile 1254×1254, environ
17% de bordure blanche de chaque côté. `logo::crop_to_content` recadre
avant l'encodage, pour que le budget de cellules limité (`target_size`)
représente du vrai détail plutôt que du blanc — surtout sensible en
half-blocks. 2 tests unitaires sur une image synthétique (pas l'asset réel,
pour ne pas dépendre de ses dimensions).

**Limite de vérification honnête** : impossible de vérifier ici le rendu
Kitty/Sixel réel — aucun terminal capable de ces protocoles n'est
disponible dans cet environnement de test (le pseudo-terminal Python
`pyte` ne les implémente pas complètement ; un artefact de texte visible
lié à la requête de capacité Kitty est apparu dans ce test synthétique,
probablement une limite de `pyte` plutôt qu'un vrai bug, mais pas confirmé
sur un vrai terminal). Le repli half-blocks (toujours vérifiable) reste
correct et amélioré par le recadrage. À confirmer par l'utilisateur sur
son propre terminal, en particulier s'il utilise Kitty/WezTerm/iTerm2.

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

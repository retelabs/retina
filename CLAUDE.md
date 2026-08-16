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

Pas encore fait : construction d'image via l'API Docker, et un deuxième
chantier envisagé pour le même objectif d'apprentissage — une méthode de
calcul de coût réel.

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

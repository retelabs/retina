# docker-engine-api — contrat du control plane "maison" (`crates/orchestrator`)

- Contexte : dossier section 5 laissait "GCP vs Azure" ouvert. Décision prise
  avec l'utilisateur (2026-08-15) de mettre cette question de côté au profit
  d'un objectif d'apprentissage — coder, en Rust et en direct, les briques
  d'un service managé (ici : un control plane qui fait converger des
  conteneurs vers un état voulu), avant de choisir un hébergeur. Le choix du
  cloud reste une question séparée, pas tranchée ici.
- Source faisant autorité : code source réel de `bollard` 0.21.0 et
  `bollard-stubs` 1.53.1-rc (`~/.cargo/registry/src/.../bollard-0.21.0/src/container.rs`,
  `bollard-stubs-.../src/models.rs`, `.../src/query_parameters.rs`) — pas
  seulement docs.rs. Une première passe par `WebFetch` sur docs.rs a donné
  les bonnes lignes générales (signatures `create_container`/`start_container`/...)
  mais pas les noms de champs exacts des structs (`ContainerCreateBody`,
  `HealthConfig`, `ContainerState`) ; ceux-là ont demandé de lire le code
  généré depuis le registre Cargo local, exactement comme pour le code
  généré par `tonic` à l'étape 2 du kernel.
- Version d'API Docker Engine ciblée : schéma `1.52`/`1.53.1-rc` (nom du
  paquet `bollard-stubs`), pas épinglée dans `vendor/` (contrairement à
  semconv-genai/opentelemetry-proto) — `bollard` fixe déjà la version dans
  `Cargo.lock`, un deuxième pin serait redondant.
- Date de vérification : 2026-08-15 (mise à jour le même jour : extension à
  `kernel`/`query-api`)

## Ce que `crates/orchestrator` fait aujourd'hui

Les 3 services du dossier étape 6 (`ClickHouse`, `kernel`, `query-api`),
déployés dans l'ordre de leurs dépendances. `crates/orchestrator/src/main.rs`
n'est pas encore une API HTTP — un binaire qui prouve le principe contre le
vrai démon Docker local (`--keep-running` pour laisser la pile debout et
taper dessus manuellement, comme `scripts/demo.sh --keep-running`).

```rust
pub enum ImageSource { Registry, Local } // pull si absent, vs doit déjà exister

pub struct PortSpec { pub container_port: u16, pub host_port: u16 }

pub struct ManagedService {
    pub name: String,
    pub image: String,
    pub image_source: ImageSource,
    pub env: Vec<String>,
    pub healthcheck: Option<HealthCheckSpec>,
    pub ports: Vec<PortSpec>,
    pub depends_on: Vec<String>,
}

pub async fn ensure_running(docker: &Docker, network: &str, service: &ManagedService) -> Result<(), OrchestratorError>;
pub async fn ensure_network(docker: &Docker, name: &str) -> Result<(), OrchestratorError>;
pub async fn deploy_all(docker: &Docker, network: &str, services: &[ManagedService]) -> Result<(), OrchestratorError>;
pub async fn teardown_all(docker: &Docker, services: &[ManagedService]) -> Result<(), OrchestratorError>;
pub async fn status(docker: &Docker, name: &str) -> Result<ServiceStatus, OrchestratorError>;
pub async fn wait_healthy(docker: &Docker, name: &str, timeout: Duration) -> Result<ServiceStatus, OrchestratorError>;
pub async fn teardown(docker: &Docker, name: &str) -> Result<(), OrchestratorError>;
```

`ensure_running`/`ensure_network`/`teardown` sont **idempotents** —
propriété centrale d'un control plane réel (une boucle de réconciliation
doit pouvoir rappeler "convergence vers cet état" sans se soucier de l'état
de départ, y compris sous appels concurrents — voir la trouvaille de race
plus bas). `deploy_all` trie `services` par dépendances (tri topologique,
algorithme de Kahn, déterministe : les égalités sont départagées par
l'ordre d'entrée, pas par l'itération d'une `HashMap`) puis, pour chacun
dans cet ordre, `ensure_running` + `wait_healthy` avant de passer au
suivant — au moment où un service dépendant démarre, tout ce dont il
dépend est déjà sain. C'est exactement ce que `depends_on: condition:
service_healthy` donne sous `docker compose`, reconstruit depuis l'API.

**Réseau partagé, pas le réseau `bridge` par défaut** : les conteneurs sur
le réseau `bridge` par défaut de Docker ne se résolvent *pas* par nom (seule
une résolution DNS par IP existe) — seul un réseau défini par l'utilisateur
donne la résolution par nom. `deploy_all` crée un tel réseau
(`ensure_network`) et y attache chaque conteneur (`HostConfig.network_mode`)
pour que `kernel`/`query-api` joignent `CLICKHOUSE_URL=http://<nom du
conteneur clickhouse>:8123` — vérifié en poussant un vrai rejeu fraudos par
gRPC à travers la pile déployée par l'orchestrateur puis en relisant les
métriques via `query-api`, pas seulement en lisant `docker inspect`.

`kernel`/`query-api` utilisent `ImageSource::Local` (images déjà construites
via `docker/kernel.Dockerfile`/`docker/query-api.Dockerfile`, le même chemin
que `docker-compose.stack.yml`) — ce control plane ne construit pas encore
d'image lui-même, voir plus bas.

## Trois trouvailles réelles, pas devinables depuis un résumé de doc

1. **`create_image` (pull d'image) retourne un `Stream`, pas un `Future`** —
   `docker.create_image(options, None, None)` ne tire rien tant que le
   stream n'est pas consommé (`while let Some(event) = pull.next().await`).
   Appeler la fonction sans le faire compile et ne fait rien silencieusement.
2. **`ContainerCreateBody`/`CreateContainerOptions`, pas `Config<String>`** —
   les noms de types de bollard ont changé entre versions (générés depuis le
   schéma OpenAPI de Docker plutôt qu'écrits à la main). Coder depuis un
   souvenir d'une version antérieure de la crate aurait produit du code qui
   ne compile pas.
3. **Distinguer "jamais créé" de "existe mais arrêté"** demande
   `list_containers(all: true)` filtré par nom (`^/<name>$`), pas
   `inspect_container` seul — `inspect_container` sur un nom absent renvoie
   une erreur HTTP qu'il faut interpréter, alors que `list_containers` donne
   directement une liste vide ou non, plus simple à traiter sans dépendre du
   code d'erreur exact.
4. **`ensure_network` avait une vraie race** (trouvée par la propre suite de
   tests du crate, pas construite exprès) : vérifier-puis-créer
   (`list_networks` puis `create_network`) laisse une fenêtre où deux
   appelants concurrents voient tous les deux "n'existe pas" et essaient
   tous les deux de créer — le perdant reçoit un `409`. Corrigé en traitant
   `409 (already exists)` comme un succès, même logique que le `304 (already
   started)` déjà géré pour `start_container`. Reproduit à volonté :
   `cargo test -p orchestrator -- --ignored` lance deux tests qui appellent
   `ensure_network` sur le même nom en parallèle par défaut.

## Santé d'un conteneur (`ServiceStatus`)

Reflète `ContainerState.health.status` (`HealthStatusEnum` : `NONE`,
`STARTING`, `HEALTHY`, `UNHEALTHY`, plus `EMPTY` pour une chaîne vide côté
API) tel que Docker le calcule lui-même à partir du `Healthcheck` déclaré à
la création du conteneur — le même mécanisme que
`docker/docker-compose.stack.yml` utilise déjà pour `clickhouse`
(`wget --spider -q http://localhost:8123/ping`, forme `["CMD", ...]` pas
`CMD-SHELL`, reproduite à l'identique dans `ManagedService`). `wait_healthy`
est une boucle de réconciliation en miniature : observer l'état, comparer à
l'état voulu, attendre, recommencer — le même principe qu'un vrai
contrôleur Kubernetes/Nomad, réduit à sa version la plus simple.

## Pas encore fait, explicitement pas dans ce V0

- **API HTTP** (`POST /deploy`, `GET /status`, `POST /teardown`) — le
  binaire prouve juste le client Docker pour l'instant.
- **Construction d'image via l'API** (`/build`, contexte de build en tar
  streamé) — `kernel`/`query-api` doivent être construits par le chemin
  existant (`docker/docker-compose.stack.yml` ou `docker build` direct)
  avant que `deploy_all` puisse les déployer ; `ImageSource::Local` échoue
  avec une erreur explicite (`MissingLocalImage`) plutôt que de tenter un
  pull qui échouerait de façon confuse.
- **Modèle de coût** — deuxième chantier envisagé pour le même objectif
  d'apprentissage, pas commencé.

## Vérifié comment (mise à jour kernel/query-api)

Bout en bout réel, pas seulement `docker inspect` : `cargo run -p
orchestrator -- --keep-running` déploie les 3 services, puis un vrai rejeu
gRPC (`fraudos-replay`) contre `localhost:4317` et une vraie requête HTTP
authentifiée contre `localhost:8080/metrics/summary` confirment que
`kernel` a réellement écrit dans ClickHouse (via le nom de conteneur résolu
sur le réseau partagé) et que `query-api` relit les mêmes données. Plus les
tests automatisés : 4 tests unitaires pour `topological_order` (ordre
correct, dépendance inconnue rejetée, cycle rejeté, déterminisme) sans
Docker, et 2 tests d'intégration `--ignored` contre un vrai démon
(idempotence sur un seul service, `deploy_all`/`teardown_all` sur une
chaîne de dépendance à deux services) — les deux passés à plusieurs
reprises pour confirmer que le fix de la race n'était pas un coup de
chance.

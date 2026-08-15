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
- Date de vérification : 2026-08-15

## Ce que `crates/orchestrator` fait aujourd'hui (V0)

Un seul service géré pour l'instant (`ClickHouse`, le plus simple des trois
— pas de build d'image maison contrairement à `kernel`/`query-api`) : preuve
du cycle de vie complet avant d'étendre. `crates/orchestrator/src/main.rs`
n'est pas encore une API HTTP — un binaire qui prouve le principe contre le
vrai démon Docker local.

```rust
pub struct ManagedService {
    pub name: String,
    pub image: String,
    pub env: Vec<String>,
    pub healthcheck: Option<HealthCheckSpec>,
}

pub async fn ensure_running(docker: &Docker, service: &ManagedService) -> Result<(), OrchestratorError>;
pub async fn status(docker: &Docker, name: &str) -> Result<ServiceStatus, OrchestratorError>;
pub async fn wait_healthy(docker: &Docker, name: &str, timeout: Duration) -> Result<ServiceStatus, OrchestratorError>;
pub async fn teardown(docker: &Docker, name: &str) -> Result<(), OrchestratorError>;
```

`ensure_running`/`teardown` sont **idempotents** — propriété centrale d'un
control plane réel (une boucle de réconciliation doit pouvoir rappeler
"convergence vers cet état" sans se soucier de l'état de départ). Vérifié en
rappelant les deux à la suite contre un vrai conteneur
(`crates/orchestrator/tests/integration.rs`, `--ignored`).

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
- **`kernel`/`query-api`** — ont une dépendance d'ordre en plus (attendre
  `clickhouse` healthy avant de démarrer, comme `docker-compose.stack.yml`
  l'exprime via `depends_on: condition: service_healthy`) ; `kernel`/
  `query-api` ont aussi besoin que leurs images (`docker-kernel:latest`,
  `docker-query-api:latest`) soient déjà construites — construire l'image
  via l'API Docker (`/build`, contexte de build en tar streamé) est un vrai
  morceau séparé, pas encore abordé.
- **Publication de ports** (`HostConfig.port_bindings`) — pas nécessaire
  pour ClickHouse seul (le healthcheck tourne dans l'espace réseau du
  conteneur, pas besoin d'exposer 8123 à l'hôte pour ça) ; redeviendra
  pertinent pour `kernel`/`query-api`, qui doivent être joignables depuis
  l'extérieur.
- **Modèle de coût** — deuxième chantier envisagé pour le même objectif
  d'apprentissage, pas commencé.

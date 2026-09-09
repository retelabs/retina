# kernel-auth — authentification sur les surfaces HTTP/gRPC exposées

- Source faisant autorité : deux contrats vérifiés séparément avant d'écrire
  le code, parce que chaque surface a son propre mécanisme d'auth natif —
  pas un contrat externe unique.
  - Convention de header côté client OTLP : spec OpenTelemetry
    (https://opentelemetry.io/docs/specs/otel/protocol/exporter/), vérifiée
    le 2026-08-15. `OTEL_EXPORTER_OTLP_HEADERS` (et les variantes
    `_TRACES_HEADERS`) acceptent des paires `key=value` façon W3C Baggage,
    ex. `OTEL_EXPORTER_OTLP_HEADERS="authorization=Bearer <token>"` — un vrai
    SDK OTel n'a donc besoin d'aucun code custom pour s'authentifier contre
    notre receiver, juste cette variable d'environnement.
  - API `tonic::service::Interceptor` et `TraceServiceServer::with_interceptor`
    : vérifiées contre le code généré réel (`tonic`/`tonic-prost` 0.14.6,
    inspecté dans `target/.../out/opentelemetry.proto.collector.trace.v1.rs`
    après un vrai `cargo build`, pas depuis la doc seule) :
    ```rust
    pub trait Interceptor {
        fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status>;
    }
    impl<T> TraceServiceServer<T> {
        pub fn with_interceptor<F>(inner: T, interceptor: F) -> InterceptedService<Self, F>
        where F: tonic::service::Interceptor;
    }
    ```
  - API `axum::middleware::from_fn_with_state` (axum 0.8.9) : vérifiée contre
    docs.rs pour cette version exacte — signature `FnMut(State<S>, Request,
    Next) -> impl Future<Output = impl IntoResponse>`, appliqué au routeur via
    `.route_layer(...)`.
- Date de vérification : 2026-08-15 (mise à jour 2026-08-16 : étendu à
  `crates/orchestrator`, même mécanisme axum que `query-api`, rien de
  nouveau à vérifier)

## Décision : secret partagé statique, pas de JWT/OAuth

Le dossier exclut explicitement le multi-tenant du MVP (section 4). Un jeton
partagé statique par surface est proportionné à un kernel mono-tenant ; une
vraie infra d'identité (JWT, OAuth, rotation de clés) serait une dépendance
supplémentaire à opérer pour un besoin qui n'existe pas encore. Décision
prise avec l'utilisateur — à revisiter si/quand le multi-tenant est tranché.

**Deux secrets distincts, pas un seul**, parce que l'ingestion (écriture) et
la query API (lecture) ne sont pas le même niveau de confiance : un service
qui pousse des spans n'a pas besoin de pouvoir relire toutes les données
persistées, et inversement.

- `KERNEL_API_KEY` — protège `crates/otlp-receiver` (le service gRPC
  `TraceService.Export`, écoute par défaut sur `:4317`).
- `QUERY_API_KEY` — protège `crates/query-api` (les 3 endpoints HTTP,
  `:8080`).
- `ORCHESTRATOR_API_KEY` — protège `crates/orchestrator` (`POST /deploy`,
  `GET /status`, `POST /teardown`, `:9000` par défaut sur `127.0.0.1`
  uniquement). Même mécanisme qu'un troisième secret pour la même raison
  que les deux premiers : cette surface peut arrêter des conteneurs, un
  niveau de confiance encore différent de lire (`query-api`) ou écrire des
  spans (`kernel`).

## Mécanisme

Les deux surfaces attendent le même header, `authorization: Bearer <token>`
— gRPC via les métadonnées de la requête, HTTP via l'en-tête standard. Même
convention des deux côtés pour que la configuration reste symétrique
(`OTEL_EXPORTER_OTLP_HEADERS` côté client OTLP, header HTTP classique côté
client query-api).

- **Échec fermé** : si `KERNEL_API_KEY`/`QUERY_API_KEY`/`ORCHESTRATOR_API_KEY`
  n'est pas défini au démarrage, le binaire concerné refuse de démarrer
  (`expect`/panic explicite) plutôt que de tourner sans authentification. Un
  contrôle de sécurité qu'on peut oublier d'activer par défaut n'en est pas
  un.
- **Comparaison en temps constant** : la comparaison du jeton reçu contre le
  jeton attendu ne doit pas fuiter d'information de timing (byte par byte
  avec early-return serait une fuite classique côté canal auxiliaire) — une
  petite fonction `constant_time_eq` locale à chaque crate plutôt qu'une
  dépendance externe pour une seule comparaison.
- **Rejet** : `tonic::Status::unauthenticated(...)` côté gRPC,
  `StatusCode::UNAUTHORIZED` (401) côté HTTP.

## Plusieurs jetons valides simultanément — fait (2026-09-09)

Deuxième client réel (second-client, en plus de the-client/`fraudos-replay`) — même
raisonnement que `ENABLED_PLUGINS`/`TRIAGE_KNOWN_SERVICES` ailleurs dans ce
kernel : généraliser quand un deuxième cas réel arrive, pas avant.
`KERNEL_API_KEY`/`QUERY_API_KEY` restent les jetons **requis** (échec fermé
inchangé) ; `KERNEL_API_KEYS_EXTRA`/`QUERY_API_KEYS_EXTRA` (optionnels,
séparés par des virgules) ajoutent des jetons valides supplémentaires — un
par client, révocable indépendamment sans casser les autres. Toujours pas
d'autorisation fine (voir ci-dessous) : n'importe quel jeton de la liste
donne le même accès complet à la surface, juste une identité de credential
distincte, pas un scope différent.

`ApiKeyInterceptor`/`ExpectedBearer` comparent maintenant contre un
`Vec<String>` plutôt qu'un `String` unique — chaque jeton attendu comparé en
temps constant, **sans court-circuiter sur le premier qui matche** (le
`fold` parcourt toute la liste même après un succès), pour ne pas fuiter en
plus quel jeton de la liste a matché.

`crates/orchestrator` volontairement pas étendu — outil interne
(`127.0.0.1` par défaut), aucun deuxième client externe à qui donner un
jeton distinct pour l'instant.

## Ce que ça ne couvre pas

- Rotation de jeton (déploiement sans interruption d'un nouveau jeton qui
  remplace un ancien) — toujours pas nécessaire, `KERNEL_API_KEYS_EXTRA`/
  `QUERY_API_KEYS_EXTRA` couvrent l'ajout, pas le remplacement à chaud.
- Autorisation fine (quel client peut lire quelles traces) — non pertinent
  tant qu'il n'y a qu'un seul tenant.
- Chiffrement du canal (TLS) — hors périmètre de ce contrat, orthogonal à
  l'authentification. Sans TLS, le jeton circule en clair sur le réseau —
  acceptable pour `docker-compose.stack.yml` (dev local/démo, mono-VM,
  trafic inter-conteneurs sur le réseau Docker interne). **Résolu pour le
  déploiement public** (2026-08-20) : `docker/docker-compose.prod.yml` +
  `docker/Caddyfile` terminent TLS (Let's Encrypt automatique) devant
  `kernel`/`query-api`, qui ne publient plus de port sur l'hôte — voir
  `docs/interfaces/caddy-reverse-proxy.md`.

## Vérifié comment

`cargo test --workspace` (unit) + `cargo test -p query-api -- --ignored`
contre un vrai ClickHouse (`scripts/dev-clickhouse.sh up`) couvrent le
rejet/l'acceptation via `tower::ServiceExt::oneshot`. Bout en bout réel :
`scripts/dev-stack.sh up` (jetons dev fixes dans
`docker/docker-compose.stack.yml`, même logique que `CLICKHOUSE_PASSWORD:
dev` déjà en place) puis `scripts/demo.sh`, qui envoie désormais le header
`Authorization` sur chaque appel `curl` et chaque rejeu
`fraudos-replay`/`oncology-replay`.

**`crates/orchestrator` (2026-08-16)** : `cargo test -p orchestrator --
--ignored` (`tests/api_integration.rs`) couvre le même rejet/acceptation
contre le vrai `Router`. Bout en bout réel : `cargo run -p orchestrator`
lancé sans `ORCHESTRATOR_API_KEY` panique avant toute connexion Docker ;
lancé avec, `curl` sans header confirmé `401`, puis avec le bon header un
cycle `/deploy`→`/status`→`/teardown` complet, un vrai rejeu gRPC
(`fraudos-replay`) et une vraie requête `query-api` confirmant que la pile
déployée derrière l'auth fonctionne réellement, pas seulement
`status: "Healthy"`.

**Jetons multiples (2026-09-09)** : nouveaux tests unitaires
`crates/otlp-receiver/src/auth.rs` (accepte n'importe quel jeton d'un
ensemble, rejette un jeton hors ensemble) et
`crates/kernel/src/main.rs`/`crates/query-api/src/main.rs`
(`parse_extra_tokens` — non défini, chaîne vide, virgules avec espaces).
Nouveau test d'intégration `query-api`
(`a_second_client_with_its_own_token_can_also_authenticate`, contre un vrai
ClickHouse) : deux jetons distincts passés à `build_app`, les deux acceptés
indépendamment, un troisième jeton toujours rejeté. `cargo test --workspace`
et `cargo clippy --workspace -- -D warnings` au vert après le changement.

# query-api — contrat de l'API de requête minimale (étape 4 du kernel)

- Source faisant autorité : conception propre à ce projet (pas de spec
  externe) — mais c'est un vrai contrat d'interopérabilité (consommé par de
  futurs dashboards/couche d'analyse, section 2.3), documenté avec la même
  rigueur que les frontières externes.
- Driver HTTP : `axum` 0.8.9 (déjà résolu dans le workspace via `tonic`).
  Point vérifié avant de coder : **axum 0.8 utilise `{param}` dans les routes,
  pas `:param`** — l'ancienne syntaxe *panique* au démarrage du routeur au
  lieu d'échouer silencieusement (changelog officiel, vérifié le 2026-08-14).
- Date de vérification : 2026-08-14
- Portée : 2-3 endpoints minimaux (dossier section 2.2 étape 4), pas de
  dashboard riche, pas d'authentification (MVP local).

## Endpoints

### `GET /traces?limit=N`

"Lister les traces récentes." Il n'existe pas de table `traces` — une trace
est dérivée à la volée en groupant `spans` par `trace_id`
(`min(start_time)`, `max(end_time)`, `count()`), triée par début décroissant.
`limit` par défaut 50, plafonné à 500 côté serveur (pas de politique de
rétention pour borner le scan autrement — dossier section 4 — donc ce plafond
est la seule protection contre un `?limit=` absurde).

Réponse : tableau de
```json
{ "trace_id": "<32 hex>", "span_count": 3, "start_time_unix_nano": 0, "end_time_unix_nano": 0 }
```

### `GET /traces/{trace_id}`

"Récupérer l'arbre d'une trace." `trace_id` en hex minuscule 32 caractères —
même encodage que `traceId` en OTLP/JSON (docs/interfaces/otlp-ingestion.md),
pas une convention inventée ici. Validé (longueur, non-tout-zéro) via
`kernel_model::TraceId` avant toute requête ClickHouse → `400` si invalide.
`404` si aucun span pour ce `trace_id`.

**Décision** : retourne une **liste plate** de spans (triée par
`start_time`), pas un arbre JSON imbriqué. Chaque span porte déjà
`parent_span_id` — reconstruire l'arbre côté client est suffisant pour "pas
de dashboard riche nécessaire" (dossier section 2.2). Construire et valider
une vraie structure imbriquée côté serveur (racines multiples, parents
orphelins, cycles sur données malformées) est plus que ce que le MVP demande.

### `GET /metrics/summary`

"Agréger quelques métriques de base." Compte de spans et totaux de tokens
par `kind` (`model_call`/`tool_call`/`agent_run`). Pas de filtre temporel au
MVP — sans politique de rétention, un filtre par intervalle serait cosmétique
plutôt que réellement nécessaire au volume attendu.

## Décisions de binding ClickHouse (suite de l'incertitude laissée à l'étape 3)

`docs/interfaces/clickhouse-schema.md` notait que binder un `[u8; 16]`
directement dans un `?` échoue (le driver le sérialise en `Tuple`, que
ClickHouse refuse de comparer à `FixedString`). Vérifié contre un vrai
serveur avant d'écrire `queries.rs` : la requête
`WHERE trace_id = unhex(?)` en bindant la **représentation hex (`String`)**
plutôt que les octets bruts fonctionne — `unhex()` fait la conversion côté
serveur, et binder un `String` est un cas standard du driver, pas un type
exotique. C'est l'approche retenue pour tout futur lookup paramétré par
`trace_id`/`span_id`.

## Ignoré volontairement pour le MVP

- Authentification/autorisation — API locale de validation, pas de surface
  d'exposition prod encore décidée (dossier section 5/6).
- Pagination au-delà d'un `limit` simple sur `/traces` (pas de curseur/offset).
- Filtre temporel sur `/metrics/summary`.
- Endpoint de recherche/filtre par attribut — hors des "2-3 endpoints
  minimaux" demandés.

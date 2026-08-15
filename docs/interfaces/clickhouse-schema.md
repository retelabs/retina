# clickhouse-schema — driver Rust et schéma de stockage (étape 3 du kernel)

- Sources faisant autorité :
  - driver : https://docs.rs/clickhouse (crate `clickhouse`, v0.15.1, vérifié via docs.rs)
  - schéma : conception propre à ce projet (pas une spec externe), mais documentée
    ici avec la même rigueur car c'est la frontière kernel-model ↔ stockage
- Date de vérification : 2026-08-14
- Portée : instance ClickHouse unique (dossier section 2.2 étape 3), locale
  pour l'instant (`docker/docker-compose.clickhouse.yml`) — le choix
  prod (auto-hébergé vs BigQuery/ADX, GCP vs Azure) reste une question
  ouverte (dossier section 5), non tranchée ici. Construire contre le
  ClickHouse local ne préjuge pas de ce choix.

## Driver Rust (`clickhouse` crate)

- Transport HTTP (port 8123, celui déjà exposé par `docker-compose.clickhouse.yml`)
  — pas le protocole natif (port 9000).
- Construction : `Client::default().with_url(...).with_user(...).with_password(...).with_database(...)`.
- Une struct de ligne = `#[derive(Row, Serialize, Deserialize)]` + `serde`.
- Insertion : `client.insert::<Row>("table").await?` puis `.write(&row).await?` pour
  chaque ligne, puis **`.end().await?` obligatoire** — *"If `end()` isn't called,
  the `INSERT` is aborted."* Pas de garantie ligne par ligne à l'intérieur d'un
  batch : soit tout le batch est committé (`end()` réussit), soit rien ne l'est.
  **Implication directe** : le `SpanSink` ne peut pas rester "un événement à la
  fois, infaillible" comme le stub `InMemorySink` de l'étape 2 — il doit devenir
  **async, par lot (par requête `Export`), et faillible**.
- Mapping de types (table du driver) :

  | Type ClickHouse | Type Rust |
  |---|---|
  | `(U)Int(8-64)` | `(u)i(8-64)` |
  | `String` | `String` / `&str` |
  | `FixedString(N)` | `[u8; N]` (et `Option<[u8; N]>` pour `Nullable(FixedString(N))`) |
  | `DateTime64(_)` | `i64` (ticks bruts) ou `chrono::DateTime<Utc>` |
  | `Array(_)` | `Vec<_>` |
  | `Map(K, V)` | `HashMap<K, V>` ou `Vec<(K, V)>` |
  | `Nullable(_)` | `Option<_>` |

## Schéma retenu : une seule table `spans` (pas une par type d'événement)

**Pourquoi une table unique plutôt que 3** : l'étape 4 du kernel doit pouvoir
"récupérer l'arbre d'une trace" — un `trace_id` mélange souvent des spans
`ModelCallEvent`/`ToolCallEvent`/`AgentRunEvent` en parent/enfant. Une table
large avec colonnes nullables pour les champs spécifiques à chaque type,
plus une colonne `kind` discriminante, évite une jointure à 3 tables pour
reconstruire un arbre. C'est aussi la pratique standard des exporteurs
ClickHouse OTel-natifs.

```sql
CREATE TABLE spans
(
    trace_id        FixedString(16),
    span_id         FixedString(8),
    parent_span_id  Nullable(FixedString(8)),
    kind            LowCardinality(String), -- 'model_call' | 'tool_call' | 'agent_run'

    start_time      DateTime64(9, 'UTC'),
    end_time        DateTime64(9, 'UTC'),
    status_code     LowCardinality(String), -- 'unset' | 'ok' | 'error'
    status_message  String,
    error_type      Nullable(String),

    operation_name  LowCardinality(String),
    provider_name   LowCardinality(Nullable(String)),
    request_model   Nullable(String),
    response_model  Nullable(String),
    input_tokens                 Nullable(UInt64),
    output_tokens                Nullable(UInt64),
    cache_read_input_tokens      Nullable(UInt64),
    cache_creation_input_tokens  Nullable(UInt64),
    finish_reasons  Array(String),
    conversation_id Nullable(String),

    tool_name        Nullable(String),
    tool_call_id      Nullable(String),
    tool_type         Nullable(String),
    tool_description  Nullable(String),

    agent_invocation_kind  LowCardinality(Nullable(String)), -- 'client' | 'internal'
    agent_name        Nullable(String),
    agent_id          Nullable(String),
    agent_description Nullable(String),
    agent_version     Nullable(String),

    extra_attributes  Map(String, String)
)
ENGINE = MergeTree
PARTITION BY toYYYYMMDD(start_time)
ORDER BY (trace_id, start_time, span_id)
```

- `ORDER BY (trace_id, start_time, span_id)` : optimise "tous les spans d'une
  trace" (étape 4), pas d'index secondaire nécessaire pour ce cas au MVP.
- `PARTITION BY toYYYYMMDD(start_time)` : coûte rien à poser maintenant même si
  "pas de rétention fine au MVP" (dossier section 4) — évite une migration de
  schéma le jour où une politique de rétention par partition est ajoutée.
  **Ce jour est arrivé (2026-08-15)** : un TTL de 90 jours existe maintenant
  sur `start_time`, voir `docs/interfaces/clickhouse-retention.md` — c'est
  précisément ce partitionnement qui le rend peu coûteux (suppression par
  partition entière, pas ligne par ligne).
- `agent_name` est réutilisé entre `ToolCallEvent` (l'agent qui exécute l'outil)
  et `AgentRunEvent` (l'agent lui-même) — même colonne, sémantique cohérente
  dans les deux cas (dossier ne distingue pas les deux).

## `extra_attributes` : `Map(String, String)`, pas `Map(String, AnyValue)`

`AttributeValue` (kernel-model) peut être imbriqué (`Array`, `KeyValueList`),
mais ClickHouse `Map` n'accepte pas de type de valeur récursif pratique ici.
**Décision** : stringifier chaque valeur (nombres/bool en `to_string()`, bytes
en hex, imbriqué en représentation debug lisible mais non ré-analysable).
C'est **avec perte** pour les types imbriqués — acceptable pour le MVP car
les seuls attributs `gen_ai.*` potentiellement imbriqués/complexes
(`gen_ai.tool.call.arguments`/`result`, type `any`) sont `opt_in` et
désactivés par défaut par notre propre politique PII (dossier section 2.1).
À revisiter (colonne `JSON` native ou une vraie sérialisation JSON) si des
attributs imbriqués deviennent courants.

## Conversions à risque (mêmes réflexes que pour l'ingestion)

- `start_time_unix_nano`/`end_time_unix_nano` (`u64`, wire OTLP) →
  `DateTime64(9)` stocké comme `i64` côté driver : conversion `u64 -> i64`
  **vérifiée** (`i64::try_from`), pas un cast nu — sûr jusqu'en 2262, mais le
  cast doit rester explicite et faillible plutôt que supposé toujours correct.
- Persistance **tout ou rien par lot** (voir plus haut) : un span
  structurellement valide qui échoue à la persistance doit être compté dans
  `rejected_spans` de la réponse OTLP (succès partiel), au même titre qu'un
  span malformé — le client OTLP n'a pas à distinguer "rejeté à la validation"
  de "rejeté à l'écriture", les deux veulent dire "pas persisté".

## Ignoré volontairement pour le MVP

- Pas de `ReplacingMergeTree`/déduplication : les doublons de spans que le
  protocole OTLP autorise explicitement (docs/interfaces/otlp-ingestion.md,
  "Known Limitations") ne sont pas dédupliqués au MVP — cohérent avec "pas de
  rétention fine ni de logique avancée" (dossier section 4).
- Pas de table séparée pour les extensions provider (ex. `aws.bedrock.*`) —
  elles vivent dans `extra_attributes` comme le reste, conformément à la
  section 2.1 du dossier ("stockées séparément... pour ne pas forcer une
  normalisation qui perdrait l'info").
- Pas de connexion TLS/rustls — HTTP simple vers le ClickHouse local ; à
  revoir si le stockage prod est distant (question encore ouverte, section 5).

## Erreur trouvée en testant contre un vrai serveur (pas devinable depuis la doc seule)

Le schéma initial utilisait `Nullable(LowCardinality(String))` pour
`provider_name`/`agent_invocation_kind`. ClickHouse 26.7.3 le refuse à la
création de table : *"Nested type LowCardinality(String) cannot be inside
Nullable type (ILLEGAL_TYPE_OF_ARGUMENT)"* — l'ordre correct est
`LowCardinality(Nullable(String))`. Ni la doc du driver Rust ni le dossier ne
mentionnaient cette contrainte ; seul le test d'intégration réel
(`crates/clickhouse-sink/tests/integration.rs`, `--ignored`, contre
`scripts/dev-clickhouse.sh up`) l'a révélée. Rappel que documenter un contrat
depuis la doc ne dispense pas de le vérifier en conditions réelles quand
c'est possible.

Deuxième trouvaille du même test : binder un `[u8; 16]` (trace_id) directement
dans un `?` d'une requête paramétrée sérialise en `Tuple(UInt8, ...)` côté
driver, pas en littéral `FixedString` — ClickHouse répond `NO_COMMON_TYPE` en
comparant à la colonne. Pas encore résolu (contourné en filtrant côté Rust
après `fetch_all`) ; à reprendre précisément quand l'étape 4 (API de requête)
aura besoin de vrais lookups paramétrés par `trace_id`.

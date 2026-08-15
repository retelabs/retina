# otlp-ingestion — contrat du receiver OTLP/traces (étape 2 du kernel)

- Source faisant autorité : https://github.com/open-telemetry/opentelemetry-proto
  (`opentelemetry/proto/collector/trace/v1/trace_service.proto`,
  `opentelemetry/proto/trace/v1/trace.proto`,
  `opentelemetry/proto/common/v1/common.proto`,
  `opentelemetry/proto/resource/v1/resource.proto`,
  `docs/specification.md`)
- Version/commit pinné : tag `v1.11.0` (`790608c4d51e6ffc12210b541e8514cbed9e91a4`)
  — **repo à tags de release réels**, contrairement à `semconv-genai`.
- Date de vérification : 2026-08-14
- Épinglé via : `scripts/pin-otlp-proto.sh v1.11.0` → `vendor/opentelemetry-proto/`
- Scope de ce contrat : uniquement le signal **traces** (dossier section 2.1 : les 3
  événements MVP sont tous des spans, voir `docs/interfaces/semconv-genai.md`) — logs
  et métriques OTLP hors périmètre.
- **Authentification (ajoutée le 2026-08-15)** : `TraceService.Export` exige
  un header de métadonnées gRPC `authorization: Bearer <KERNEL_API_KEY>`,
  sinon `UNAUTHENTICATED`. Convention alignée sur
  `OTEL_EXPORTER_OTLP_HEADERS` d'un vrai SDK OTel — contrat complet dans
  `docs/interfaces/kernel-auth.md`.

## Service gRPC

```
service TraceService {
  rpc Export(ExportTraceServiceRequest) returns (ExportTraceServiceResponse) {}
}
```

- Port par défaut OTLP/gRPC : **4317**.
- `tonic` implémente ce service directement à partir du `.proto` — c'est le point
  d'entrée du receiver de l'étape 2.

## Forme du message reçu (imbrication exacte)

```
ExportTraceServiceRequest
  └─ repeated ResourceSpans
       ├─ resource: Resource { repeated KeyValue attributes, uint32 dropped_attributes_count, repeated EntityRef entity_refs }
       ├─ repeated ScopeSpans
       │    ├─ scope: InstrumentationScope { name, version, repeated KeyValue attributes, dropped_attributes_count }
       │    └─ repeated Span
       └─ schema_url: string
```

Un seul appel `Export` peut donc contenir des spans de plusieurs resources et
plusieurs scopes — la validation/persistance à l'étape 2 doit itérer ces 3 niveaux
d'imbrication, pas supposer "une requête = un span".

## `Span` — champs et types exacts

| Champ | Type protobuf | Contrainte |
|---|---|---|
| `trace_id` | `bytes` | **required**, exactement 16 octets ; tout-zéro ou longueur ≠ 16 = **invalide** |
| `span_id` | `bytes` | **required**, exactement 8 octets ; tout-zéro ou longueur ≠ 8 = **invalide** |
| `parent_span_id` | `bytes` | vide = span racine |
| `name` | `string` | sémantiquement requis (vide = "nom inconnu", pas une erreur dure) |
| `kind` | `enum SpanKind` | `UNSPECIFIED=0, INTERNAL=1 (défaut), SERVER=2, CLIENT=3, PRODUCER=4, CONSUMER=5` |
| `start_time_unix_nano` / `end_time_unix_nano` | `fixed64` | ns Unix ; **"expected end_time >= start_time" mais non garanti par le protocole** |
| `attributes` | `repeated KeyValue` | clés censées être uniques ; **"comportement imprévisible" si dupliquées** — non spécifié par OTLP, à la charge du receiver |
| `dropped_attributes_count` | `uint32` | — |
| `events` / `links` | `repeated Span.Event` / `repeated Span.Link` | non utilisés par les 3 spans `gen_ai.*` retenus pour le MVP (le contenu vit dans les attributs, pas dans des events de span) |
| `status` | `Status { message: string, code: enum{UNSET=0,OK=1,ERROR=2} }` | statut logique du span, distinct de l'attribut `error.type` |

**Point de jonction avec `semconv-genai.md`** : le `kind: client` / `kind: internal`
déclaré dans `spans.yaml` (ex. `gen_ai.inference.client` vs `gen_ai.execute_tool.internal`)
correspond directement à ce champ `Span.kind` — c'est littéralement le même concept
exprimé dans les deux specs, pas une coïncidence de nommage à revérifier.

## `AnyValue` / `KeyValue` (attributs)

```protobuf
message AnyValue {
  oneof value {
    string string_value = 1;
    bool bool_value = 2;
    int64 int_value = 3;
    double double_value = 4;
    ArrayValue array_value = 5;
    KeyValueList kvlist_value = 6;
    bytes bytes_value = 7;
    int32 string_value_strindex = 8; // [Alpha] — signal Profiling uniquement
  }
}
message KeyValue {
  string key = 1;
  AnyValue value = 2;
  int32 key_strindex = 3; // [Alpha] — signal Profiling uniquement
}
```

- `int_value` confirme ce qu'on avait noté comme incertitude dans `semconv-genai.md` :
  **`i64` signé**, pas `u64`.
- `string_value_strindex` et `key_strindex` sont explicitement réservés au signal
  Profiling (statut *Alpha*). La spec dit noir sur blanc : un receiver d'un autre
  signal (nous, en traces) **doit traiter leur présence comme une anomalie non
  fatale** — logguer et traiter comme si le champ était absent, pas tenter de les
  interpréter.

## Transport HTTP (si le receiver l'expose en plus de gRPC)

- Chemin par défaut : `POST /v1/traces`, port par défaut **4318**.
- Deux encodages possibles, distingués par `Content-Type` (client et serveur DOIVENT
  utiliser le même) :
  - `application/x-protobuf` — binaire, schéma identique au gRPC.
  - `application/json` — mapping JSON standard de Protobuf **avec 3 dérogations** :
    `traceId`/`spanId` en hex (pas base64), les enums en entier (pas en nom), et les
    **entiers 64 bits encodés en string JSON** (donc `gen_ai.usage.input_tokens` en
    JSON n'est pas un `number` mais une chaîne comme `"100"` — un parseur JSON naïf
    qui suppose un type numérique natif se trompera).
  - Les champs JSON inconnus DOIVENT être ignorés silencieusement (compatibilité
    ascendante), pas rejetés.
- Limite de taille de requête **recommandée** : 64 MiB (après décompression) → sinon
  `HTTP 413`. Limite de réponse recommandée : 4 MiB.
- `Content-Encoding: gzip` supporté en option.

## Sémantique de réponse — succès partiel

```protobuf
message ExportTraceServiceResponse { ExportTracePartialSuccess partial_success = 1; }
message ExportTracePartialSuccess { int64 rejected_spans = 1; string error_message = 2; }
```

Le protocole prévoit explicitement un **succès partiel** : `HTTP 200` /
gRPC OK même si une partie des spans a été rejetée, avec `rejected_spans` renseigné
et un `error_message` optionnel. Ça veut dire que "valider et persister brut"
(étape 2) doit pouvoir traiter un lot span par span et compter les rejets, plutôt
qu'accepter/rejeter tout le `ExportTraceServiceRequest` en bloc.

## Ignoré volontairement pour le MVP (et pourquoi)

- Signaux logs/métriques OTLP (`logs_service.proto`, `metrics_service.proto`) —
  hors périmètre, seul le signal traces porte les 3 spans `gen_ai.*` retenus.
- `Span.events` / `Span.links` — pas référencés par `gen_ai.inference.client`,
  `gen_ai.execute_tool.internal`, `gen_ai.invoke_agent.{client,internal}` dans la
  spec pinnée ; à réévaluer si un futur incrément gen_ai les utilise.
- `EntityRef` (sur `Resource`) — statut *Development* dans opentelemetry-proto
  lui-même, pas nécessaire pour les 3 spans MVP.
- Codage OTLP/HTTP+JSON — le dossier prévoit `tonic` (gRPC) pour l'étape 2 ; le
  binaire protobuf HTTP peut se rajouter facilement plus tard (même schéma), le
  JSON demande une couche de conversion supplémentaire (int64-as-string, hex ids)
  qui n'est pas nécessaire tant qu'aucun client OTLP/HTTP+JSON réel n'est identifié.

## Incertitudes / décisions à prendre explicitement avant de coder le receiver

- **Clés d'attribut dupliquées dans un même `KeyValue[]`** : le protocole dit
  "comportement imprévisible", ce qui est une permission, pas une spec — le kernel
  doit choisir une politique (ex. dernière occurrence gagne, avec un compteur de
  rejet/anomalie loggé) et la documenter, pas laisser le hasard de l'ordre décider.
- **`trace_id`/`span_id` invalides** (tout-zéro ou mauvaise longueur) : la spec dit
  "considéré invalide" mais ne dicte pas le comportement du receiver — à décider :
  rejeter le span individuellement (cohérent avec le mécanisme de succès partiel
  ci-dessus) plutôt que rejeter tout le lot.
- **Duplication de spans entiers** : la spec documente explicitement, dans ses
  "Known Limitations", que les clients OTLP peuvent renvoyer les mêmes spans après
  une coupure réseau sans accusé de réception ("*this is a deliberate choice*") —
  ce n'est PAS un bug côté émetteur. Le stockage (étape 3) doit décider s'il est
  idempotent sur `(trace_id, span_id)` ou s'il accepte des doublons pour le MVP ;
  ne pas découvrir ce comportement en prod en le prenant pour une anomalie.
- **Limite de taille des messages gRPC** : contrairement à HTTP (64 MiB recommandé
  explicitement dans cette spec), la taille max des messages gRPC n'est pas définie
  ici — c'est un paramètre de configuration `tonic`/`grpc`, pas de ce contrat. À
  vérifier séparément dans la doc `tonic` avant de fixer une valeur, plutôt que de
  supposer un défaut.

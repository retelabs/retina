# Guide d'intégration client

Ce guide s'adresse à qui veut **utiliser** trellis depuis une application
tierce — envoyer de la télémétrie, l'interroger — pas à qui développe le
kernel lui-même. Pour le détail vérifié de chaque contrat (types exacts,
comment chaque champ a été confirmé), voir `docs/interfaces/`, référencé
section par section ci-dessous. Rien ici n'est nouveau : c'est une
synthèse orientée client de contrats déjà vérifiés et déjà en production
dans ce repo.

## Vue d'ensemble

```
votre app (SDK OTel)  --OTLP/gRPC, authentifié-->  kernel (:4317)  --> ClickHouse
dashboard / script     --HTTP, authentifié-->      query-api (:8080) <-- ClickHouse
```

Deux surfaces, deux jetons, aucune infrastructure OTel custom à écrire côté
client — un vrai SDK OpenTelemetry suffit.

## Envoyer de la télémétrie

### Connexion et authentification

- Endpoint gRPC : `TraceService.Export` (proto OTLP standard,
  `docs/interfaces/otlp-ingestion.md`), port `4317` par défaut
  (`KERNEL_BIND`).
- Header requis : métadonnée gRPC `authorization: Bearer <KERNEL_API_KEY>`
  — exactement la convention `OTEL_EXPORTER_OTLP_HEADERS` qu'un SDK OTel
  sait déjà émettre sans code custom :
  ```bash
  export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
  export OTEL_EXPORTER_OTLP_HEADERS="authorization=Bearer <KERNEL_API_KEY>"
  ```
  Détail du mécanisme et pourquoi ce header précisément :
  `docs/interfaces/kernel-auth.md`.
- Sans jeton valide : `UNAUTHENTICATED`. Sans span structurellement valide :
  compté dans `rejected_spans` de la réponse (succès partiel), le reste du
  batch est accepté.

### Ce que le kernel reconnaît

Le dispatch se fait uniquement sur l'attribut `gen_ai.operation.name` —
OTLP ne porte aucun marqueur dédié "ceci est un span gen_ai". Trois valeurs
sont routées vers un type d'événement kernel ; toute autre valeur est
ignorée (`Unmodeled`, pas compté comme rejeté — juste hors périmètre MVP).

| `gen_ai.operation.name` | Événement kernel | Champ obligatoire en plus |
|---|---|---|
| `chat`, `generate_content`, `text_completion` | Appel modèle | `gen_ai.provider.name` |
| `execute_tool` | Appel outil | `gen_ai.tool.name` |
| `invoke_agent` | Run d'agent | `gen_ai.provider.name` **si** le span est de kind OTLP `CLIENT` (agent distant type Bedrock Agents) — pas requis pour un agent in-process (kind `INTERNAL`, type LangChain/CrewAI) |

Attributs recommandés par type (liste complète et niveaux `required`/
`recommended`/`opt_in` : `docs/interfaces/semconv-genai.md`) :

- **Appel modèle** : `gen_ai.request.model`, `gen_ai.response.model`,
  `gen_ai.usage.input_tokens`/`output_tokens` (`int`, pas de `u64` négatif
  côté wire), `gen_ai.response.finish_reasons` (array), `gen_ai.conversation.id`.
- **Appel outil** : `gen_ai.tool.call.id`, `gen_ai.tool.type`,
  `gen_ai.tool.description`, `gen_ai.agent.name` (l'agent qui exécute
  l'outil).
- **Run d'agent** : `gen_ai.agent.name`/`id`/`description`/`version`,
  `gen_ai.request.model` (seulement si l'agent a un modèle fixe unique, pas
  de sélection dynamique), usages de tokens.

Tout attribut non reconnu de la liste ci-dessus est conservé tel quel dans
`extra_attributes` — rien n'est perdu, juste pas promu en champ de première
classe.

### Attributs métier (pour qu'un plugin réagisse)

Les plugins réels (`crates/plugin-fraudos`, `crates/plugin-medical`)
interprètent des attributs `<vertical>.*` posés directement sur le span
`invoke_agent` par l'application elle-même, pendant l'exécution — pas
injectés après coup :

- **fraudos** : `fraudos.final_decision` (`CONFIRMED_FRAUD`/`REQUEST_BLOCK`/
  `ESCALATED_COMPLIANCE`/`CASE_OPENED`/...), `fraudos.transaction_id`.
- **oncology** : `oncology.current_step`, `oncology.hipaa_cleared`/
  `gdpr_cleared` (bool), `oncology.submitted_by`/`approved_by`.

Le plugin ajoute ses propres attributs dérivés (ex.
`fraudos.requires_urgent_review`, `oncology.awaiting_approval`) et, s'il
détecte une anomalie, une entrée `plugin.warning` — visible dans
`extra_attributes` du span concerné et compté dans
`GET /metrics/summary.spans_with_warnings` (voir plus bas). Détail des
règles de chaque plugin : `docs/interfaces/plugin-contract-v0.md`,
`docs/interfaces/oncology-governance.md`.

### Exemple (Python, SDK OTel standard)

```python
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter

provider = TracerProvider()
provider.add_span_processor(
    BatchSpanProcessor(OTLPSpanExporter(endpoint="localhost:4317", insecure=True))
    # Bearer KERNEL_API_KEY via OTEL_EXPORTER_OTLP_HEADERS, pas de code ici.
)
trace.set_tracer_provider(provider)
tracer = trace.get_tracer("my-agent-app")

with tracer.start_as_current_span("invoke_agent my-agent", kind=trace.SpanKind.INTERNAL) as agent_span:
    agent_span.set_attribute("gen_ai.operation.name", "invoke_agent")
    agent_span.set_attribute("gen_ai.agent.name", "my-agent")
    agent_span.set_attribute("fraudos.transaction_id", transaction_id)
    agent_span.set_attribute("fraudos.final_decision", "CONFIRMED_FRAUD")

    with tracer.start_as_current_span("chat claude-sonnet", kind=trace.SpanKind.CLIENT) as model_span:
        model_span.set_attribute("gen_ai.operation.name", "chat")
        model_span.set_attribute("gen_ai.provider.name", "aws.bedrock")
        model_span.set_attribute("gen_ai.request.model", "claude-sonnet")
        # ... appel réel, puis :
        model_span.set_attribute("gen_ai.usage.input_tokens", usage.input_tokens)
        model_span.set_attribute("gen_ai.usage.output_tokens", usage.output_tokens)
```

Squelette illustratif — vérifier l'API exacte du SDK `opentelemetry-python`
au moment de l'écrire pour de vrai, même règle que pour tout le reste de ce
projet.

## Interroger

Port `8080` par défaut (`QUERY_API_BIND`), header `Authorization: Bearer
<QUERY_API_KEY>` requis sur les 3 routes, sinon `401`. Contrat complet :
`docs/interfaces/query-api.md`.

### `GET /traces?limit=N` — traces récentes

`limit` par défaut 50, plafonné à 500. Tableau, trié par début décroissant :

```json
[{ "trace_id": "<32 hex>", "span_count": 3, "start_time_unix_nano": 0, "end_time_unix_nano": 0 }]
```

### `GET /traces/{trace_id}` — spans d'une trace

`trace_id` en hex minuscule 32 caractères (même encodage que `traceId` en
OTLP/JSON). `400` si mal formé, `404` si aucun span. **Liste plate**, triée
par `start_time` — pas d'arbre JSON imbriqué, reconstruire côté client via
`parent_span_id` (déjà présent sur chaque span).

Champs par span (`SpanDto`, `crates/query-api/src/dto.rs`) : `trace_id`,
`span_id`, `parent_span_id`, `kind` (`model_call`/`tool_call`/`agent_run`),
`start_time_unix_nano`, `end_time_unix_nano`, `status_code`,
`status_message`, `error_type`, `operation_name`, `provider_name`,
`request_model`, `response_model`, `input_tokens`, `output_tokens`,
`cache_read_input_tokens`, `cache_creation_input_tokens`,
`finish_reasons`, `conversation_id`, `cost_usd`, `tool_name`, `tool_call_id`,
`tool_type`, `tool_description`, `agent_invocation_kind`, `agent_name`,
`agent_id`, `agent_description`, `agent_version`, et `extra_attributes`
(objet `string → string` — tout ce qui n'est pas un champ de première
classe, y compris les attributs `<vertical>.*` et `plugin.warning`).

`cost_usd` (ajouté le 2026-08-17) : `null` sauf si le span pose
`gen_ai.usage.input_tokens`/`output_tokens` **et** un `gen_ai.request.model`/
`response.model` reconnu par la table de prix statique de
`crates/pricing` (aujourd'hui : une partie des modèles OpenAI et
Anthropic seulement — détail et lacunes connues dans
`docs/interfaces/cost-calculation.md`). Calculé une seule fois à
l'ingestion, jamais recalculé après un changement de tarif.

### `GET /metrics/summary` — agrégats

```json
{
  "by_kind": [{ "kind": "agent_run", "span_count": 1, "total_input_tokens": 8420, "total_output_tokens": 1150, "total_cost_usd": 0.126 }],
  "spans_with_warnings": 0
}
```

`total_cost_usd` : `null` (pas `0.0`) si aucun span du groupe n'a de coût
calculé — distinct d'un coût réellement nul.

`spans_with_warnings` : nombre de spans portant au moins une entrée
`plugin.warning` — le signal de gouvernance/monitoring produit par les
plugins réellement câblés dans `kernel` (`crates/plugin-sink`).

## Déployer

Pas dupliqué ici :
- `scripts/dev-stack.sh up` — pile Docker complète en local
  (`docker/docker-compose.stack.yml`).
- `crates/orchestrator` — control plane maison, mêmes 3 services, via une
  vraie API HTTP (`POST /deploy`, `GET /status`, `POST /teardown`) plutôt
  que des scripts — `docs/interfaces/docker-engine-api.md`.

## Pour aller plus loin

Chaque contrat ci-dessus a sa fiche complète (source vérifiée, date,
incertitudes restantes) dans `docs/interfaces/` : `otlp-ingestion.md`,
`semconv-genai.md`, `kernel-auth.md`, `query-api.md`,
`plugin-contract-v0.md`, `oncology-governance.md`, `fraudos-agentspan.md`.

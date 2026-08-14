# semconv-genai — conventions sémantiques `gen_ai.*` retenues pour le MVP

- Source faisant autorité : https://github.com/open-telemetry/semantic-conventions-genai
  (`model/gen-ai/spans.yaml`, `model/gen-ai/registry.yaml`, `model/aws-bedrock/registry.yaml`)
- Version/commit pinné : `30182acd5ed78ab5f619041eaec5e95a4eb83a48` (branche `main` —
  **aucun tag publié** dans ce repo au moment du pin ; statut global du repo :
  *Development*, aucun span/attribut *Stable*)
- Date de vérification : 2026-08-14
- Épinglé via : `scripts/pin-semconv.sh 30182acd5ed78ab5f619041eaec5e95a4eb83a48`
  → `vendor/semconv-genai/`

## Les 3 spans retenus pour le MVP (dossier section 2.1 : "appel modèle, appel outil, run d'agent")

### 1. Appel modèle → span `gen_ai.inference.client`

`kind: client`. Attributs retenus pour le modèle interne :

| Attribut | Type spec | Niveau |
|---|---|---|
| `gen_ai.provider.name` | enum fermée (`members`) : `openai`, `anthropic`, `aws.bedrock`, `gcp.vertex_ai`, `gcp.gemini`, `gcp.gen_ai`, `azure.ai.inference`, `azure.ai.openai`, `cohere`, `ibm.watsonx.ai`, `perplexity`, `x_ai`, `deepseek`, `groq`, `mistral_ai`, `moonshot_ai` | **required**, sampling_relevant |
| `gen_ai.operation.name` | enum fermée (17 valeurs, voir plus bas) | **required** |
| `gen_ai.request.model` | string | conditionally_required (si disponible) |
| `gen_ai.response.model` | string | recommended |
| `server.address` / `server.port` | string / int | recommended / conditionally_required |
| `gen_ai.usage.input_tokens` | **int** | recommended |
| `gen_ai.usage.output_tokens` | **int** | recommended |
| `gen_ai.usage.cache_read.input_tokens` | int | recommended — déjà inclus dans `input_tokens` |
| `gen_ai.usage.cache_creation.input_tokens` | int | recommended — déjà inclus dans `input_tokens` |
| `gen_ai.response.finish_reasons` | **string[]** | recommended |
| `gen_ai.conversation.id` | string | conditionally_required |
| `error.type` | (défini hors repo, voir "Incertitudes") | conditionally_required si erreur |

Note importante trouvée dans la spec : le comptage des tokens **diffère par provider** — ex. Anthropic exclut les tokens cache de `input_tokens` (il faut les rajouter), OpenAI/Azure les incluent déjà. Le mapping/adaptateur ne peut donc pas traiter `gen_ai.usage.input_tokens` comme une valeur brute universelle sans connaître `gen_ai.provider.name`.

### 2. Appel outil → span `gen_ai.execute_tool.internal`

`kind: internal`.

| Attribut | Type spec | Niveau |
|---|---|---|
| `gen_ai.operation.name` | enum fermée, valeur `execute_tool` | **required** |
| `gen_ai.tool.name` | string | **required** |
| `gen_ai.tool.call.id` | string | recommended si disponible |
| `gen_ai.tool.type` | string libre (exemples : `function`, `extension`, `datastore` — **pas** une enum fermée contrairement à `operation.name`/`provider.name`) | recommended |
| `gen_ai.tool.description` | string | recommended si disponible |
| `gen_ai.agent.name` | string | conditionally_required si applicable |
| `gen_ai.tool.call.arguments` / `gen_ai.tool.call.result` | **`any`** (schéma JSON référencé, objet libre) | **opt_in** — marqué "may contain sensitive information" dans la spec |
| `error.type` | — | conditionally_required si erreur |

### 3. Run agent → spans `gen_ai.invoke_agent.client` (agent distant) et `gen_ai.invoke_agent.internal` (agent in-process)

Différence clé entre les deux variantes, à ne pas manquer : `gen_ai.provider.name` est **required sur la variante `client`** (ex. AWS Bedrock Agents, OpenAI Assistants) mais **absent de la liste d'attributs de la variante `internal`** (ex. LangChain, CrewAI) — un agent in-process n'a pas de "provider" au sens transport.

| Attribut | Type spec | Niveau |
|---|---|---|
| `gen_ai.operation.name` | enum fermée, valeur `invoke_agent` | **required** |
| `gen_ai.agent.name` | string | conditionally_required si disponible |
| `gen_ai.agent.id` | string (identifiant stable fourni par le provider, ex. ARN Bedrock) | conditionally_required — variante client surtout |
| `gen_ai.agent.description` | string | conditionally_required |
| `gen_ai.agent.version` | string | conditionally_required — variante client |
| `gen_ai.request.model` | string | recommended, **seulement si l'agent a un modèle unique fixe** (ne pas peupler si sélection dynamique) |
| `gen_ai.provider.name` | enum fermée | **required (variante client uniquement)** |
| `gen_ai.usage.input_tokens` / `output_tokens` / caches | int | recommended |
| `gen_ai.conversation.id` | string | conditionally_required |
| `error.type` | — | conditionally_required si erreur |

## Deux enums fermées mais en évolution — implication pour le typage Rust

`gen_ai.operation.name` et `gen_ai.provider.name` sont définies avec une liste `members` fermée dans le YAML — mais le repo est en statut *Development* et de nouvelles valeurs (nouveaux providers, nouvelles opérations) sont ajoutées régulièrement sans bump de version stable. **Recommandation pour le modèle interne** : ne pas coder ces valeurs comme un `enum` Rust strict qui échouerait à la désérialisation sur une valeur inconnue — utiliser un type "enum connu + variante `Other(String)`" pour rester tolérant à l'évolution amont sans perdre la donnée, à revalider si/quand la spec passe en *Stable*.

## Ignoré volontairement pour le MVP (et pourquoi)

- **Spans hors périmètre** : `gen_ai.embeddings.client`, `gen_ai.retrieval.client`, `gen_ai.fetch_response.client`, `gen_ai.memory.client`, `gen_ai.create_agent.client`, `gen_ai.invoke_workflow.internal`, `gen_ai.plan.internal` — retrieval/mémoire/workflow explicitement hors périmètre MVP (dossier section 4).
- **Tous les attributs de contenu** (`gen_ai.input.messages`, `gen_ai.output.messages`, `gen_ai.system_instructions`, `gen_ai.tool.definitions`, `gen_ai.tool.call.arguments`, `gen_ai.tool.call.result`, `gen_ai.retrieval.query.text`) — déjà marqués `opt_in` par la spec elle-même, et de toute façon désactivés par défaut par notre propre politique PII (dossier section 2.1).
- **Paramètres de requête fins** (`top_k`, `top_p`, `frequency_penalty`, `presence_penalty`, `seed`, `stop_sequences`, `reasoning.level`, `output.type`, `stream`, `prompt.name/version/variable`) — recommended mais pas centraux au schéma générique à 2 couches du MVP ; s'ils arrivent, ils atterrissent dans le sac d'attributs génériques non promus en champs de première classe, pas dans les extensions provider (ce ne sont pas des attributs provider-spécifiques).
- **Extensions Bedrock non fintech** (`aws.bedrock.guardrail.id`, `aws.bedrock.knowledge_base.id`, définies dans `model/aws-bedrock/registry.yaml`, seulement 2 attributs dans ce repo) — stockées dans la couche d'attributs spécifiques provider (dossier section 2.1) mais pas promues en champs kernel de première classe ; à l'inverse, `transaction_id` / `risk_score` / seuil de décision (section 3) ne viennent PAS de cette spec — ce sont des attributs propres au plugin fintech, à définir dans le contrat de plugin v0 (étape 5), pas ici.

## Incertitudes restantes / à revalider avant prod

- ~~Largeur exacte du type `int`~~ **Résolu** : `vendor/opentelemetry-proto@v1.11.0`,
  `opentelemetry/proto/common/v1/common.proto` confirme `int64 int_value = 3;` dans le
  `oneof` `AnyValue`. Donc `gen_ai.usage.input_tokens`/`output_tokens`/etc. sont des
  **`i64` signés** côté wire OTLP — pas `i32`, pas `u64`. Implication pour le modèle
  interne : soit garder `i64` en cohérence stricte avec le wire (et rejeter/loguer les
  valeurs négatives reçues comme une anomalie plutôt que les caster en `u64`), soit
  convertir en `u64` **après** une validation explicite `>= 0` — ne jamais faire un
  `as u64` nu sur une valeur reçue, qui wrapperait silencieusement un `int_value`
  négatif malformé en un très grand nombre positif.
- **`error.type`** n'est PAS défini dans `semantic-conventions-genai` — c'est une référence (`ref: error.type`) vers l'attribut générique du repo de base `open-telemetry/semantic-conventions`. On n'a pas besoin de pinner ce second repo pour le MVP : la note locale ("SHOULD match... ou un autre identifiant d'erreur à faible cardinalité") suffit à le traiter comme `String` libre à faible cardinalité côté kernel — mais si un jour on a besoin de sa définition précise (contraintes de format, exemples), il faudra pinner ce second repo séparément.
- **Repo sans tag** : le pin actuel est un commit `main`, pas un tag stable — à surveiller plus activement qu'un pin sur tag, et à re-vérifier (`scripts/check-pins.sh` + nouvelle lecture) avant tout changement de version, puisqu'il n'y a pas de changelog de release formel à suivre entre deux pins.

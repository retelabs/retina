# cost-calculation — calcul du coût $ par span (`crates/pricing`)

- Sources faisant autorité :
  - comportement des champs `gen_ai.usage.*` par fournisseur :
    `docs/interfaces/semconv-genai.md` (déjà vérifié à l'étape 1 du kernel)
  - prix OpenAI : https://developers.openai.com/api/docs/pricing
    (`platform.openai.com/docs/pricing` redirige dessus), récupéré le
    2026-08-17
  - prix Anthropic : https://platform.claude.com/docs/en/about-claude/pricing,
    récupéré le 2026-08-17
  - modèles Anthropic `claude-sonnet-5`/`claude-opus-5`/`claude-fable-5`/
    `claude-haiku-4-5-20251001` : identifiants exacts issus du contexte
    système de cette session (aussi fiable qu'une doc officielle — cette
    session tourne elle-même sur un modèle Claude)
- Date de vérification : 2026-08-17
- Portée : `crates/pricing`, appelé une seule fois à l'ingestion depuis
  `crates/clickhouse-sink/src/row.rs`, stocké dans `spans.cost_usd`
  (migration `0003_add_cost_usd.sql`).

## Décisions de scope, prises avec l'utilisateur

Trois questions posées avant de coder (voir échange du 2026-08-17) :

1. **Où vit la table de prix ?** Statique, versionnée dans ce repo
   (`crates/pricing/src/table.rs`) — pas fournie par le client à
   l'ingestion. Cohérent avec ADR-0001 (mono-tenant) : un seul client réel
   (the-client) aujourd'hui, pas de besoin démontré de prix par tenant.
2. **Calcul à l'ingestion ou à la requête ?** À l'ingestion. Le coût est
   figé au prix en vigueur au moment de l'insert.
3. **Changements de prix dans le temps ?** Un changement de tarif = un
   commit dans `table.rs`, jamais un recalcul rétroactif — les spans déjà
   ingérés gardent le coût calculé avec le prix qui était réellement en
   vigueur à ce moment-là. Revers assumé : un span ingéré avant l'ajout
   d'un modèle à la table reste `cost_usd = NULL` pour toujours, pas de
   rattrapage automatique.

## Trouvaille structurante : la comptabilité des tokens de cache diffère par fournisseur

`docs/interfaces/semconv-genai.md` (§33, déjà documenté avant ce chantier)
notait : *"le comptage des tokens diffère par provider — ex. Anthropic
exclut les tokens cache de `input_tokens` (il faut les rajouter),
OpenAI/Azure les incluent déjà."* Une seule formule de coût universelle
aurait été fausse pour l'un des deux fournisseurs — vérifié avant d'écrire
`estimate_cost_usd`, pas découvert après coup en comparant à une facture
réelle.

`CacheAccounting` (`crates/pricing/src/lib.rs`) encode cette différence :

- **`IncludedInInput`** (OpenAI, Azure OpenAI) : `input_tokens` contient déjà
  les tokens de cache-read. Coût = `(input_tokens - cache_read_tokens)` au
  tarif normal + `cache_read_tokens` au tarif cache + `output_tokens`.
- **`AdditionalToInput`** (Anthropic) : `input_tokens` exclut les tokens de
  cache. Coût = `input_tokens` au tarif normal + `cache_read_tokens` +
  `cache_creation_tokens` (write), chacun à son tarif propre, + `output_tokens`.

Approximation documentée, pas un guess silencieux : Anthropic facture les
écritures de cache différemment selon leur TTL (5 minutes vs 1 heure), mais
`ModelCallEvent`/`AgentRunEvent` ne portent aucun champ pour distinguer
lequel a été utilisé. `crates/pricing` utilise systématiquement le tarif
5 minutes (le comportement par défaut du cache prompt Anthropic).

## Quel identifiant de modèle est utilisé pour le lookup de prix

`response_model` en priorité, `request_model` en repli
(`ModelCallEvent` a les deux ; `AgentRunEvent` n'a que `request_model`) —
c'est le modèle qui a réellement servi la requête, donc celui dont le tarif
s'applique. Lookup par correspondance **exacte de chaîne**, pas de
préfixe/fuzzy matching — un mauvais match tarifierait silencieusement un
span au mauvais prix, pire qu'un span non tarifié.

## Lacune connue, pas encore comblée

**Aucune télémétrie réelle n'existait pour confirmer le format exact des
chaînes `request_model`/`response_model`** envoyées en pratique — the-client ne
peuple aujourd'hui aucun de ces deux champs (constat qui a lancé ce
chantier). Un alias (`"gpt-4o"`) et un instantané daté
(`"gpt-4o-2024-08-06"`) sont deux chaînes différentes pour le lookup exact
de `crates/pricing` ; laquelle un SDK réel envoie n'a pas été vérifié
contre une vraie réponse d'API. À vérifier dès que the-client (ou tout autre
client) peuple ces champs pour de vrai, avant de faire confiance à la
couverture de la table au-delà de ses nombres par token.

## Fournisseurs non tarifés, délibérément

Groq (demandé explicitement par l'utilisateur) : la page officielle
(`groq.com/pricing`) n'a renvoyé aucun tableau de prix exploitable, et
`console.groq.com/docs/pricing` a renvoyé `404` — vérifié le 2026-08-17.
Seuls des agrégateurs tiers avaient des chiffres, écartés pour la même
raison que la divergence Hetzner déjà rencontrée dans `docs/cost-model.md`
(chiffres non fiables/contradictoires). AWS Bedrock, IBM watsonx, GCP
Vertex/Gemini, Azure AI, Cohere, Perplexity, xAI, DeepSeek, Mistral,
Moonshot : non tarifés non plus, aucun n'a été demandé explicitement et
aucun n'a été vérifié cette session. Un span de l'un de ces fournisseurs
reste `cost_usd = NULL` — pas une erreur, juste "pas encore tarifé".
Ajouter un fournisseur : vérifier sa vraie page de prix officielle avant
d'ajouter une entrée à `table.rs`, jamais depuis la mémoire (règle
permanente du projet, CLAUDE.md).

## `spans.cost_usd` — comportement d'agrégation vérifié

`cost_usd` est `Nullable(Float64)`. Vérifié contre un vrai ClickHouse
(2026-08-17, pas supposé) : `sum(cost_usd)` renvoie `NULL` à la fois sur un
groupe vide et sur un groupe où toutes les valeurs sont `NULL` — différent
du comportement de `max()` sur une colonne non-nullable déjà documenté dans
`docs/interfaces/clickhouse-retention.md` (`0`, pas `NULL`, sur une table
vide). `query-api::MetricsSummaryDto.by_kind[].total_cost_usd` est donc
`Option<f64>`, délibérément **pas** ramené à `0.0` comme le sont
`total_input_tokens`/`total_output_tokens` — un total à `0.0` laisserait
croire à un coût réellement nul plutôt qu'à "aucun span tarifé dans ce
groupe", deux situations différentes que l'agrégat doit pouvoir
distinguer.

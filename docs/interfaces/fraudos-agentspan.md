# fraudos-agentspan — écart avec le dossier + forme réelle du signal fraudos (étape 7)

- Source faisant autorité : the fraudos prototype repository (cloné en
  lecture seule le 2026-08-14 dans un répertoire scratch, pas vendoré dans ce
  repo — ce n'est pas une spec externe à figer comme semconv-genai/otlp, c'est
  le code réel d'un prototype qui continuera d'évoluer indépendamment).
- Date de vérification : 2026-08-14

## Correction à apporter au dossier, section 3

Le dossier affirme : *"Un prototype existant (fraudos) tourne déjà sur AWS
Bedrock, avec une instrumentation botocore + ADOT"* et *"Bedrock via ADOT
émet déjà nativement les conventions OTel GenAI... Il n'est pas nécessaire de
construire un adaptateur."*

**Vérifié faux pour l'état actuel du repo** : `requirements.txt` ne contient
aucune dépendance `opentelemetry-*`/ADOT, et aucune référence à
`opentelemetry`, `adot` ou `otlp` n'existe nulle part dans le code. L'app
utilise `bedrock-runtime.converse()` (API Converse, tool-use multi-tours,
modèles Claude Haiku/Sonnet/Opus via inference profiles cross-region), mais
l'observabilité est **maison** :

- `observability/span.py` — dataclass `AgentSpan`, un **rollup par run
  d'agent complet** (pas un span par appel modèle/outil), sans `trace_id`,
  `span_id` ni attribut `gen_ai.*`.
- `observability/writer.py` — écrit vers **CloudWatch Logs** (audit 7 ans) et
  **DynamoDB** (dashboards business), jamais vers un collecteur OTLP.

**Conséquence pour l'étape 7** : "brancher directement, sans couche
intermédiaire à construire" (dossier) n'est pas possible en l'état — il n'y a
rien à brancher. Un adaptateur est nécessaire dans tous les cas, que ce soit
pour convertir `AgentSpan` a posteriori (option retenue ici) ou pour ajouter
une vraie instrumentation ADOT dans `fraudos-prototype` (option écartée pour cette
itération — voir ci-dessous).

## Deuxième correction : le score de risque vient d'un outil, pas du modèle

Le dossier suppose *"score de risque et seuil de décision comme attributs de
premier ordre [de l'appel modèle], pas noyés dans le blob de sortie"*.
Réellement : `tools/data_tools.py::get_transaction_score` interroge Athena
(modèle ML XGBoost externe) et retourne `fraud_score`, `score_band`, et des
contributions de features — c'est un **résultat d'appel outil**
(`gen_ai.tool.call.result`), pas un attribut de réponse du LLM. Dans notre
schéma, `gen_ai.tool.call.result` est `opt_in` et masqué par défaut (politique
PII, dossier section 2.1) — un futur plugin fintech devra explicitement
promouvoir `fraud_score` en attribut de première classe plutôt que compter
sur `gen_ai.response.*`.

## Forme réelle d'`AgentSpan` (`observability/span.py`)

```python
@dataclass
class AgentSpan:
    session_id: str
    agent_role: str          # "fraud_orchestrator" | "fraud_investigator" |
                              # "compliance_officer" | "fraud_scorer" (agents/factory.py)
    task_summary: str
    total_turns: int
    escalated_to_opus: bool
    tools_called: list[str]  # noms d'outils, ordonnés, pas d'id/timing individuel
    tools_failed: list[str]
    unique_tools: int        # dérivé
    success: bool
    requires_human_review: bool
    final_decision: Optional[str]   # mot-clé extrait par recherche dans le texte :
                                     # CASE_OPENED, DISMISSED, ESCALATED_COMPLIANCE,
                                     # ESCALATED, REQUEST_BLOCK, MONITOR,
                                     # CONFIRMED_FRAUD, FALSE_POSITIVE
    total_input_tokens: int         # agrégé sur tous les tours, pas de détail par tour
    total_output_tokens: int
    estimated_cost_usd: float
    primary_model_id: str           # modèle du dernier tour seulement
    duration_seconds: float
    started_at: str                 # ISO 8601
    ended_at: str
    case_id: Optional[str]
    bank_id: Optional[str]
    transaction_id: Optional[str]
```

## Décision de conversion retenue : AgentSpan → OTLP (reconstruction, pas fidèle)

`AgentSpan` est un agrégat de fin de run, pas une trace distribuée — la
conversion vers des spans OTLP est donc une **reconstruction au mieux**, pas
un mapping fidèle (l'information par-tour n'existe pas dans la source).
Implémentée dans `crates/fraudos-replay` :

- 1 span racine `gen_ai.invoke_agent.internal` (→ `AgentRunEvent`,
  `invocation_kind: Internal` — l'app tourne le run localement, ce n'est pas
  un agent Bedrock hébergé) : `agent_name = agent_role`,
  `operation_name = InvokeAgent`, tokens = `total_input/output_tokens`.
- 1 span enfant `gen_ai.inference.client` (→ `ModelCallEvent`) agrégeant tous
  les tours en un seul, faute de détail par tour : `request_model =
  primary_model_id`, `provider_name = aws.bedrock`, mêmes tokens totaux.
  **Limitation assumée** : si le run a été escaladé (`escalated_to_opus`),
  les tours pré-escalade (Haiku/Sonnet) et le tour Opus sont fondus dans un
  seul span — un vrai ADOT capturerait un span par tour.
- 1 span enfant `gen_ai.execute_tool.internal` (→ `ToolCallEvent`) par entrée
  de `tools_called`, marqué en erreur (`status.code = Error`) s'il apparaît
  dans `tools_failed` — sans timing individuel réel (span synthétique, même
  fenêtre temporelle que le parent, documenté comme approximatif).
- `case_id`, `bank_id`, `transaction_id`, `final_decision`,
  `estimated_cost_usd` : pas d'attribut `gen_ai.*` correspondant — envoyés
  comme attributs génériques préfixés `fraudos.*` (namespace propre à ce
  vertical, pas dans le registre `gen_ai.*` pinné) sur le span racine,
  atterrissant dans `extra_attributes` côté kernel. C'est exactement le point
  que le dossier (section 3) anticipait pour un futur plugin fintech : ces
  attributs devraient être promus en champs de première classe par un plugin
  dédié plutôt que de rester dans le sac générique — non fait ici,
  volontairement hors périmètre de cette itération (pas de plugin fintech
  écrit, seulement le contrat v0 générique de l'étape 5).
- `trace_id`/`span_id` : dérivés déterministiquement de `session_id` (hash),
  pas aléatoires — un rejeu du même fixture produit le même trace_id,
  pratique pour re-tester sans dupliquer des lignes en base à volonté.

## Option écartée pour cette itération

Ajouter une vraie instrumentation ADOT dans `fraudos-prototype` (repo séparé) et la
faire tourner pour de vrai contre des appels Bedrock réels — nécessite des
credentials AWS Bedrock valides et modifie un projet distinct de celui-ci.
Choix explicite de l'utilisateur : rejouer des `AgentSpan` réalistes
converties en OTLP contre le pipeline réel (`otlp-receiver` →
`clickhouse-sink` → `query-api`) sans toucher à `fraudos-prototype`, quitte à
revisiter une vraie instrumentation ADOT plus tard si besoin.

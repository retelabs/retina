# oncology-governance — forme réelle du vertical oncologie (plugin médical)

- Source faisant autorité : the oncology pipeline repository
  (cloné en lecture seule, pas vendoré — même traitement que
  `docs/interfaces/fraudos-agentspan.md`), + comparaison ponctuelle avec
  another implementation of the same vertical pour un second point de
  référence sur le même domaine (médical). Les identifiants des démos en
  ligne fournis par l'utilisateur n'ont pas été utilisés — le code seul a
  suffi à ancrer les décisions ci-dessous, comme pour fraudos.
- Date de vérification : 2026-08-15

## Ce que ce vertical apporte de différent de fraudos

- **Pipeline LangGraph déterministe** (7 nodes en séquence fixe), pas une
  boucle agentique multi-tours à sélection d'outils par le LLM (Bedrock
  Converse). Un seul vrai appel LLM dans tout le pipeline : `recommendation_node`
  (`ChatOpenAI(model="gpt-4o")`, sortie structurée `ClinicalRecommendation`).
  **Conséquence pour le mapping** : contrairement à `fraudos-agentspan.md`
  (1 `AgentRunEvent` + 1 `ModelCallEvent` agrégé + N `ToolCallEvent`), ici
  les nodes déterministes (`ingestion`, `preprocessing`, `modeling`,
  `evaluation`, `visualization`, `monitoring`) ne sont **pas** mappés en
  `ToolCallEvent` — ce sont des étapes de workflow, pas des outils
  sélectionnés par un agent (`gen_ai.execute_tool.*` ne correspond pas
  sémantiquement à ça ; `gen_ai.invoke_workflow.internal` existe dans la
  spec pinnée mais est hors périmètre MVP, `docs/interfaces/semconv-genai.md`).
  Reconstruction retenue : 1 `AgentRunEvent` (le run complet) + 1
  `ModelCallEvent` (le seul vrai appel LLM, `provider=openai`,
  `request_model=gpt-4o`) — plus simple et plus honnête que de forcer un
  mapping qui n'existe pas dans la source.
- **Observabilité** : LangSmith (`src/observability/langsmith_config.py`,
  activé par variables d'env `LANGSMITH_*`, callbacks LangChain) — encore
  une fois pas d'OTLP natif. Deuxième vertical de suite où c'est le cas ;
  schéma qui se confirme plutôt que hypothèse isolée à fraudos.

## Gouvernance — deux mécanismes réels, explicites dans le code

1. **Gate de conformité déterministe** (`src/tools/oncology_tools.py::check_hipaa_compliance`)
   — noms de colonnes suspects (`ssn`, `email`, `dob`, ...) + scan NER du
   contenu (Microsoft Presidio) si le premier check ne trouve rien. Produit
   `hipaa_cleared`/`gdpr_cleared` (bool) et `compliance_flags` (liste de
   strings : `"PII_DETECTED:ssn,email"`, `"HIPAA_CLEARED"`, ...). Si l'un des
   deux est faux, `route_after_ingestion` route vers `__end__` — le pipeline
   **ne peut pas** atteindre modeling/LLM. Invariant explicite du repo
   (`CLAUDE.md` de `oncology-pipeline`) : *"NEVER let the LLM be the final
   decision-maker for a compliance/regulatory block — deterministic checks
   only."*
2. **Gate HITL** — `interrupt_before=["recommendation"]` (LangGraph) : le
   graphe **pause** avant `recommendation_node`, reprend seulement sur
   `POST /pipeline/{p}/{s}/approve` (`Depends(require_clinician)`, RBAC
   clinician-only). `submitted_by`/`approved_by` tracent qui a soumis vs
   approuvé. Invariant explicite : *"ALWAYS keep interrupt_before=[...] — no
   clinical recommendation without human sign-off."* Un run peut donc
   légitimement être observé **dans l'état intermédiaire** : `current_step`
   encore sur `"recommendation"`, `submitted_by` renseigné,
   `approved_by = None` — ce n'est pas un bug, c'est l'état normal d'un run
   en attente de validation humaine. C'est un cas de fixture réaliste, pas
   inventé.

## Trouvaille : deux implémentations réelles du même vertical divergent sur la conformité

`client-project` (`backend/src/client-project.OnCall.Infrastructure/AI/Agents/ComplianceAgent.cs`)
fait juger la conformité RGPD **par le LLM lui-même** — un prompt libre
demandant un rapport texte non structuré (*"Analyze the following text and
report: 1. Any personal data (PII)... Be concise."*), pas de sortie
structurée bool/enum, pas de check déterministe. C'est exactement ce que
l'invariant explicite d'`oncology-pipeline` interdit. Pas une supposition :
une divergence réelle entre deux systèmes de production du même vertical,
trouvée en lisant les deux. Le plugin ci-dessous ne peut donc pas supposer
qu'"une vérification de conformité a eu lieu" implique "vérification fiable" —
seul le résultat structuré (`hipaa_cleared`/`gdpr_cleared` bool) est exploité,
jamais un texte libre.

## Décision de conversion retenue (`crates/oncology-replay`)

Mêmes réflexes que `fraudos-agentspan.md` — reconstruction best-effort, pas
fidèle (le state LangGraph réel a bien plus de champs que ce qu'on retient) :

- `AgentRunEvent` (racine, `invocation_kind: Internal` — LangGraph tourne en
  process, pas un agent hébergé) : `agent_name = "oncology_pipeline"`,
  attributs `oncology.*` : `current_step`, `hipaa_cleared`, `gdpr_cleared`,
  `compliance_flags` (joints par `;`), `submitted_by`, `approved_by`
  (absent si `None`), `patient_id`.
- `ModelCallEvent` (enfant, seulement si `recommendation_node` a
  **réellement tourné** — `current_step ∈ {"monitoring", "done"}`, **pas**
  `current_step == "recommendation"`). Point vérifié précisément dans le
  code, pas déduit du nom : `visualization_node` retourne déjà
  `current_step: "recommendation"` **avant** que `interrupt_before=["recommendation"]`
  ne mette le graphe en pause — cette valeur signifie "sur le point d'appeler
  le LLM, pas encore fait", et `recommendation_node` lui-même ne retourne
  `current_step: "monitoring"` qu'une fois l'appel effectué. Confondre les
  deux aurait généré un span `ModelCallEvent` pour un appel qui n'a pas eu
  lieu. `provider_name = openai`, `request_model = "gpt-4o"`. **Pas de
  compte de tokens** : le code (`generate_recommendations`) ne capture pas
  la métadonnée d'usage retournée par LangChain — contrairement à
  `AgentSpan` (fraudos) qui l'agrège explicitement. Champ laissé `None`,
  pas inventé.
- Pas de `ToolCallEvent` du tout pour ce vertical (voir mapping plus haut).

## Plugin retenu (`crates/plugin-medical`) — règles directement issues des invariants ci-dessus

1. `oncology.hipaa_cleared == false` ou `oncology.gdpr_cleared == false` **et**
   `oncology.current_step != "failed"` → avertissement : le gate de
   conformité rapporte un échec mais le pipeline n'a pas été stoppé — signal
   de défense en profondeur côté observabilité (le kernel ne peut pas
   vérifier que `route_after_ingestion` a réellement été respecté, seulement
   que les attributs qu'il a produits sont cohérents entre eux).
2. `oncology.current_step` ∈ `{"recommendation", "monitoring", "done"}` **et**
   `oncology.approved_by` absent → avertissement : recommandation clinique
   atteinte ou dépassée sans trace d'approbation HITL.
3. Attribut dérivé `oncology.awaiting_approval = true` quand `submitted_by`
   est présent mais `approved_by` absent (état intermédiaire légitime,
   distinct d'une violation — pas un avertissement, juste un signal de
   monitoring pour repérer les runs en attente).
- No-op sur tout événement sans `oncology.current_step` (pas un run de ce
  vertical) — même garde-fou que `FraudosPlugin`.

## Câblage dans le pipeline — résout une question ouverte de `docs/interfaces/plugin-contract-v0.md`

`crates/plugin-sink` (`PluginSink<S: SpanSink>`) enveloppe n'importe quel
`SpanSink` : avant de déléguer à l'intérieur, passe chaque événement
converti à travers une liste de `Box<dyn Plugin>`, fusionne
`PluginOutcome.attributes` dans `extra_attributes`, et convertit chaque
avertissement en attribut `("plugin.warning", "[<nom du plugin>] <texte>")`
— même sac générique que le reste, pas de nouvelle colonne ClickHouse.
Choisi plutôt qu'un branchement dans `otlp-receiver` directement : garde
`otlp-receiver` indépendant des plugins (comme documenté dès l'étape 5), et
réutilisable avec n'importe quel `SpanSink`, pas seulement `ClickHouseSink`.

# triage-eval-plugin — premier "eval-as-plugin" (`crates/plugin-triage-eval`)

- Source faisant autorité : code réel de the-client lu par la session the-client
  (`TriageAgent.cs:35-46`, `Domain/Entities/Service.cs`,
  `generate-rich-seed.py:129-134`, `seed-dev.sql:139,169,184,199`), relayé
  avec citations exactes — pas des exemples inventés pour l'occasion.
- Date de vérification : 2026-08-17
- Portée : premier volet d'un chantier "évals sans LangSmith" initié par
  l'utilisateur the-client (échange du 2026-08-17). Volet déterministe seulement
  — le volet "jugement sémantique" (summary/compliance) reste une question
  d'architecture ouverte, voir section dédiée plus bas.

## Contexte : pourquoi ce vertical spécifique, et pas summary/compliance

3 agents the-client analysés : triage, summary, compliance. Un seul a un
référentiel canonique réel à comparer à une sortie catégorielle — c'est ce
qui permet une règle déterministe plutôt qu'un jugement sémantique.

- **Triage** (`TriageAgent.cs:35-46`) : le prompt demande un tag de
  spécialité médicale en français, avec une liste **ouverte**
  ("for example: cardiologie, pédiatrie, neurologie, biologie,
  dermatologie, gynécologie, urgence, gériatrie, médecine générale").
  Sortie post-traitée `.Trim().ToLowerInvariant()`. Mais le référentiel
  **réel** que the-client utilise ailleurs (`Domain/Entities/Service.cs`) n'a que
  6 valeurs seedées (`generate-rich-seed.py:129-134`) : `Cardiologie`,
  `Pédiatrie`, `Urgences`, `Gynécologie-Obstétrique`, `Dermatologie`,
  `Médecine générale`. **Dérive confirmée en base, pas hypothétique** :
  `seed-dev.sql:139,169,184,199` a de vraies lignes
  `Call.AiTriageTag = 'biologie'`/`'neurologie'` sans aucun `Service`
  correspondant, et `urgence` (prompt, singulier) ne correspond même pas à
  `Urgences` (Service réel, pluriel) alors que le concept existe des deux
  côtés.
- **Summary/Compliance** (`SummaryAgent.cs:33-42`, `ComplianceAgent.cs:32-42`) :
  prose libre, aucun schéma structuré, aucun référentiel à comparer — un
  résumé fidèle ou un audit RGPD correct ne se vérifient pas par une règle
  déterministe sur des attributs. Hors scope de ce plugin.

## Contrat

**Attribut d'entrée** (posé par the-client, **pas encore émis au 2026-08-17** —
lacune trouvée en scopant ce plugin, pas supposée) : `oncology.triage.tag`
(`String`), la valeur déjà post-traitée (`.Trim().ToLowerInvariant()`) que
the-client stocke aujourd'hui comme `Call.AiTriageTag` en base. Sans cet
attribut sur le span `invoke_agent`, `TriageEvalPlugin` est un no-op — même
posture que `MedicalPlugin` face à un span sans `oncology.current_step`.

**Attribut de sortie** : `eval.triage.tag_known` (`Bool`) — nouveau
namespace `eval.*`, distinct de `oncology.*` (donnée brute posée par le
client) pour séparer clairement "ce que the-client a produit" de "ce que Retina
en a déduit". Proposé par la session the-client, retenu tel quel.

**Comparaison** : `tag_known = true` si le tag (normalisé
trim+lowercase, même normalisation que the-client applique déjà) correspond
exactement à un nom de service connu (normalisé pareil). Pas de
correspondance floue/partielle — un match approximatif masquerait
silencieusement une vraie dérive de vocabulaire, l'inverse de ce que ce
plugin doit détecter.

**Avertissement** : si `tag_known = false`, un `plugin.warning` est
attaché — visible via `spans_with_warnings` (`query-api::/metrics/summary`),
même mécanisme de monitoring déjà utilisé par `MedicalPlugin`.

## Source du référentiel canonique

Décision prise avec l'utilisateur (pas seulement `MedicalPlugin`-style figé
dans le code) : `TriageEvalPlugin::new(known_services)` prend la liste en
paramètre plutôt que de la coder en dur. `crates/kernel/src/main.rs` la
résout via `TRIAGE_KNOWN_SERVICES` (comma-separated, même style que
`ENABLED_PLUGINS`) — non défini = utilise `DEFAULT_KNOWN_SERVICES` (les 6
vraies valeurs seedées de the-client aujourd'hui), pas une liste vide (une liste
vide ferait échouer tous les tags, pire qu'un défaut basé sur de la vraie
donnée). Permet de suivre l'évolution du référentiel `Service` de the-client sans
recompiler le kernel — un choix différent de `MedicalPlugin` (liste de
champs figée), justifié parce qu'un vocabulaire métier dérive dans le temps
alors qu'un schéma de gouvernance HIPAA/GDPR ne bouge pas au même rythme.

## Volet 2 (jugement sémantique summary/compliance) — tranché, pas de code côté Retina

Décision prise avec l'utilisateur (2026-08-17), après une première piste
écartée : un binaire séparé (`eval-worker`) qui irait relire les
transcripts/sorties d'agent dans ClickHouse pour appeler un juge LLM a été
envisagé, puis rejeté — `docs/interfaces/clickhouse-schema.md` établit déjà
que les attributs potentiellement sensibles sont **opt-in, désactivés par
défaut**, précisément pour ne pas stocker de donnée patient en clair dans
Retina. Le cas réel `ComplianceAgent` (nom, date de naissance, numéro de
sécu, statut VIH) rendrait ce risque concret, pas théorique, si le texte
source transitait par Retina pour être jugé.

**Décision retenue** : le juge sémantique tourne **côté client** (the-client, ou
tout futur client), avec son propre texte, sa propre clé API, son propre
budget — jamais transmis à Retina. Seul le **verdict structuré** est
posté comme attribut sur le span `invoke_agent`, namespace `eval.*` (même
que `eval.triage.tag_known` ci-dessus), valeur typée (bool/int/float),
jamais de texte libre en sortie de verdict — cohérent avec pourquoi
`ComplianceAgent` lui-même n'a pas de gate structuré aujourd'hui (dossier
`oncology-governance.md`) : un verdict en prose n'est pas interrogeable,
un verdict structuré l'est.

**Conséquence** : aucun nouveau crate/table/migration/clé API côté Retina
pour ce volet — `extra_attributes` (`Map(String, String)`, déjà générique)
absorbe `eval.summary.*`/`eval.compliance.*` exactement comme `oncology.*`
aujourd'hui. Généralise mieux qu'un `eval-worker` centralisé : pas de
couplage Retina à un fournisseur LLM ou un format de texte par client,
cohérent avec le mono-tenant actuel (ADR-0001) plutôt que d'ajouter une
responsabilité multi-client. Convention à communiquer à chaque client qui
veut l'utiliser, pas un contrat à faire évoluer côté kernel.

## Vérifié

7 tests unitaires (`crates/plugin-triage-eval`) : tag connu (cas réel
"pédiatrie" produit cette session) → `tag_known=true` sans warning ; tag en
dérive déjà vu en base the-client ("biologie") → `tag_known=false` + warning ;
`urgence` vs `Urgences` (mismatch singulier/pluriel même concept) →
`tag_known=false` ; normalisation casse/espaces ; tag absent → no-op ;
event non-`AgentRun` → no-op. Plus 2 tests sur `triage_known_services`
(`crates/kernel`) : défaut = liste réelle the-client, override par env
splitté/trimmé. **Pas encore vérifié en conditions réelles** (contrairement
à `cost_usd`) : bloqué sur `oncology.triage.tag`, toujours pas émis par
the-client au moment de l'écriture — prochaine étape une fois cet attribut posé
côté client.

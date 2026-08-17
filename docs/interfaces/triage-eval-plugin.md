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
client) pour séparer clairement "ce que the-client a produit" de "ce que trellis
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

## Volet 2 (jugement sémantique summary/compliance) — question ouverte, pas attaquée ici

`docs/interfaces/plugin-contract-v0.md` : un plugin est synchrone, sans
I/O, borné à un timeout court (`crates/plugin-sink`, `spawn_blocking` +
100ms — généreux pour une règle sur des attributs, pas pour un appel
réseau). Un juge LLM (`summary` fidèle au transcript ? `compliance` audit
correct ?) prend des secondes et fait du réseau — incompatible avec ce
contrat tel quel. Où et quand ce jugement tournerait (à l'ingestion en
async détaché du chemin critique, en tâche batch séparée hors kernel, à la
demande via un futur endpoint `query-api`) reste à trancher avec
l'utilisateur avant tout code — décision à impact architecture
significatif, pas prise unilatéralement ici (même posture que le choix
cloud/multi-tenant, CLAUDE.md).

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

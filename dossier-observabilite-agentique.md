# Dossier de conception — Système d'observabilité agentique hybride

**Statut** : phase de cadrage, prêt pour démarrage du kernel MVP.
**Destinataire** : agent de développement (SaaS factory).
**Objectif de ce dossier** : donner à l'agent tout le contexte de décision nécessaire pour démarrer le kernel sans repartir de zéro sur les choix déjà tranchés.

---

## 1. Vision du produit

Système hybride d'observabilité inspiré d'OpenTelemetry/Datadog, écrit en Rust, avec deux axes combinés :

- **Axe A — Observabilité pour systèmes agentiques** : tracer/monitorer des workflows d'agents LLM eux-mêmes (appels de modèle, appels d'outils, chaînes de raisonnement, coûts token, latence par étape), avec une couche d'analyse qui utilise elle-même des agents IA pour la corrélation et le diagnostic (root cause analysis, détection d'anomalies).
- **Axe C — Plugins métiers directement utilisables** : un cœur générique (kernel) + des plugins spécialisés par domaine (fintech en premier, puis extension à d'autres verticaux) qui savent interpréter des métriques et attributs métier spécifiques.

Déploiement cible en production : GCP ou Azure (arbitrage non encore tranché — voir section 6).

---

## 2. Décisions d'architecture validées

### 2.1 Modèle de données

- **Alignement sur les conventions sémantiques OTel GenAI**, mais **verrouillées à une version précise** (commit ou tag donné dans le code), car ces conventions restent en statut *Development* et évoluent encore sur `main` dans le repo dédié `open-telemetry/semantic-conventions-genai`. Ne pas suivre `main` en continu.
- Schéma en deux couches obligatoire :
  1. **Attributs génériques `gen_ai.*`** (communs à tous les providers/frameworks).
  2. **Attributs spécifiques par provider** (ex. les extensions Bedrock où `gen_ai.provider.name = "aws.bedrock"`), stockés séparément (table ou colonne JSON) pour ne pas forcer une normalisation qui perdrait l'info spécifique.
- Portée du modèle pour le MVP : ne couvrir que 3–4 types d'événements essentiels au départ — appel modèle, appel outil, run d'agent. Le reste (retrieval, mémoire, etc.) attend l'incrément suivant.
- Une couche de mapping/adaptateur doit séparer "ce qui arrive en OTLP" de "ce que le kernel stocke en interne", pour isoler l'impact d'une évolution du schéma amont OTel.
- Ne pas capturer le contenu des prompts/réponses par défaut (principe PII d'OTel GenAI) — à activer explicitement si besoin, avec les implications de conformité que ça implique.

### 2.2 Pipeline (kernel MVP)

Ordre de construction retenu, chaque étape testable indépendamment :

1. **Modèle de données verrouillé** (voir 2.1).
2. **Ingestion OTLP minimale** — receiver gRPC/HTTP en Rust (`tonic`), qui accepte, valide et persiste brut. Pas de batching avancé ni de sampling intelligent à ce stade.
3. **Stockage à instance unique** — un seul backend pour commencer (ClickHouse auto-hébergé ou BigQuery managé sur GCP / Azure Data Explorer sur Azure). Pas de multi-tenancy ni de rétention fine au MVP.
4. **API de requête minimale** — 2-3 endpoints : lister les traces récentes, récupérer l'arbre d'une trace, agréger quelques métriques de base. Pas de dashboard riche nécessaire pour valider le kernel.
5. **Contrat de plugin v0** — trait Rust (ou interface WASM via `wasmtime`) que les plugins métiers devront implémenter. Écrire un plugin factice pour valider le contrat avant d'anticiper les besoins de verticaux non encore rencontrés.
6. **Déploiement squelette sur un seul cloud** — une seule région, pas de haute disponibilité. Objectif : pipeline CI/CD fonctionnel via la SaaS factory, pas une infra de prod robuste.
7. **Boucle de validation** — faire tourner le kernel contre un vrai flux de télémétrie (voir section 3, cas fraudos).

### 2.3 Couche d'analyse agentique

- Les agents d'analyse (RCA, détection d'anomalies) génèrent eux-mêmes des traces LLM lors de leur fonctionnement : prévoir dès le départ que cette couche s'auto-instrumente via le même pipeline (méta-observabilité), pour ne pas être aveugle sur son propre système de diagnostic.
- Cette couche n'est **pas** dans le périmètre du kernel MVP — elle vient après validation du pipeline de base.

### 2.4 Architecture de plugins

- Cœur en traits Rust ; modules WASM (`wasmtime`) envisagés pour isoler les plugins spécifiques à un client/vertical, permettre un chargement dynamique sans recompiler le core.
- Ne pas sur-designer le contrat de plugin à l'avance : le laisser être informé par le premier vertical réel (fintech/fraude, voir section 3).

---

## 3. Premier vertical : fintech (fraudos)

- Un prototype existant (**fraudos**) tourne déjà sur **AWS Bedrock**, avec une instrumentation botocore + ADOT (AWS Distro for OpenTelemetry).
- **Point clé** : Bedrock via ADOT émet déjà nativement les conventions OTel GenAI (`gen_ai.*`, avec extensions Bedrock spécifiques). Il n'est **pas nécessaire de construire un adaptateur CloudWatch → OTLP** — CloudWatch n'est qu'une des destinations possibles de cette instrumentation, pas le seul chemin.
- **Action d'intégration recommandée** : repointer l'exporteur OTLP d'ADOT vers l'endpoint d'ingestion du kernel (variable d'environnement côté ADOT), en parallèle de CloudWatch pendant la transition si besoin de comparaison.
- **Attributs spécifiques au domaine fraude à prévoir dans le plugin fintech** :
  - Identifiant de transaction pour corréler la décision du modèle avec l'issue réelle (fraude confirmée ou non) — l'issue arrive souvent après l'inférence, donc prévoir un mécanisme de mise à jour a posteriori d'une trace existante.
  - Score de risque et seuil de décision comme attributs de premier ordre, pas noyés dans le blob de sortie du modèle.
  - Contraintes de rétention/immutabilité pour l'auditabilité réglementaire des décisions automatisées — impacte potentiellement le choix de stockage, pas seulement le plugin.
- Ce cas fraudos sert de **cas de validation concret pour l'étape 7 du kernel** (section 2.2) : brancher directement, sans couche intermédiaire à construire.

---

## 4. Ce que le kernel MVP ne doit PAS couvrir (hors périmètre volontaire)

- Multi-tenancy.
- Haute disponibilité / multi-région.
- Couverture exhaustive des conventions OTel GenAI (retrieval, mémoire, etc.).
- Couche d'analyse agentique (RCA, anomalies).
- Dashboard riche.
- Multi-cloud simultané.

---

## 5. Questions ouvertes à trancher avec l'agent / en cours de route

- Mono-tenant pour valider un vertical, ou architecture multi-tenant visée dès la conception du stockage et de l'isolation des plugins ?
- GCP vs Azure pour la prod (arbitrage encore ouvert — GKE a un écosystème Kubernetes plus mature pour ce type de charge, AKS s'intègre mieux si les clients cibles sont déjà dans l'écosystème Microsoft/365 ; Pub/Sub vs Event Hubs pour le découplage ingestion/traitement).
- ClickHouse auto-hébergé vs stockage managé (BigQuery/ADX) — arbitrage à trancher selon volume attendu (repère : en dessous de ~100k requêtes/jour, le managé gagne presque toujours en coût total ; au-dessus de ~1M/jour, l'auto-hébergé devient intéressant si la bande passante ops suit).
- Modalité exacte du chargement de plugins : trait Rust compilé statiquement vs modules WASM chargés dynamiquement.

---

## 6. Repères techniques (Rust)

- Ingestion OTLP : `tonic` (gRPC) + `prost`.
- Plugins isolés : `wasmtime`.
- Stockage : driver ClickHouse Rust, ou SDK cloud correspondant (BigQuery/ADX) selon arbitrage.
- Le SDK Rust d'OpenTelemetry existant peut servir de référence d'implémentation pour la conformité au protocole OTLP, même si le kernel ne le réutilise pas tel quel.

---

*Ce dossier reflète l'état des décisions prises en amont de la conversation de cadrage. Toute décision listée en section 5 doit être tranchée avant ou pendant la construction de l'étape correspondante du kernel.*

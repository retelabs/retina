# cost-model — sources de prix et méthode (`crates/cost-model`)

Pas un contrat technique comme `docs/interfaces/` (pas d'API, pas de
protocole) — mais la même discipline : chaque chiffre porte sa source et sa
date, rien n'est deviné. Deuxième chantier "apprentissage cloud" (dossier
section 5), après `crates/orchestrator` — objectif : rendre le critère
posé le 2026-08-15 (comparer par le **coût à usage zéro**) vérifiable avec
de vrais nombres, pas seulement affirmé.

## Octets par span : mesuré, pas estimé

Une estimation analytique depuis les types de colonnes
(`docs/interfaces/clickhouse-schema.md`) ignorerait la compression réelle
de ClickHouse (`LowCardinality`, compression de colonne par défaut) et
surestimerait largement. `crates/cost-model/src/measure.rs` interroge
`system.parts` (`sum(data_compressed_bytes) / sum(rows)`, filtré sur la
table `spans`, parts actives) contre un vrai serveur — colonnes confirmées
via `DESCRIBE TABLE system.parts` avant d'écrire la requête, pas devinées.

Mesuré le 2026-08-16 contre 15 spans réels (rejeu complet des fixtures
`fraudos-replay` + `oncology-replay`, après `OPTIMIZE TABLE spans FINAL`
pour un état de compression représentatif plutôt que des parts fraîchement
insérées et pas encore fusionnées) : **≈304 octets/span compressés**.
Échantillon volontairement petit (les seules données réelles disponibles
dans cet environnement) — un ordre de grandeur vérifié, pas une moyenne
statistiquement solide. `cargo run -p cost-model` remesure à chaque
lancement contre l'instance ClickHouse réellement connectée, ce nombre
n'est pas figé dans le code.

## Prix vérifiés le 2026-08-16 contre les pages officielles

Une première recherche via des sites d'agrégation a donné des chiffres
contradictoires pour Hetzner (entre 3,79 € et 5,49 €/mois selon la source)
— écartés au profit de la documentation officielle directement.

- **VM — Hetzner Cloud CX23** (2 vCPU / 4 Go RAM / 40 Go disque inclus),
  5,49 €/mois hors IPv4 et hors TVA. Source :
  docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/.
  Le plan s'appelait CX22 avant l'ajustement de prix de mi-2026 qui l'a
  renommé CX23 — exactement le genre de dérive qui justifie de revérifier
  avant toute décision réelle, pas de faire confiance à un chiffre lu une
  fois.
- **Stockage objet (backups ClickHouse) — Backblaze B2** : 6,95 $/To/mois
  (≈0,00695 $/Go/mois), 10 Go gratuits, egress gratuit jusqu'à 3x le
  stockage puis 0,01 $/Go au-delà. Source :
  backblaze.com/cloud-storage/pricing. L'egress n'est pas modélisé ici —
  ce chantier ne calcule que le coût de stockage.
- **CDN/edge — Cloudflare** : plan gratuit, CDN non mesuré (contrairement
  aux Workers/compute, qui eux le sont) — 0€ à tout volume réaliste pour ce
  projet, pas seulement à volume zéro. Source : cloudflare.com/plans.
- **Registre de conteneurs — GitLab Container Registry** : déjà en place
  (`.gitlab-ci.yml`), pas un nouveau prix à vérifier.

## Ce que le modèle calcule

Pour un volume donné (spans/jour), à l'état stationnaire de la fenêtre de
rétention réelle (90 jours,
`crates/clickhouse-sink/migrations/0002_spans_retention_ttl.sql`) :

- Coût VM — identique entre "100% perso" et "hybride", domine tant que le
  volume reste faible (voir `cargo run -p cost-model` sans argument, qui
  imprime les repères `0`/`1 000`/`100 000`/`1 000 000` spans/jour —
  `100 000` est le repère du dossier pour l'arbitrage ClickHouse managé vs
  auto-hébergé).
- Jours avant de dépasser le disque inclus de la VM à ce volume — dérivé du
  même octets/span mesuré, une vraie question opérationnelle qu'on peut
  maintenant chiffrer plutôt que deviner.
- Coût du seul ajout hybride dont le prix dépend du volume (stockage
  objet) — `0$` en dessous du palier gratuit Backblaze, ce qui couvre
  largement les volumes réalistes pour ce projet aujourd'hui.

Vérifié à deux niveaux : tests unitaires sur `report.rs` (fonctions pures,
sans Docker ni ClickHouse — volume zéro annule les extras hybrides,
croissance monotone, dépassement du disque inclus détecté) ; et un test
`--ignored` qui mesure réellement contre un vrai ClickHouse
(`crates/cost-model/tests/integration.rs`) confirmant un résultat positif
et plausible, pas seulement que la requête ne plante pas.

## Ce que ça ne couvre pas

- L'egress (téléchargement) du stockage objet — au-delà de 3x le stockage
  chez Backblaze, non modélisé.
- Le coût de calcul (CPU/bande passante) réellement consommé par
  `kernel`/`query-api`/`orchestrator` eux-mêmes — la VM est traitée comme
  un coût fixe unique, pas décomposée par service.
- D'autres fournisseurs VM (OVH, Scaleway, DigitalOcean, mentionnés lors de
  la discussion du 2026-08-15) — Hetzner choisi comme premier point de
  repère vérifié, pas comme décision finale de fournisseur.

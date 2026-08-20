# 0002. Modèle d'hébergement : auto-hébergé sur hardware nu, fournisseur différé

Date : 2026-08-16 (discussion et décision réparties du 2026-08-14 au
2026-08-16 ; formalisée ici)

Statut : Accepté pour le principe — le choix du fournisseur n'est **pas**
tranché par cette ADR, volontairement.

## Contexte

Le dossier de conception (section 5) posait "GCP vs Azure pour la prod ?".
La discussion avec l'utilisateur a fait dévier la question dès le
2026-08-14 : l'enjeu réel n'était pas le logo du fournisseur mais acquérir,
en codant en direct, les concepts et la méthode pour construire — et
chiffrer — ses propres briques d'infrastructure plutôt que de consommer des
services managés tout faits. Critère précisé le 2026-08-15 : le rejet des
services managés n'est pas catégorique, c'est spécifiquement le coût
**facturé indépendamment de l'usage** qui doit être évité (frais de control
plane Kubernetes managé même à zéro pod, bases managées facturées à
l'instance provisionnée, capacité réservée à l'heure).

## Décision

Construire sur du hardware nu (une VM/VPS) plus Docker, avec un control
plane maison (`crates/orchestrator`) plutôt que des services cloud managés
spécifiques à un fournisseur. Critère explicite pour toute brique candidate
à l'ajout : calculer son **coût à usage zéro** avant de la retenir
(`crates/cost-model`, pas deviné) ; préférer la coder soi-même plutôt que
la consommer quand la valeur d'apprentissage le justifie, le coût
d'ingénierie tranchant seulement quand il dépasse largement ce que ça
enseigne.

Le fournisseur d'hébergement lui-même **n'est pas tranché ici** — Hetzner
sert de repère vérifié dans `crates/cost-model` (prix officiel confirmé,
pas un choix de fournisseur), OVH/Scaleway/DigitalOcean ont été mentionnés
sans être évalués.

## Conséquences

Les images Docker (`docker/kernel.Dockerfile`, `docker/query-api.Dockerfile`)
et `crates/orchestrator` sont déjà portables vers n'importe quel fournisseur
de VM — aucun changement de code nécessaire au moment de choisir, seul le
provisionnement en dépendra (cohérent avec l'intention posée dès l'étape 6
du kernel). Revers assumé : pas de scaling automatique, pas de haute
disponibilité managée, pas de support fournisseur — tout ce qu'un cloud
managé offrirait est soit à construire soi-même, soit accepté comme absent
(cohérent avec le hors-périmètre MVP existant : HA/multi-région, dossier
section 4).

## Alternatives considérées

- **GKE/AKS** (Kubernetes managé) — écartés : frais de control plane
  facturés même à zéro pod, contraire au critère de coût à usage zéro.
- **ClickHouse Cloud / bases managées à capacité provisionnée** — écartées
  pour la même raison (détail : ADR-0003).
- **Rester 100% sur des services pay-per-use gérés par des tiers**, sans
  rien construire soi-même — écarté : contraire à l'objectif d'apprentissage
  explicitement posé par l'utilisateur, qui est la vraie motivation de
  cette ADR, pas un choix de coût pur.

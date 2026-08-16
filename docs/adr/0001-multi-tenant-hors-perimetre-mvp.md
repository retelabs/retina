# 0001. Multi-tenant hors périmètre du MVP

Date : 2026-08-16 (rédigée rétroactivement — la décision elle-même remonte
au tout début du projet, dossier section 4 ; formalisée ici en ADR pour
combler l'écart entre l'outillage `/adr` prévu et son usage réel jusqu'ici)

Statut : Accepté

## Contexte

Le dossier de conception (section 5) posait la question dès le départ :
"mono-tenant pour valider un vertical, ou conception multi-tenant visée dès
la conception du stockage et de l'isolation des plugins ?" Le kernel devait
être construit et validé rapidement contre un vertical réel (fraudos,
étape 7), sans détourner l'effort vers une architecture multi-tenant dont
le besoin n'était — et n'est toujours — pas démontré par un second client
ou tenant réel.

## Décision

Mono-tenant pour tout le MVP (dossier section 4, explicitement listé "hors
périmètre volontaire"). Aucune isolation de tenant dans le schéma de
stockage (table `spans`, pas de colonne `tenant_id`), aucune segmentation
dans l'exécution des plugins (`crates/plugin-sink` traite tous les
événements de la même façon), un seul jeu de secrets d'authentification par
surface (`KERNEL_API_KEY`/`QUERY_API_KEY`/`ORCHESTRATOR_API_KEY`, pas un
jeton par tenant).

## Conséquences

Simplifie tout ce qui a été construit depuis : le schéma ClickHouse, une
authentification à secret partagé statique plutôt qu'un vrai système
d'identité (JWT/OAuth), une seule instance kernel par déploiement. Revers
assumé : migrer vers du multi-tenant plus tard demanderait de revoir le
schéma de stockage (colonne de partition tenant + `ORDER BY`),
l'authentification (un jeton par tenant ou une vraie infra d'identité), et
potentiellement l'isolation des plugins (un plugin ne doit pas voir les
données d'un autre tenant). Ce n'est pas un mur — c'est un coût différé
assumé consciemment, pas découvert après coup.

## Alternatives considérées

Concevoir le stockage et l'authentification multi-tenant dès le départ —
écarté : aucun second client/tenant n'existe encore, et deviner la bonne
forme d'isolation sans un second cas d'usage réel aurait probablement
produit une abstraction fausse — le risque classique de sur-concevoir avant
d'avoir un deuxième exemple concret pour la valider.

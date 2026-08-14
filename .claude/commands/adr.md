---
description: Crée une nouvelle Architecture Decision Record pour trancher une question ouverte ou toute décision structurante.
argument-hint: <titre court de la décision>
---

Crée `docs/adr/NNNN-<slug>.md` (numéro suivant, calculé à partir du plus
haut fichier existant dans `docs/adr/`) en partant du gabarit
`docs/adr/0000-template.md`, pour la décision : "$ARGUMENTS".

Avant de proposer une décision :
- Relis `docs/adr/README.md` et la section 5 du dossier de conception si la
  question y figure déjà, pour repartir des repères déjà donnés (ex. seuils
  volumétriques ClickHouse vs managé, section 5).
- Si la décision touche une interface externe, vérifie d'abord son contrat
  via `/contract` plutôt que de supposer son comportement.
- Ne tranche pas seul une question à impact produit/coût significatif
  (choix de cloud, multi-tenant) : présente les options avec leurs
  trade-offs dans l'ADR au statut "Proposé", et demande confirmation avant
  de la passer à "Accepté".

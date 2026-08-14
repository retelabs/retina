---
description: Avancement du kernel MVP par rapport aux 7 étapes du dossier de conception (section 2.2).
---

Compare l'état actuel du repo (fichiers, code, tests, fiches de contrat) aux
7 étapes de la section 2.2 de `dossier-observabilite-agentique.md` :

1. Modèle de données verrouillé
2. Ingestion OTLP minimale
3. Stockage à instance unique
4. API de requête minimale
5. Contrat de plugin v0
6. Déploiement squelette
7. Boucle de validation (cas fraudos, section 3)

Pour chaque étape, indique : fait / en cours / pas commencé, avec les
fichiers ou commits qui le montrent. Signale :
- toute étape entamée hors ordre par rapport à la séquence retenue ;
- toute étape déjà entamée qui touche une frontière externe sans fiche
  correspondante dans `docs/interfaces/` (utilise `/contract` pour combler) ;
- les pins `vendor/` manquants ou absents via `scripts/check-pins.sh` si
  l'étape 1 ou 2 est en cours.

Termine par une recommandation courte sur la prochaine action concrète.

---
description: Vérifie et documente le contrat précis d'une interface externe avant d'écrire du code dessus.
argument-hint: <sujet: otlp-ingestion | semconv-genai | bedrock-adot-export | clickhouse-schema | wasm-plugin-trait-v0 | ...>
---

Avant d'écrire le moindre code touchant à l'interface "$ARGUMENTS", applique
la règle permanente du projet (voir CLAUDE.md) :

1. Identifie la ou les sources faisant autorité pour ce contrat (spec
   officielle, repo pinné dans `vendor/`, doc du SDK/crate utilisé, doc
   AWS/ADOT, etc.) — jamais la mémoire ou une supposition.
2. Si la source est une spec externe versionnée censée être épinglée
   (semconv GenAI, proto OTLP), lance `scripts/check-pins.sh`. Si le pin
   manque ou est absent, pose-le avec `scripts/pin-semconv.sh <ref>` ou
   `scripts/pin-otlp-proto.sh <ref>` avant de continuer — choisis une ref
   précise (tag stable ou commit), jamais `main`.
3. Lis la doc réelle (le contenu vendoré ou la doc officielle, pas un
   résumé de mémoire) : noms de champs exacts, types, obligatoire/optionnel,
   valeurs par défaut, statut (stable vs development), cas limites.
4. Écris ou mets à jour `docs/interfaces/<sujet>.md` selon le format décrit
   dans `docs/interfaces/README.md` : source, version/commit pinné, champs
   utilisés par le kernel, ce qui est ignoré et pourquoi, incertitudes
   restantes.
5. Seulement ensuite, propose ou écris le code d'intégration correspondant.

Ne saute aucune étape même si le sujet paraît familier.

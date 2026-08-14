# Fiches de contrat d'interface

Chaque frontière externe ou inter-composant du kernel a une fiche ici,
écrite **après lecture de la doc réelle** (pas de mémoire, pas de
supposition) et **avant** d'écrire le code qui l'implémente. C'est
l'application concrète de la règle permanente du projet (voir
[CLAUDE.md](../../CLAUDE.md)).

Utilise `/contract <sujet>` pour produire une fiche.

## Format d'une fiche

```markdown
# <sujet>

- Source faisant autorité : <repo/spec/doc, avec URL>
- Version/commit pinné : <ref exacte, ou "N/A" si spec non versionnée>
- Date de vérification : <YYYY-MM-DD>

## Champs/comportements utilisés par le kernel

<liste précise : nom exact, type, obligatoire/optionnel, valeur par défaut>

## Ignoré volontairement (et pourquoi)

## Incertitudes restantes / à revalider avant prod
```

## Sujets attendus (au fil de l'avancement du kernel MVP)

- `otlp-ingestion.md` — format des requêtes gRPC/HTTP acceptées par le
  receiver (étape 2 du kernel).
- `semconv-genai.md` — attributs `gen_ai.*` retenus pour le MVP (étape 1).
- `bedrock-adot-export.md` — attributs spécifiques Bedrock émis par ADOT,
  et configuration de repointage de l'exporteur OTLP (section 3 du dossier).
- `clickhouse-schema.md` — schéma de stockage et garanties d'écriture
  (étape 3).
- `wasm-plugin-trait-v0.md` — contrat du trait de plugin, Rust natif et/ou
  ABI WASM `wasmtime` (étape 5).

Cette liste n'est pas figée : ajoute une fiche pour toute autre frontière
rencontrée en cours de route.

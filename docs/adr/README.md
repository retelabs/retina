# Architecture Decision Records

Une ADR par décision structurante, notamment les questions ouvertes listées
en section 5 de
[dossier-observabilite-agentique.md](../../dossier-observabilite-agentique.md).

## Index

| ADR | Décision | Statut |
|---|---|---|
| [0001](0001-multi-tenant-hors-perimetre-mvp.md) | Multi-tenant hors périmètre du MVP | Accepté |
| [0002](0002-modele-hebergement.md) | Modèle d'hébergement : auto-hébergé sur hardware nu, fournisseur différé | Accepté (principe) — fournisseur non tranché |
| [0003](0003-clickhouse-auto-heberge.md) | ClickHouse auto-hébergé, pas de service managé | Accepté |
| [0004](0004-chargement-plugins-natif.md) | Chargement de plugins : trait Rust natif, WASM différé | Accepté (mode natif) — WASM différé, pas rejeté |

Les 4 rédigées le 2026-08-16, rétroactivement — les décisions elles-mêmes
avaient déjà été prises et enactées en code au fil du projet (voir
`CLAUDE.md` pour le détail chronologique de chacune), pas de nouvelle
décision tranchée par cet exercice de rédaction.

Utilise `/adr <titre>` pour créer une nouvelle entrée à partir de
[0000-template.md](0000-template.md). Numérote séquentiellement
(`0005-...`, ...). Les décisions à impact produit/coût significatif
(cloud cible, multi-tenant) se présentent avec leurs trade-offs et attendent
confirmation avant d'être marquées "Accepté".

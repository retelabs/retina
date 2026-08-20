# 0004. Chargement de plugins : trait Rust natif, WASM dynamique différé

Date : 2026-08-16 (exploration WASM faite le 2026-08-15 ; décision de ne
pas l'intégrer pour l'instant formalisée ici)

Statut : Accepté pour le mode natif actuel — le chargement dynamique WASM
n'est pas rejeté, seulement différé faute de besoin démontré.

## Contexte

Le dossier de conception (section 5) posait "trait Rust compilé
statiquement vs modules WASM chargés dynamiquement ?" et demandait
explicitement de ne pas sur-designer le contrat de plugin à l'avance, en le
laissant informé par le premier vertical réel (section 2.4). Les deux
verticaux réels (`plugin-fraudos`, `plugin-medical`) ont été écrits comme
des types Rust compilés dans le binaire `kernel` dès leur création. Une
exploration WASM complète a ensuite été menée (`crates/plugin-wasm-wire`,
`crates/plugin-wasm-example`, `crates/plugin-wasm-host`,
`docs/interfaces/wasm-plugin-loading.md`) et a prouvé, par un test qui
compare bit à bit la sortie du plugin WASM et du plugin natif équivalent,
l'équivalence comportementale des deux approches — mais cette exploration
n'a jamais été branchée dans le pipeline réel : `crates/plugin-sink` ne
charge que des `Box<dyn Plugin>` natifs (`FraudosPlugin`, `MedicalPlugin`).

## Décision

Les plugins tournant réellement dans `kernel` restent des types Rust
compilés statiquement, activables/désactivables par configuration
(`ENABLED_PLUGINS`, `crates/kernel/src/main.rs` — décision distincte,
documentée dans `CLAUDE.md`). Le chargement dynamique de code WASM tiers
(`crates/plugin-wasm-host`) n'est pas adopté dans le pipeline réel pour
l'instant.

## Conséquences

Simplicité et confiance : un plugin natif est du code interne de confiance,
pas besoin de l'isoler contre un panic ou une boucle infinie malveillante
au-delà de ce que `crates/plugin-sink` fait déjà pour un tout autre motif
(isolation panic/timeout de tout plugin, natif compris, ajoutée le
2026-08-15). Revers assumé : ajouter un nouveau vertical métier nécessite
de recompiler le kernel — pas de marketplace de plugins tiers possible sans
reprendre ce chantier. La question de la gestion d'un plugin WASM tiers qui
panique ou boucle indéfiniment (un vrai risque pour du code non fiable,
contrairement au code interne actuel) reste explicitement non tranchée si
ce chantier est repris (`docs/interfaces/plugin-contract-v0.md`).

## Alternatives considérées

Charger `crates/plugin-wasm-host` dans le pipeline réel dès maintenant —
écarté : aucun besoin réel de plugin tiers ou chargé dynamiquement ne s'est
présenté (seulement 2 verticaux à ce jour, tous deux internes et de
confiance). L'exploration reste disponible et vérifiée équivalente au
natif si ce besoin apparaît — un investissement qui a déjà payé en
compréhension, pas une impasse.

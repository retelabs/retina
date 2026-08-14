# plugin-contract-v0 — contrat de plugin métier (étape 5 du kernel)

- Source faisant autorité : conception propre à ce projet (dossier section
  2.2 étape 5, section 2.4) — documentée ici parce que c'est le contrat que
  devra implémenter tout futur plugin vertical (fintech en premier, dossier
  section 3), donc un vrai point d'interopérabilité même sans spec externe.
- Date de vérification : 2026-08-14
- Crates : `crates/plugin-api` (le contrat), `crates/plugin-example` (le
  "plugin factice" que le dossier demande d'écrire pour valider le contrat
  avant d'anticiper les besoins d'un vertical réel).

## Ce que fixe ce contrat, et ce qu'il ne fixe pas

Le dossier (section 2.4) est explicite : *"Ne pas sur-designer le contrat de
plugin à l'avance : le laisser être informé par le premier vertical réel."*
En conséquence, `plugin-api` ne fixe que le **contrat d'interprétation** —
comment un plugin inspecte un événement du kernel et ce qu'il peut renvoyer.
Il ne fixe **pas** :

- **La modalité de chargement** — trait Rust compilé statiquement vs module
  WASM chargé dynamiquement via `wasmtime` reste une question ouverte
  (dossier section 5). Le trait `Plugin` est volontairement *object-safe*
  (`Vec<Box<dyn Plugin>>` fonctionne, testé) pour ne fermer aucune des deux
  portes, mais aucun mécanisme de chargement n'est implémenté ici.
- **Le câblage dans le pipeline** — `plugin-api` ne dépend que de
  `kernel-model`, pas de `otlp-receiver` ni `clickhouse-sink`. Rien n'invoque
  encore un plugin depuis `TraceService::export` ou `ClickHouseSink`. Le
  câblage réel est laissé à l'étape 7 (validation contre le cas fraudos), où
  le premier vrai vertical dictera où et comment l'insérer dans le pipeline
  plutôt que de deviner maintenant.
- **Les attributs fintech eux-mêmes** (`transaction_id`, score de risque,
  seuil de décision — dossier section 3) ne sont pas dans ce contrat. Le
  plugin factice (`ExamplePlugin`) est délibérément générique/jouet, pas une
  ébauche du plugin fintech.

## Le contrat

```rust
pub enum KernelEvent<'a> {
    ModelCall(&'a ModelCallEvent),
    ToolCall(&'a ToolCallEvent),
    AgentRun(&'a AgentRunEvent),
}

pub struct PluginOutcome {
    pub attributes: Vec<Attribute>,   // (String, AttributeValue) à rattacher
    pub warnings: Vec<String>,
}

pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn inspect(&self, event: KernelEvent<'_>) -> PluginOutcome;
}
```

- `KernelEvent` emprunte les 3 structs `kernel-model` — pas de copie, pas de
  format intermédiaire.
- `PluginOutcome` est **infaillible** : pas de variante de rejet dur d'un
  événement. Donner à un plugin le pouvoir de casser l'ingestion est un
  engagement plus lourd que ce qu'un plugin factice v0 doit prouver ; plus
  facile à ajouter plus tard qu'à retirer si on le pose trop tôt.
- `inspect` est synchrone. Pas encore de besoin d'I/O externe identifié pour
  un plugin v0 ; à revisiter si un vertical réel en a besoin (ça toucherait
  aussi la question WASM — les appels hôte `wasmtime` ont leurs propres
  contraintes d'asynchronicité, à vérifier précisément le moment venu plutôt
  que supposé maintenant).

## `ExamplePlugin` — ce que le plugin factice valide

- Calcule `example.total_tokens` (somme input+output) pour `ModelCallEvent`
  et `AgentRunEvent` quand les deux comptes sont présents — prouve que le
  contrat permet de produire un attribut dérivé cohérent avec le modèle
  interne (`AttributeValue::Int`, conversion `u64 -> i64` vérifiée, pas de
  cast nu, même réflexe que partout ailleurs dans le kernel).
- Avertit quand un `ToolCallEvent` n'a pas de `tool_call_id` — prouve que le
  contrat permet une validation métier sans bloquer l'ingestion.
- 4 tests couvrent les 3 variantes de `KernelEvent` et le cas "champ manquant
  → pas d'attribut produit".

## Incertitudes / décisions à prendre plus tard, pas maintenant

- Chargement WASM (`wasmtime`) — dossier section 5, toujours ouvert.
- Où insérer l'appel plugin dans le pipeline (avant/après stockage ? avant
  ou après validation OTLP ?) — dépend du premier vertical réel (étape 7).
- Gestion d'erreur si un plugin panique ou boucle (pertinent surtout pour du
  code WASM tiers non fiable) — pas pertinent tant que les seuls plugins sont
  internes et de confiance (le plugin factice).

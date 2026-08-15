# wasm-plugin-loading — chargement dynamique de plugins via `wasmtime` (v0)

- Source faisant autorité : https://docs.rs/wasmtime (crate `wasmtime`, v47.0.3,
  vérifié via docs.rs) — c'est la question ouverte du dossier section 5
  ("modalité exacte du chargement de plugins : trait Rust compilé
  statiquement vs modules WASM chargés dynamiquement"), qu'on explore ici
  sans pour autant la trancher définitivement (voir plus bas).
- Date de vérification : 2026-08-15
- Portée : un mécanisme de chargement WASM v0, à côté du contrat trait Rust
  déjà validé (`docs/interfaces/plugin-contract-v0.md`) — pas un remplacement.

## Décision : module WASM "core" (ABI maison), pas le Component Model

`wasmtime` supporte deux façons de structurer une interface :

1. **Component Model** (WIT + `wit-bindgen`) — bindings typés générés,
   approche "moderne" recommandée par le projet wasmtime pour du nouveau
   code, mais nécessite l'outillage `cargo-component`/`wasm-tools` (build
   d'un `.wasm` en composant, pas juste `cargo build --target
   wasm32-unknown-unknown`). **Aucun des deux outils n'est installé dans cet
   environnement**, et les installer ajoute une dépendance d'outillage
   nouvelle avant même de savoir si l'approche WASM est retenue durablement.
2. **Module "core"** — `Engine`/`Module::from_file`/`Store`/`Linker`/
   `Instance::get_typed_func`, avec un ABI mémoire linéaire fait main
   (passer des pointeurs + longueurs). Ne nécessite que
   `rustup target add wasm32-unknown-unknown` — déjà dans les targets
   installables par défaut (vérifié : `rustup target list`).

**Retenu pour ce v0 : l'option 2.** Cohérent avec la mise en garde du
dossier (section 2.4) contre le sur-design du contrat de plugin avant qu'un
vrai vertical n'en ait besoin — on valide ici le *mécanisme* (chargement
dynamique d'un `.wasm` sans recompiler le core), pas une interface figée à
long terme. Si l'approche WASM est confirmée utile, migrer vers le Component
Model plus tard est un changement d'outillage, pas de conception — les DTO
`plugin-wasm-wire` définis ici resteraient valables.

## API `wasmtime` vérifiée (v47.0.3)

```rust
let engine = Engine::default();
let module = Module::from_file(&engine, "plugin.wasm")?;
let mut store: Store<()> = Store::new(&engine, ());
let linker = Linker::new(&engine);
let instance = linker.instantiate(&mut store, &module)?;

let memory = instance.get_memory(&mut store, "memory").ok_or(...)?;
memory.data_mut(&mut store)[ptr..ptr+len].copy_from_slice(bytes); // écrire

let alloc = instance.get_typed_func::<i32, i32>(&mut store, "alloc")?;
let process = instance.get_typed_func::<(i32, i32), i64>(&mut store, "process")?;
```

## ABI retenu (mémoire linéaire, JSON)

Le guest exporte 2 fonctions et sa mémoire linéaire :

- `alloc(len: i32) -> i32` — le guest alloue `len` octets dans SA mémoire et
  retourne le pointeur ; l'hôte y écrit ensuite les octets d'entrée. Le
  guest est propriétaire de l'allocation, pas l'hôte — évite d'avoir à
  exposer un allocateur hôte au guest.
- `process(ptr: i32, len: i32) -> i64` — le guest lit `len` octets JSON à
  `ptr` (un `WireKernelEvent`, `crates/plugin-wasm-wire`), calcule un
  `WirePluginOutcome`, l'écrit en JSON dans sa propre mémoire (via un second
  `alloc`), et retourne `(ptr_sortie << 32) | len_sortie` empaqueté dans un
  seul `i64`.

  **Essayé d'abord, écarté** : faire retourner `process` un tuple Rust
  `(i32, i32)` via `extern "C"` pour profiter du retour multi-valeur wasm
  nativement. Ça compile, mais `rustc` avertit explicitement `improper_ctypes_definitions`
  — *"tuples have unspecified layout"* : le mapping tuple → valeurs de
  retour wasm n'est pas une garantie du langage, juste un comportement
  actuel du compilateur. Vérifié en compilant un cas isolé avant de le
  mettre dans le contrat plutôt que de s'appuyer dessus. L'empaquetage
  manuel dans un `i64` n'a aucune ambiguïté de layout.

**Format des données : JSON** (`serde_json`), pas un format binaire
compact — lisibilité et simplicité de debug priment sur la performance pour
un v0 dont le but est de valider le mécanisme, pas de l'optimiser.
`crates/plugin-wasm-wire` définit les types `Serialize`/`Deserialize`
(`WireAttributeValue`, `WireKernelEvent`, `WirePluginOutcome`) séparément de
`kernel-model` — `kernel-model` reste sans dépendance externe (voir
CLAUDE.md étape 1), donc les types `serde` du pont WASM vivent dans leur
propre crate, pas ajoutés à `kernel-model`.

## Sécurité / robustesse — différence avec le plugin trait Rust natif

Contrairement à `plugin-example` (code interne, de confiance,
`docs/interfaces/plugin-contract-v0.md` note explicitement que la gestion
d'un plugin qui panique/boucle "n'est pas pertinente tant que les seuls
plugins sont internes") : un module WASM est censé pouvoir venir d'un tiers.
Le host wrapper (`WasmPlugin::inspect`, `crates/plugin-wasm-host`) doit donc
absorber toute défaillance guest (trap, JSON invalide, fonctions manquantes)
en un `PluginOutcome` vide + warning plutôt que de la propager — cohérent
avec le contrat `PluginOutcome` déjà infaillible, mais ça déplace la
responsabilité de "ne jamais planter" du guest (on ne lui fait pas
confiance) vers le wrapper hôte (lui, on lui fait confiance).

**Limite non traitée dans ce v0** : pas de limite de temps d'exécution
(`wasmtime` supporte le fuel metering et les épuisements de temps, mais
c'est un mécanisme à part, pas activé ici) ni de limite mémoire au-delà des
défauts `wasmtime`. Un guest qui boucle infiniment bloquerait l'appelant.
À traiter si/quand des plugins tiers non fiables sont réellement envisagés
— pas anticipé plus loin ici, conformément à la même logique de non
sur-design.

## Ignoré volontairement pour ce v0

- Component Model / WIT (voir plus haut).
- `wasi` (accès fichiers/réseau depuis le guest) — un plugin d'interprétation
  n'en a pas besoin ; le guest ne voit que les octets qu'on lui passe.
- Limites de ressources (fuel/temps/mémoire) — voir ci-dessus.
- Cache de modules compilés entre appels — `WasmPlugin` compile le module
  une fois à la construction (`Module::from_file`) et réutilise `Engine`,
  mais recrée un `Store`/`Instance` à chaque appel (plus simple, pas de
  risque de fuite mémoire guest entre appels ; le coût d'instantiation n'est
  pas mesuré/optimisé ici).

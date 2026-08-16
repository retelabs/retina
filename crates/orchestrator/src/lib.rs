//! Control plane "maison" pour trellis — voir `src/main.rs` pour le contexte
//! (dossier section 5, objectif d'apprentissage plutôt que choix de cloud).

pub mod api;
pub mod auth;
pub mod docker_client;
pub mod image_build;
pub mod topology;

//! Retina's home-made control plane: see `src/main.rs` for the context
//! (design dossier section 5, a learning goal rather than a cloud choice).

pub mod api;
pub mod auth;
pub mod docker_client;
pub mod image_build;
pub mod topology;

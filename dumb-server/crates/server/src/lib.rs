//! Server crate: an axum WebSocket server authoritative over a single world
//! zone. Modules are declared here so integration tests can reach them;
//! `main.rs` is a thin launcher.

pub mod config;
pub mod http;
pub mod ingest;
pub mod metrics;
pub mod player;
pub mod profile;
pub mod writer;
pub mod zone;
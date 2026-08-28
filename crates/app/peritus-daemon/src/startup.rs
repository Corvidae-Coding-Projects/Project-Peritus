//! Deterministic production startup composition.

mod evolution;
mod migration;
mod plan;
mod projection;
mod recovery;
mod registry;
mod runtime;
pub mod workspace;

pub use runtime::DaemonRuntime;

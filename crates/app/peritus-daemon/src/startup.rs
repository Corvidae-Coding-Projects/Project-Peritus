//! Deterministic production startup composition.

mod migration;
mod projection;
mod recovery;
mod runtime;
mod workspace;

pub use runtime::DaemonRuntime;

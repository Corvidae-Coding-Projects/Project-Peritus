//! Host composition for the single-command Peritus product experience.
//!
//! The launcher prepares protected application state, starts or reuses the packaged daemon, and
//! hands the authenticated local endpoint to the interactive client. Durable authority remains in
//! the existing G0, C0, C1, B1, and C3 components.

mod app;
mod bootstrap;
mod daemon;
mod error;
mod identity;
mod layout;
mod persistence;
mod provider_setup;
mod terminal;
mod workspace_setup;

pub use app::{
    configure_providers_interactive, configure_workspaces_interactive, launch_interactive,
    launch_interactive_at,
};
pub use bootstrap::{PreparedProduct, ProductBootstrap};
pub use daemon::{DaemonLaunch, DaemonShutdown, DaemonSupervisor, SiblingBinaries};
pub use error::LauncherError;
pub use layout::AppLayout;

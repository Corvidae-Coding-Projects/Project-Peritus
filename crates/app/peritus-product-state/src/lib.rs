//! Pure, resumable product state for the single-command Peritus experience.
//!
//! This crate owns no filesystem, process, terminal, network, credential, or daemon effects. It
//! defines the durable facts effectful product composition must preserve between launches.

mod error;
mod identity;
mod phase;
mod provider;
mod state;
pub mod verified;
mod workspace;

pub use error::ProductStateError;
pub use identity::InstallIdentity;
pub use phase::{BootstrapPhase, LaunchReadiness};
pub use provider::{CompatibleProtocol, DirectProviderProfile, ProviderKind, ProviderSelection};
pub use state::{PRODUCT_STATE_SCHEMA_VERSION, ProductState};
pub use workspace::{WorkspaceProfile, WorkspaceSelection, WorkspaceTrust};

//! Durable workspace choices and remembered trust without live filesystem claims.

mod profile;
mod selection;

pub use profile::{WorkspaceProfile, WorkspaceTrust};
pub use selection::WorkspaceSelection;

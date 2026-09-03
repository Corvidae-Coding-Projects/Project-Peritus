//! Managed-workspace tools exposed to the D0 developer loop.

mod access_policy;
mod catalog;
mod command_budget;
mod command_runtime;
mod effect;
mod evidence;
mod executor;
mod grounding;
mod inspection;
mod ownership;
mod path;
mod receipt;
mod removal;
mod resources;
mod wire;

pub use catalog::{definitions, read_only_definitions};
pub use command_runtime::CommandRuntime;
pub use evidence::{CommandPurpose, SuccessfulCommand, merge_successful};
pub use executor::ToolCheckpointBoundary;
pub use executor::WorkspaceDeveloperTools;
pub use ownership::WorkspaceOwnership;

pub fn merge_rendered(retained: &mut String, incoming: &str) {
    evidence::merge_rendered(retained, incoming);
}

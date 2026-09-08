//! Managed-workspace tools exposed to the D0 developer loop.

use serde_json::Value;
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

pub use catalog::{definitions, in_place_definition, read_only_definitions};
pub use command_runtime::CommandRuntime;
pub use evidence::{CommandPurpose, SuccessfulCommand, merge_successful};
pub use executor::ToolCheckpointBoundary;
pub use executor::WorkspaceDeveloperTools;
pub use ownership::WorkspaceOwnership;

pub fn merge_rendered(retained: &mut String, incoming: &str) {
    evidence::merge_rendered(retained, incoming);
}

pub fn checked_protected_file(
    root: &std::path::Path,
    relative: &str,
    contract: &str,
    protected: &[std::path::PathBuf],
) -> Result<std::path::PathBuf, peritus_agent::DeveloperLoopError> {
    let mut policy = access_policy::WorkspaceAccessPolicy::from_transcript(root, contract);
    policy.protect(root, protected);
    policy
        .authorize("workspace_read", &Value::from_iter([("path", Value::from(relative))]))
        .map_err(|_| {
            path::tool("context file dependency is outside the current task's access policy")
        })?;
    path::checked(root, relative, true)
}

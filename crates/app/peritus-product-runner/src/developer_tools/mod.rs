//! Managed-workspace tools exposed to the D0 developer loop.

#[cfg(not(verus_only))]
use serde_json::Value;
#[cfg(not(verus_only))]
mod access_policy;
#[cfg(not(verus_only))]
mod arguments;
#[cfg(not(verus_only))]
mod catalog;
#[cfg(not(verus_only))]
mod command_budget;
#[cfg(not(verus_only))]
mod command_runtime;
#[cfg(not(verus_only))]
mod effect;
#[cfg(not(verus_only))]
mod evidence;
#[cfg(not(verus_only))]
mod executor;
mod folder_patch_request;
#[cfg(not(verus_only))]
mod grounding;
#[cfg(not(verus_only))]
mod inspection;
#[cfg(not(verus_only))]
mod ownership;
#[cfg(not(verus_only))]
mod path;
mod preview;
#[cfg(not(verus_only))]
mod receipt;
#[cfg(not(verus_only))]
mod removal;
#[cfg(not(verus_only))]
mod resources;
#[cfg(not(verus_only))]
mod wire;

#[cfg(not(verus_only))]
pub use catalog::{definitions, in_place_definition, read_only_definitions};
#[cfg(not(verus_only))]
pub use command_runtime::{CommandRuntime, FolderPatchAuthority, FolderPatchAuthorityPlan};
#[cfg(not(verus_only))]
pub use evidence::{CommandPurpose, SuccessfulCommand, merge_successful};
#[cfg(not(verus_only))]
pub use executor::ToolCheckpointBoundary;
#[cfg(not(verus_only))]
pub use executor::WorkspaceDeveloperTools;
pub use folder_patch_request::FolderPatchAuthorityPlanRequest;
#[cfg(not(verus_only))]
pub use ownership::WorkspaceOwnership;
pub use preview::{PreviewCommand, PreviewLaunch, PreviewObservation, PreviewProcessState};

#[cfg(not(verus_only))]
pub fn merge_rendered(retained: &mut String, incoming: &str) {
    evidence::merge_rendered(retained, incoming);
}

/// Checks an explicit file against the existing workspace task and protected-path policy.
/// This does not grant a reusable read capability; callers must perform their own bounded read.
///
/// # Errors
/// Rejects protected/opaque targets and paths that escape or traverse links.
#[cfg(not(verus_only))]
pub fn checked_protected_file(
    root: &std::path::Path,
    relative: &str,
    contract: &str,
    protected: &[std::path::PathBuf],
) -> Result<std::path::PathBuf, crate::ProductRunnerError> {
    checked_protected_file_for_developer(root, relative, contract, protected).map_err(|error| {
        crate::ProductRunnerError::new(
            crate::ProductRunnerErrorKind::InvalidPrecondition,
            "check protected workspace file",
            error.to_string(),
        )
    })
}

#[cfg(not(verus_only))]
pub fn checked_protected_file_for_developer(
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

//! Managed-workspace tools exposed to the D0 developer loop.

use serde_json::Value;

mod access_policy;

mod arguments;

mod attachment_read;

mod bundled_skill;
pub(crate) use bundled_skill::{list as list_bundled_skills, read as read_bundled_skill};
mod catalog;

mod command_budget;
mod computer;
mod web;

mod command_runtime;

mod effect;

mod evidence;

mod executor;
mod folder_patch_request;

mod grounding;

mod inspection;

mod inspection_cancellation;
mod inspection_document;

pub mod inspection_cursor;

mod inspection_list;

mod inspection_read;

mod inspection_search;

mod ownership;

mod path;
pub(crate) use access_policy::may_reuse_read;
pub(crate) use grounding::file_digest as source_digest;
pub(crate) use path::ignored;
mod preview;

mod receipt;

mod reference;

mod reference_list;

mod reference_path;

mod removal;

mod resources;

mod wire;

pub(crate) use catalog::{definition as tool_definition, evidence_definition};
pub use catalog::{
    definitions_for_attachments, in_place_definition, read_only_definitions, reviewer_definitions,
};
pub(crate) use wire::observation as module_observation;

pub use command_runtime::{
    CommandRuntime, FolderPatchAuthority, FolderPatchAuthorityPlan,
    MaterializationAuthorityContext, MaterializationAuthorityRequest,
    OwnedMaterializationAuthority, OwnedRestrictedProcessAuthority, PreviewTerminal,
    RestrictedProcessContext,
};

pub use evidence::ReviewerEvidenceSources;

pub use evidence::{CommandPurpose, SuccessfulCommand, merge_successful};

pub use executor::ToolCheckpointBoundary;

pub use executor::WorkspaceDeveloperTools;
pub use folder_patch_request::FolderPatchAuthorityPlanRequest;

pub const DEFAULT_INSPECTION_PAGE_BYTES: usize = 16 * 1024;

/// Default response budget for reading a document, including its line labels and metadata.
pub const DEFAULT_DOCUMENT_READ_BYTES: usize = 1024 * 1024;

pub use ownership::WorkspaceOwnership;
pub use preview::{
    PreviewCommand, PreviewLaunch, PreviewObservation, PreviewOutputMatch,
    PreviewOutputMatchSource, PreviewOutputRange, PreviewOutputStream, PreviewOwner,
    PreviewProcessState,
};

pub use receipt::{
    UncertainEffect, UncertainEffectState, acknowledge_uncertain_effect, uncertain_effects,
};

pub fn merge_rendered(retained: &mut String, incoming: &str) {
    evidence::merge_rendered(retained, incoming);
}

/// Checks an explicit file against the existing workspace task and protected-path policy.
/// This does not grant a reusable read capability; callers must perform their own bounded read.
///
/// # Errors
/// Rejects protected/opaque targets and paths that escape or traverse links.

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

pub(crate) fn checked_protected_observation(
    root: &std::path::Path,
    relative: &str,
    protected: &[std::path::PathBuf],
) -> Result<Option<std::path::PathBuf>, peritus_agent::DeveloperLoopError> {
    let mut policy = access_policy::WorkspaceAccessPolicy::default();
    policy.protect(root, protected);
    policy
        .authorize("workspace_read", &Value::from_iter([("path", Value::from(relative))]))
        .map_err(path::tool)?;
    path::observed(root, relative)
}

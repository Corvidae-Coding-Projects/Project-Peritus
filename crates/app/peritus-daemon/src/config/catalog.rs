//! Strict project, workspace, and tool inventory declarations.

use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

use super::decode_identifier;
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

const MAX_PROJECTS: usize = 1_024;
const MAX_WORKSPACES: usize = 4_096;
const MAX_TOOLS: usize = 256;

/// One configured project and its exact workspace lineages.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectDeclaration {
    project_id: String,
    #[serde(default)]
    workspace_ids: Vec<String>,
}

impl ProjectDeclaration {
    /// Returns the checked project identity.
    ///
    /// # Errors
    ///
    /// Returns invalid input when configuration was constructed without parsing.
    pub fn project_identity(&self) -> Result<peritus_types::ProjectId, DaemonError> {
        peritus_types::ProjectId::new(decode_identifier(&self.project_id, "project identity")?)
            .map_err(|_| invalid("project identity must be nonzero"))
    }

    /// Returns the checked workspace identities owned by this project.
    ///
    /// # Errors
    ///
    /// Returns invalid input when any configured identity is malformed.
    pub fn workspace_identities(&self) -> Result<Vec<peritus_types::WorkspaceId>, DaemonError> {
        self.workspace_ids
            .iter()
            .map(|value| {
                peritus_types::WorkspaceId::new(decode_identifier(value, "workspace identity")?)
                    .map_err(|_| invalid("workspace identity must be nonzero"))
            })
            .collect()
    }
}

/// Path to one exact canonical C1 registration envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDeclaration {
    registration_file: PathBuf,
}

impl WorkspaceDeclaration {
    /// Borrows the absolute registration-envelope file path.
    #[must_use]
    pub fn registration_file(&self) -> &Path {
        &self.registration_file
    }
}

/// Closed explicit C4 tool allowlist. An empty list exposes no tools.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ToolPolicy {
    #[serde(default)]
    allow: Vec<String>,
}

impl ToolPolicy {
    /// Borrows canonical configured tool names.
    #[must_use]
    pub fn allowed(&self) -> &[String] {
        &self.allow
    }
}

pub(super) fn validate(
    projects: &[ProjectDeclaration],
    workspaces: &[WorkspaceDeclaration],
    tools: &ToolPolicy,
) -> Result<(), DaemonError> {
    if projects.len() > MAX_PROJECTS
        || workspaces.len() > MAX_WORKSPACES
        || tools.allow.len() > MAX_TOOLS
    {
        return Err(invalid("configured component inventory exceeds its production bound"));
    }
    let mut project_ids = BTreeSet::new();
    let mut referenced_workspaces = BTreeSet::new();
    for project in projects {
        if !project_ids.insert(project.project_identity()?) {
            return Err(invalid("project identity is configured more than once"));
        }
        for workspace in project.workspace_identities()? {
            if !referenced_workspaces.insert(workspace) {
                return Err(invalid("workspace identity belongs to more than one project"));
            }
        }
    }
    let mut registration_paths = BTreeSet::new();
    for workspace in workspaces {
        let path = workspace.registration_file();
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
            || !registration_paths.insert(path.to_owned())
        {
            return Err(invalid(
                "workspace registration paths must be unique canonical absolute paths",
            ));
        }
    }
    let mut tool_names = BTreeSet::new();
    for tool in &tools.allow {
        if tool.is_empty()
            || tool.len() > 128
            || !tool.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            })
            || !tool_names.insert(tool)
        {
            return Err(invalid("tool allowlist contains an invalid or duplicate name"));
        }
    }
    Ok(())
}

fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "validate daemon component inventory",
        detail,
    )
}

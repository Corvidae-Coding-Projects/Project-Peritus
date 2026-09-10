//! Delivery-specific adapters around the single production execution pipeline.

pub mod scope;

use crate::{
    ProductRunInput, ProductRunnerError, ProductWorkspaceKind,
    budget::RunAccounting,
    candidate::CandidateBaseline,
    developer_tools::{WorkspaceDeveloperTools, WorkspaceOwnership},
    local_context::LocalContextHandle,
    progress::WorkspaceCheckpoint,
    workspace_media::WorkspaceImages,
};
use peritus_model_protocol::{ProviderProfile, ToolDefinition};

impl ProductRunInput {
    pub(crate) fn accounting(&self) -> Result<RunAccounting, ProductRunnerError> {
        if self.workspace_kind.is_in_place() {
            RunAccounting::direct_folder(self.max_elapsed)
        } else {
            RunAccounting::new(&self.workspace_root, self.max_elapsed)
        }
    }

    pub(crate) fn in_place_scope(&self) -> Option<scope::ScopedBaseline> {
        match &self.workspace_kind {
            ProductWorkspaceKind::Managed => None,
            ProductWorkspaceKind::InPlace { protected_paths, baseline_revision } => {
                Some(scope::ScopedBaseline::new(
                    self.workspace_root.clone(),
                    self.trace_path.with_extension(format!("files-{baseline_revision}.jsonl")),
                    protected_paths.clone(),
                    *baseline_revision,
                ))
            }
        }
    }

    pub(crate) fn baseline(&self) -> Result<CandidateBaseline, ProductRunnerError> {
        self.in_place_scope().map_or_else(
            || CandidateBaseline::capture(&self.workspace_root),
            |scope| Ok(CandidateBaseline::in_place(scope)),
        )
    }

    pub(crate) fn checkpoint(&self) -> Result<WorkspaceCheckpoint, ProductRunnerError> {
        // Enrollment is evidence collection, not delivery progress. Only paths whose state
        // differs from their enrolled baseline may replenish recovery budgets.
        self.in_place_scope().map_or_else(
            || WorkspaceCheckpoint::capture(&self.workspace_root),
            |scope| scope.progress_checkpoint(&self.workspace_root),
        )
    }

    pub(crate) fn ownership(&self) -> WorkspaceOwnership {
        if self.workspace_kind.is_in_place() {
            WorkspaceOwnership::direct()
        } else {
            WorkspaceOwnership::capture(&self.workspace_root)
        }
    }

    pub(crate) fn configure_tools(
        &self,
        tools: WorkspaceDeveloperTools,
    ) -> WorkspaceDeveloperTools {
        tools
            .with_protected_paths(self.workspace_kind.protected_paths())
            .with_in_place_scope(self.in_place_scope())
    }

    pub(crate) fn developer_definitions(&self) -> Result<Vec<ToolDefinition>, ProductRunnerError> {
        let mut definitions = crate::developer_tools::definitions()?;
        if self.workspace_kind.is_in_place() {
            definitions.push(scope::definition()?);
        }
        Ok(definitions)
    }

    pub(crate) fn media(
        &self,
        transcript: &str,
        profile: &ProviderProfile,
    ) -> Result<WorkspaceImages, ProductRunnerError> {
        if self.workspace_kind.is_in_place() {
            crate::workspace_media::discover_explicit(
                &self.workspace_root,
                transcript,
                profile,
                self.workspace_kind.protected_paths(),
            )
        } else {
            crate::workspace_media::discover(&self.workspace_root, transcript, profile)
        }
    }

    pub(crate) fn working_memory(
        &self,
        role: &str,
    ) -> Result<Option<LocalContextHandle>, ProductRunnerError> {
        if self.workspace_kind.is_in_place() {
            if role == "reviewer" {
                Ok(None)
            } else {
                LocalContextHandle::open_folder(self, self.workspace_kind.protected_paths())
            }
        } else {
            LocalContextHandle::open(self, role)
        }
    }

    pub(crate) const fn delivery_instructions(&self) -> &'static str {
        if self.workspace_kind.is_in_place() {
            "\nIN-PLACE DELIVERY: Work directly in this ordinary directory using the same design, verification, independent review and fixer pipeline. Do not initialize Git, create a baseline commit, or scan unrelated files. Files explicitly read or edited are tracked individually; use workspace_scope BEFORE a command to declare any additional exact files it will create or modify. Declare generated deliverables and tests, not build-cache directories. Commands retain their authorized local-user effects and are not a filesystem sandbox. Preserve private and unrelated files. Qualification covers the tracked task files, not a whole-folder inventory. Changes are already in place; there is no later Git accept/discard or automatic rollback. Verify actual requested behavior; report unavailable checks honestly."
        } else {
            ""
        }
    }
}

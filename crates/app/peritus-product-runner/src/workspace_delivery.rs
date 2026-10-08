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
            RunAccounting::direct_folder_with_cancellation(
                self.max_elapsed,
                std::sync::Arc::clone(&self.cancelled),
            )
        } else {
            RunAccounting::new_with_cancellation(
                &self.workspace_root,
                self.max_elapsed,
                std::sync::Arc::clone(&self.cancelled),
            )
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
            || {
                crate::candidate::process::with_cancellation(
                    crate::candidate::process::Cancellation::new(
                        std::sync::Arc::clone(&self.cancelled),
                        self.provider_cancellation.clone(),
                    ),
                    || CandidateBaseline::capture_task(&self.workspace_root, &self.trace_path),
                )
            },
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

    pub(crate) fn ownership(&self) -> Result<WorkspaceOwnership, ProductRunnerError> {
        if self.workspace_kind.is_in_place() {
            Ok(WorkspaceOwnership::direct())
        } else {
            WorkspaceOwnership::try_capture(&self.workspace_root)
                .map_err(|error| crate::turn::developer_error(&error))
        }
    }

    pub(crate) fn configure_tools(
        &self,
        tools: WorkspaceDeveloperTools,
    ) -> WorkspaceDeveloperTools {
        let reference_authority = self.conversation.reference_authority_context();
        let revision = self.conversation.revision();
        let request_sources = self.conversation.request_source_binding();
        let tools = tools
            .with_directory_listing_owner(crate::developer_tools::DirectoryListingOwner::new(
                self.workspace_root.clone(),
                self.trace_path.with_extension("directory-listings"),
                std::sync::Arc::clone(&self.cancelled),
                self.provider_cancellation.clone(),
            ))
            .with_reference_contract(&reference_authority)
            .with_protected_paths(self.workspace_kind.protected_paths())
            .with_protection_view(std::sync::Arc::clone(&self.conversation))
            .with_in_place_scope(self.in_place_scope());
        if self.conversation.revision() == revision {
            tools.with_progress_binding(revision, request_sources)
        } else {
            tools
        }
    }

    pub(crate) fn developer_definitions(&self) -> Result<Vec<ToolDefinition>, ProductRunnerError> {
        let mut definitions = crate::developer_tools::definitions()?;
        if self.workspace_kind.is_in_place() {
            definitions.extend(scope::definitions()?);
        }
        Ok(definitions)
    }

    pub(crate) fn media(
        &self,
        transcript: &str,
        profile: &ProviderProfile,
    ) -> Result<WorkspaceImages, ProductRunnerError> {
        if self.conversation.uses_explicit_media() {
            return Ok(WorkspaceImages::default());
        }
        if self.workspace_kind.is_in_place() {
            crate::workspace_media::discover_explicit_retained(
                &self.workspace_root,
                transcript,
                profile,
                self.workspace_kind.protected_paths(),
                &self.trace_path.with_extension("workspace-media"),
                self.cancelled.as_ref(),
            )
        } else {
            crate::workspace_media::discover_retained(
                &self.workspace_root,
                transcript,
                profile,
                &self.trace_path.with_extension("workspace-media"),
                self.cancelled.as_ref(),
            )
        }
    }

    pub(crate) fn working_memory(
        &self,
        role: &str,
    ) -> Result<Option<LocalContextHandle>, ProductRunnerError> {
        LocalContextHandle::open(self, role)
    }

    pub(crate) async fn working_memory_async(
        &self,
        role: &str,
    ) -> Result<Option<LocalContextHandle>, ProductRunnerError> {
        LocalContextHandle::open_async(self, role).await
    }

    pub(crate) fn native_session_directory(&self, role: &str) -> std::path::PathBuf {
        self.trace_path.with_extension("context").join(role).join("provider-sessions")
    }

    pub(crate) const fn delivery_instructions(&self) -> &'static str {
        if self.workspace_kind.is_in_place() {
            "\nIN-PLACE DELIVERY: Work directly in this ordinary directory using the same design, verification, independent review and fixer pipeline. Do not initialize Git, create a baseline commit, or scan unrelated files. Files explicitly read or edited are tracked individually; use workspace_scope BEFORE a command to declare any additional exact files it will create or modify. Declare generated deliverables and tests, not build-cache directories. Commands retain their authorized local-user effects and are not a filesystem sandbox. Preserve private and unrelated files. Qualification covers the tracked task files, not a whole-folder inventory. Changes are already in place; there is no later Git accept/discard or automatic rollback. Verify actual requested behavior; report unavailable checks honestly."
        } else {
            ""
        }
    }
}

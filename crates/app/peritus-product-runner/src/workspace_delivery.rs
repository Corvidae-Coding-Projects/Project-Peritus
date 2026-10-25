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
            || CandidateBaseline::capture_task(&self.workspace_root, &self.trace_path),
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
        let reference_authority = self.conversation.reference_authority_context();
        tools
            .with_computer_runtime(self.command_runtime.clone())
            .with_inspection_cancellation(
                std::sync::Arc::clone(&self.cancelled),
                self.provider_cancellation.clone(),
            )
            .with_reference_contract(&reference_authority)
            .with_protected_paths(self.workspace_kind.protected_paths())
            .with_protection_view(std::sync::Arc::clone(&self.conversation))
            .with_in_place_scope(self.in_place_scope())
            .with_observation_receipts(&self.trace_path, self.run_id, self.workspace_id)
    }

    /// Creates read-only workspace tools with this run's current authority and verified facts.
    /// This is the same scoped inspection path used by the designer and independent reviewer.
    #[must_use]
    pub fn read_only_workspace_tools(&self) -> WorkspaceDeveloperTools {
        self.configure_tools(
            WorkspaceDeveloperTools::read_only(self.workspace_root.clone())
                .with_task_contract(&self.conversation.reference_authority_context()),
        )
    }

    /// Returns the production developer catalog for this delivery and attachment scope.
    /// Memory and invocation-specific historical evidence tools are supplied by their owners.
    ///
    /// # Errors
    /// Rejects invalid bounded tool definitions.
    pub fn developer_definitions(&self) -> Result<Vec<ToolDefinition>, ProductRunnerError> {
        let mut definitions = crate::developer_tools::definitions_for_attachments(
            self.conversation.has_selected_file_attachments(),
        )?;
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
        if self.conversation.uses_explicit_media() {
            return Ok(WorkspaceImages::default());
        }
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
            if matches!(role, "reviewer" | "designer") {
                LocalContextHandle::open_folder_for_role(
                    self,
                    role,
                    self.workspace_kind.protected_paths(),
                )
            } else {
                LocalContextHandle::open_folder(self, self.workspace_kind.protected_paths())
            }
        } else {
            LocalContextHandle::open(self, role)
        }
    }

    pub(crate) const fn delivery_instructions(&self) -> &'static str {
        if self.workspace_kind.is_in_place() {
            "\nIN-PLACE DELIVERY: Work directly in this ordinary directory using the same design, verification, independent review and fixer pipeline. Do not initialize Git, create a baseline commit, or scan unrelated files. Files explicitly read or edited are tracked individually; use workspace_scope BEFORE a command to declare any additional files or a narrow task/output directory it will create or modify. A declared directory tracks its existing files and newly created descendants. Declare generated deliverables, not build-cache directories. Commands retain their authorized local-user effects and are not a filesystem sandbox. Preserve private and unrelated files. Qualification covers the declared task scope, not a whole-folder inventory. Changes are already in place; there is no later Git accept/discard or automatic rollback. Verify actual requested behavior; report unavailable checks honestly."
        } else {
            ""
        }
    }
}

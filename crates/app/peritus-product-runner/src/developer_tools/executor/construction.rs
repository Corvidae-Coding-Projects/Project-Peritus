//! Read-only and explicitly writable filesystem/tool runtime construction.

use super::{
    ActiveCommandLedger, CommandBudget, CommandEvidence, CommandResources, EffectReceiptLedger,
    ExplicitReferences, GroundingEvidence, WorkspaceAccessPolicy, WorkspaceDeveloperTools,
    WorkspaceOwnership, WorkspaceToolMode,
};
use std::{path::PathBuf, time::Duration};

impl WorkspaceDeveloperTools {
    /// Creates an executor that rejects every mutating or process tool even if a provider emits an
    /// undeclared call.
    #[must_use]
    pub fn read_only(root: PathBuf) -> Self {
        let ownership = WorkspaceOwnership::direct();
        let grounding = GroundingEvidence::for_workspace(&root);
        let directory_listings = super::inspection::DirectoryListingOwner::ephemeral(root.clone())
            .ok();
        Self {
            root,
            access_policy: WorkspaceAccessPolicy::default(),
            references: ExplicitReferences::default(),
            grounding,
            ownership,
            mode: WorkspaceToolMode::ReadOnly,
            in_place_scope: None,
            command_evidence: CommandEvidence::default(),
            command_budget: None,
            receipts: None,
            resources: CommandResources::observe(),
            command_runtime: None,
            active_commands: ActiveCommandLedger::default(),
            #[cfg(test)]
            progress_nudges: 0,
            inspection_progress: super::inspection_progress::InspectionProgress::default(),
            directory_listings,
            request_sources: super::sources::RequestSourceProgress::default(),
            checkpoint_observer: None,
            checkpoint_view: None,
            prepared_mutations: Vec::new(),
            checkpoint_targets: Vec::new(),
            protection_view: None,
        }
    }

    /// Attaches the run-owned command runtime for native, read-only verification commands.
    #[must_use]
    pub(crate) fn with_observational_commands(
        mut self,
        command_runtime: crate::CommandRuntime,
    ) -> Self {
        self.command_budget = Some(CommandBudget::new(None));
        self.command_runtime = Some(command_runtime);
        self
    }

    pub(crate) fn with_ownership(
        root: PathBuf,
        ownership: WorkspaceOwnership,
        receipt_path: PathBuf,
        receipt_scope: String,
        command_horizon: impl Into<Option<Duration>>,
        command_runtime: crate::CommandRuntime,
    ) -> Self {
        let grounding = GroundingEvidence::for_workspace(&root);
        let directory_listings = Some(super::inspection::DirectoryListingOwner::new(
            root.clone(),
            receipt_path.with_extension("directory-listings"),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            peritus_provider_core::CancellationToken::new(),
        ));
        Self {
            root,
            access_policy: WorkspaceAccessPolicy::default(),
            references: ExplicitReferences::default(),
            grounding,
            ownership,
            mode: WorkspaceToolMode::ReadWrite,
            in_place_scope: None,
            command_evidence: CommandEvidence::default(),
            command_budget: Some(CommandBudget::new(command_horizon.into())),
            receipts: Some(EffectReceiptLedger::new(receipt_path, receipt_scope)),
            resources: CommandResources::observe(),
            command_runtime: Some(command_runtime),
            active_commands: ActiveCommandLedger::default(),
            #[cfg(test)]
            progress_nudges: 0,
            inspection_progress: super::inspection_progress::InspectionProgress::default(),
            directory_listings,
            request_sources: super::sources::RequestSourceProgress::default(),
            checkpoint_observer: None,
            checkpoint_view: None,
            prepared_mutations: Vec::new(),
            checkpoint_targets: Vec::new(),
            protection_view: None,
        }
    }
}

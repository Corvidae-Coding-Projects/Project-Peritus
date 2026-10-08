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
        Self {
            root,
            access_policy: WorkspaceAccessPolicy::default(),
            references: ExplicitReferences::default(),
            grounding: GroundingEvidence::default(),
            ownership,
            mode: WorkspaceToolMode::ReadOnly,
            in_place_scope: None,
            command_evidence: CommandEvidence::default(),
            command_budget: None,
            receipts: None,
            resources: CommandResources::observe(),
            inspection_cancellation: super::InspectionCancellation::default(),
            command_runtime: None,
            active_commands: ActiveCommandLedger::default(),
            tools_without_delivery_progress: 0,
            progress_nudges: 0,
            progress_feedback_pending: false,
            inspection_progress: super::inspection_progress::InspectionProgress::default(),
            checkpoint_observer: None,
            prepared_mutations: Vec::new(),
            protection_view: None,
        }
    }

    pub(crate) fn with_ownership(
        root: PathBuf,
        ownership: WorkspaceOwnership,
        receipt_path: PathBuf,
        receipt_scope: String,
        command_horizon: impl Into<Option<Duration>>,
        command_runtime: crate::CommandRuntime,
    ) -> Self {
        Self {
            root,
            access_policy: WorkspaceAccessPolicy::default(),
            references: ExplicitReferences::default(),
            grounding: GroundingEvidence::default(),
            ownership,
            mode: WorkspaceToolMode::ReadWrite,
            in_place_scope: None,
            command_evidence: CommandEvidence::default(),
            command_budget: Some(CommandBudget::new(command_horizon.into())),
            receipts: Some(EffectReceiptLedger::new(receipt_path, receipt_scope)),
            resources: CommandResources::observe(),
            inspection_cancellation: super::InspectionCancellation::default(),
            command_runtime: Some(command_runtime),
            active_commands: ActiveCommandLedger::default(),
            tools_without_delivery_progress: 0,
            progress_nudges: 0,
            progress_feedback_pending: false,
            inspection_progress: super::inspection_progress::InspectionProgress::default(),
            checkpoint_observer: None,
            prepared_mutations: Vec::new(),
            protection_view: None,
        }
    }
}

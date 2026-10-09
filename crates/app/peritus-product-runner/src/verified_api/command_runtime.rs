//! Fail-closed projection of effectful command-runtime ownership.

use std::{
    convert::Infallible,
    path::{Path, PathBuf},
};

use peritus_leases::LeaseHolder;
use peritus_process::{ExecutionPlan, ProcessControl, ProcessStore, TerminalResult};
use peritus_types::{ActionId, EnvironmentId, ProcessId, ResourceId, RevisionTuple, RunId};
use peritus_workspace::WorkspaceAuthorizationRequest;

use crate::{
    FolderPatchAuthorityPlanRequest, PreviewCommand, PreviewLaunch, PreviewObservation,
    PreviewOutputMatch, ProductRunnerError, ProductRunnerErrorKind,
};

/// Verification-only command runtime. No safe constructor can create this effect owner.
#[derive(Clone)]
pub struct CommandRuntime {
    unavailable: Infallible,
}

/// Verification-only terminal lease; an unavailable runtime cannot construct one.
pub struct PreviewTerminal {
    unavailable: Infallible,
}

impl PreviewTerminal {
    /// Borrows the ordinary execution plan; this projection is uninhabited.
    #[must_use]
    pub const fn plan(&self) -> &ExecutionPlan {
        match self.unavailable {}
    }

    /// Returns ordinary process control; this projection is uninhabited.
    #[must_use]
    pub fn control(&self) -> ProcessControl {
        match self.unavailable {}
    }

    /// Observes ordinary terminal settlement; this projection is uninhabited.
    pub fn wait(self) -> Result<TerminalResult, ProductRunnerError> {
        match self.unavailable {}
    }
}

/// Verification-only folder-authority plan. No effectful plan can be produced in this mode.
pub struct FolderPatchAuthorityPlan {
    unavailable: Infallible,
}

impl FolderPatchAuthorityPlan {
    /// Returns the checked revision of an ordinary plan; this projection is uninhabited.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        match self.unavailable {}
    }

    /// Returns the lease holder of an ordinary plan; this projection is uninhabited.
    #[must_use]
    pub const fn lease_holder(&self) -> LeaseHolder {
        match self.unavailable {}
    }

    /// Returns the resource identity of an ordinary plan; this projection is uninhabited.
    #[must_use]
    pub const fn resource_id(&self) -> ResourceId {
        match self.unavailable {}
    }

    /// Returns the environment identity of an ordinary plan; this projection is uninhabited.
    #[must_use]
    pub const fn environment_id(&self) -> EnvironmentId {
        match self.unavailable {}
    }
}

/// Verification-only folder authority. No committed receipts exist in this mode.
pub struct FolderPatchAuthority {
    unavailable: Infallible,
}

impl FolderPatchAuthority {
    /// Borrows ordinary committed receipts; this projection is uninhabited.
    #[must_use]
    pub const fn request(&self) -> WorkspaceAuthorizationRequest<'_> {
        match self.unavailable {}
    }
}

impl CommandRuntime {
    /// Verification-only builds cannot inspect native command receipt owners.
    pub fn receipt_linked_live_owners(
        receipt_path: &Path,
        source_run: RunId,
        processes: &ProcessStore,
    ) -> Result<Vec<(RunId, ActionId, ProcessId)>, ProductRunnerError> {
        let _ = (receipt_path, source_run, processes);
        Err(unavailable("inspect receipt-linked native command owners"))
    }

    /// Verification-only builds cannot inspect or reconcile native command receipts.
    pub fn reconcile_effect_receipts(&self, receipt_path: &Path) -> Result<(), ProductRunnerError> {
        let _ = (self, receipt_path);
        Err(unavailable("reconcile native command receipts"))
    }

    /// Projects terminal borrowing without constructing an effect owner during verification.
    pub fn preview_terminal(
        &self,
        _launch: &PreviewLaunch,
    ) -> Result<PreviewTerminal, ProductRunnerError> {
        match self.unavailable {}
    }

    /// Verification-only builds cannot construct an effectful direct-folder command owner.
    pub fn open_direct(
        state_root: impl Into<PathBuf>,
        workspace_root: impl Into<PathBuf>,
        run_id: RunId,
        process_store: ProcessStore,
    ) -> Result<Self, ProductRunnerError> {
        let _ = (state_root.into(), workspace_root.into(), run_id, process_store);
        Err(unavailable("open direct-folder command runtime"))
    }

    /// Verification-only builds cannot construct an effectful command owner.
    pub fn open(
        state_root: impl Into<PathBuf>,
        workspace_root: impl Into<PathBuf>,
        run_id: RunId,
        process_store: ProcessStore,
    ) -> Result<Self, ProductRunnerError> {
        let _ = (state_root.into(), workspace_root.into(), run_id, process_store);
        Err(unavailable("open command runtime"))
    }

    /// Verification-only builds cannot configure an effect owner.
    pub fn with_local_context(
        self,
        config: crate::LocalContextConfig,
    ) -> Result<Self, ProductRunnerError> {
        let _ = (self, config);
        Err(unavailable("configure command runtime"))
    }

    /// Verification-only builds cannot plan effectful folder authority.
    pub fn plan_folder_patch_authority(
        &self,
        request: FolderPatchAuthorityPlanRequest,
    ) -> Result<FolderPatchAuthorityPlan, ProductRunnerError> {
        let _ = request;
        Err(unavailable("plan folder-patch authority"))
    }

    /// Verification-only builds cannot commit effectful folder authority.
    pub fn commit_folder_patch_authority(
        &self,
        plan: FolderPatchAuthorityPlan,
        payload: Vec<u8>,
    ) -> Result<FolderPatchAuthority, ProductRunnerError> {
        let _ = (plan, payload);
        Err(unavailable("commit folder-patch authority"))
    }

    /// Verification-only builds cannot launch a preview process.
    pub fn launch_preview(
        &self,
        command: &PreviewCommand,
    ) -> Result<PreviewLaunch, ProductRunnerError> {
        let _ = command;
        Err(unavailable("launch preview process"))
    }

    /// Verification-only builds cannot observe a preview process.
    pub fn observe_preview(
        &self,
        launch: &PreviewLaunch,
    ) -> Result<PreviewObservation, ProductRunnerError> {
        let _ = launch;
        Err(unavailable("observe preview process"))
    }

    /// Projects a bounded output read; this verification-only effect owner is uninhabited.
    pub fn preview_output_range(
        &self,
        _process_id: ProcessId,
        _stream: peritus_process::OutputStream,
        _offset: u64,
        _maximum_bytes: usize,
    ) -> Result<crate::PreviewOutputRange, ProductRunnerError> {
        match self.unavailable {}
    }

    /// Projects an output evidence search; this verification-only effect owner is uninhabited.
    pub fn preview_output_contains(
        &self,
        _process_id: ProcessId,
        _needle: &str,
    ) -> Result<bool, ProductRunnerError> {
        match self.unavailable {}
    }

    /// Projects exact output match evidence; this verification-only effect owner is uninhabited.
    pub fn preview_output_match(
        &self,
        _process_id: ProcessId,
        _needle: &str,
    ) -> Result<Option<crate::PreviewOutputMatch>, ProductRunnerError> {
        match self.unavailable {}
    }

    /// Projects retained-source evidence verification; this effect owner is uninhabited.
    pub fn verify_preview_output_match(
        &self,
        _evidence: PreviewOutputMatch,
        _needle: &str,
    ) -> Result<(), ProductRunnerError> {
        match self.unavailable {}
    }

    /// Verification-only builds cannot interact with a preview process.
    pub fn interact_preview(
        &self,
        launch: &PreviewLaunch,
        bytes: Vec<u8>,
    ) -> Result<PreviewObservation, ProductRunnerError> {
        let _ = (launch, bytes);
        Err(unavailable("interact with preview process"))
    }

    /// Verification-only builds cannot stop a preview process.
    pub fn stop_preview(
        &self,
        launch: &PreviewLaunch,
    ) -> Result<PreviewObservation, ProductRunnerError> {
        let _ = launch;
        Err(unavailable("stop preview process"))
    }

    /// Verification-only builds cannot run a preview helper process.
    pub fn run_preview_helper(
        &self,
        command: &PreviewCommand,
    ) -> Result<PreviewObservation, ProductRunnerError> {
        let _ = command;
        Err(unavailable("run preview helper process"))
    }
}

fn unavailable(operation: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        operation,
        "command effects are unavailable in a verus_only build",
    )
}

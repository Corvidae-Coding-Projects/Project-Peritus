//! Bounded launch results with independently classified evidence.

use super::{
    MAX_WORKBENCH_ARTIFACT_FEEDBACK, MAX_WORKBENCH_CAPTURES, MAX_WORKBENCH_INTERACTIONS,
    MAX_WORKBENCH_LAUNCHES, WorkbenchArtifactFeedback, WorkbenchCaptureCapability,
    WorkbenchCaptureReceipt, WorkbenchCaptureState, WorkbenchInteractionReceipt,
    WorkbenchLaunchProfile, WorkbenchLaunchState, WorkbenchResultQuery, invalid,
};
use crate::{AppProtocolError, ControlOperationId};
use peritus_types::{ProcessId, Sha256Digest};

/// One launch and its independent evidence classes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchLaunchResult {
    launch: ControlOperationId,
    profile: WorkbenchLaunchProfile,
    process: Option<ProcessId>,
    state: WorkbenchLaunchState,
    ready: bool,
    interactions: Vec<WorkbenchInteractionReceipt>,
    captures: Vec<WorkbenchCaptureReceipt>,
    feedback: Vec<WorkbenchArtifactFeedback>,
    behavior_checks: u32,
    stdout_digest: Option<Sha256Digest>,
    exit_code: Option<u32>,
}

impl WorkbenchLaunchResult {
    /// Validates bounded evidence collections and process/state consistency.
    ///
    /// # Errors
    /// Rejects excessive collections or a running state without an exact process identity.
    #[allow(clippy::too_many_arguments, reason = "independent result evidence remains explicit")]
    pub fn new(
        launch: ControlOperationId,
        profile: WorkbenchLaunchProfile,
        process: Option<ProcessId>,
        state: WorkbenchLaunchState,
        ready: bool,
        interactions: Vec<WorkbenchInteractionReceipt>,
        captures: Vec<WorkbenchCaptureReceipt>,
        feedback: Vec<WorkbenchArtifactFeedback>,
        behavior_checks: u32,
        stdout_digest: Option<Sha256Digest>,
        exit_code: Option<u32>,
    ) -> Result<Self, AppProtocolError> {
        if interactions.len() > MAX_WORKBENCH_INTERACTIONS
            || captures.len() > MAX_WORKBENCH_CAPTURES
            || feedback.len() > MAX_WORKBENCH_ARTIFACT_FEEDBACK
            || matches!(state, WorkbenchLaunchState::Running) && process.is_none()
        {
            return Err(invalid());
        }
        Ok(Self {
            launch,
            profile,
            process,
            state,
            ready,
            interactions,
            captures,
            feedback,
            behavior_checks,
            stdout_digest,
            exit_code,
        })
    }
    /// Returns original launch operation identity.
    #[must_use]
    pub const fn launch(&self) -> ControlOperationId {
        self.launch
    }
    /// Borrows the exact admitted profile.
    #[must_use]
    pub const fn profile(&self) -> &WorkbenchLaunchProfile {
        &self.profile
    }
    /// Returns actual C2 process identity after launch.
    #[must_use]
    pub const fn process(&self) -> Option<ProcessId> {
        self.process
    }
    /// Returns current process lifecycle state.
    #[must_use]
    pub const fn state(&self) -> WorkbenchLaunchState {
        self.state
    }
    /// Returns whether C2 process birth/readiness was observed.
    #[must_use]
    pub const fn ready(&self) -> bool {
        self.ready
    }
    /// Borrows exact input receipts; these do not alone prove behavior.
    #[must_use]
    pub fn interactions(&self) -> &[WorkbenchInteractionReceipt] {
        &self.interactions
    }
    /// Borrows independent capture attempts and receipts.
    #[must_use]
    pub fn captures(&self) -> &[WorkbenchCaptureReceipt] {
        &self.captures
    }
    /// Borrows human artifact feedback.
    #[must_use]
    pub fn feedback(&self) -> &[WorkbenchArtifactFeedback] {
        &self.feedback
    }
    /// Returns number of daemon-verified behavioral observations.
    #[must_use]
    pub const fn behavior_checks(&self) -> u32 {
        self.behavior_checks
    }
    /// Returns bounded terminal-output digest when the process settled.
    #[must_use]
    pub const fn stdout_digest(&self) -> Option<Sha256Digest> {
        self.stdout_digest
    }
    /// Returns terminal operating-system code when observed.
    #[must_use]
    pub const fn exit_code(&self) -> Option<u32> {
        self.exit_code
    }
    /// Returns the five user-facing evidence classes independently.
    #[must_use]
    pub fn evidence(&self) -> WorkbenchResultEvidence {
        WorkbenchResultEvidence {
            built: self.profile.build().is_some(),
            launched: self.process.is_some(),
            captured: self
                .captures
                .iter()
                .any(|capture| capture.state() == WorkbenchCaptureState::Captured),
            behavior_checked: self.behavior_checks > 0,
            human_reviewed: !self.feedback.is_empty(),
        }
    }
}

/// Independent evidence flags; no field implies any other field.
// The flags are intentionally orthogonal rather than states in one lifecycle.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchResultEvidence {
    built: bool,
    launched: bool,
    captured: bool,
    behavior_checked: bool,
    human_reviewed: bool,
}

impl WorkbenchResultEvidence {
    /// Returns whether a rechecked build identity was supplied.
    #[must_use]
    pub const fn built(self) -> bool {
        self.built
    }
    /// Returns whether an actual C2 process identity was observed.
    #[must_use]
    pub const fn launched(self) -> bool {
        self.launched
    }
    /// Returns whether a selected-window image artifact was published.
    #[must_use]
    pub const fn captured(self) -> bool {
        self.captured
    }
    /// Returns whether actual terminal behavior matched an explicit observation.
    #[must_use]
    pub const fn behavior_checked(self) -> bool {
        self.behavior_checked
    }
    /// Returns whether a human added artifact-anchored feedback.
    #[must_use]
    pub const fn human_reviewed(self) -> bool {
        self.human_reviewed
    }
}

/// Complete bounded result-panel snapshot. Opening it has no execution effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchResultPage {
    query: WorkbenchResultQuery,
    control_revision: u64,
    result_revision: u64,
    capability: WorkbenchCaptureCapability,
    launches: Vec<WorkbenchLaunchResult>,
}

impl WorkbenchResultPage {
    /// Creates a bounded result page.
    ///
    /// # Errors
    /// Rejects absent control state or excessive launch rows.
    pub fn new(
        query: WorkbenchResultQuery,
        control_revision: u64,
        result_revision: u64,
        capability: WorkbenchCaptureCapability,
        launches: Vec<WorkbenchLaunchResult>,
    ) -> Result<Self, AppProtocolError> {
        if control_revision == 0 || launches.len() > MAX_WORKBENCH_LAUNCHES {
            return Err(invalid());
        }
        Ok(Self { query, control_revision, result_revision, capability, launches })
    }
    /// Returns exact result scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchResultQuery {
        self.query
    }
    /// Returns current authoritative conversation revision used for freshness.
    #[must_use]
    pub const fn control_revision(&self) -> u64 {
        self.control_revision
    }
    /// Returns monotonic persisted preview-result revision.
    #[must_use]
    pub const fn result_revision(&self) -> u64 {
        self.result_revision
    }
    /// Borrows current host capture capability.
    #[must_use]
    pub const fn capability(&self) -> &WorkbenchCaptureCapability {
        &self.capability
    }
    /// Borrows retained launch results.
    #[must_use]
    pub fn launches(&self) -> &[WorkbenchLaunchResult] {
        &self.launches
    }
}

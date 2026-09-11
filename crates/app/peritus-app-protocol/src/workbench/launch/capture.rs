//! Explicit capture controls, receipts, and artifact feedback primitives.

use super::{WorkbenchLaunchText, invalid};
use crate::{
    AppProtocolError, ControlOperationId, WorkbenchInputText, WorkbenchQuery,
    WorkbenchReviewFeedback,
};
use peritus_types::{ArtifactId, RunId, Sha256Digest};

/// Capture consent decision; denial is persisted without attempting a capture.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkbenchCaptureConsent {
    /// The user approved this exact selected-window capture.
    Granted,
    /// The user denied capture; no backend fallback is permitted.
    Denied,
}

impl WorkbenchCaptureConsent {
    pub(crate) const fn tag(self) -> u16 {
        match self {
            Self::Granted => 1,
            Self::Denied => 2,
        }
    }
    pub(crate) const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Granted),
            2 => Some(Self::Denied),
            _ => None,
        }
    }
}

/// Explicit capture target. No whole-desktop variant exists.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkbenchCaptureTarget {
    /// One user-selected nonzero X11 window identifier.
    X11Window(u64),
}

impl WorkbenchCaptureTarget {
    /// Creates one selected X11 window target.
    ///
    /// # Errors
    /// Rejects the absent identifier zero.
    pub const fn x11_window(window: u64) -> Result<Self, AppProtocolError> {
        if window == 0 { Err(invalid()) } else { Ok(Self::X11Window(window)) }
    }
}

/// One exact capture request scoped to an accepted launch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchCaptureRequest {
    launch: ControlOperationId,
    target: WorkbenchCaptureTarget,
    consent: WorkbenchCaptureConsent,
}

impl WorkbenchCaptureRequest {
    /// Creates an explicit selected-window request.
    #[must_use]
    pub const fn new(
        launch: ControlOperationId,
        target: WorkbenchCaptureTarget,
        consent: WorkbenchCaptureConsent,
    ) -> Self {
        Self { launch, target, consent }
    }
    /// Returns the original launch operation.
    #[must_use]
    pub const fn launch(self) -> ControlOperationId {
        self.launch
    }
    /// Returns the selected non-desktop target.
    #[must_use]
    pub const fn target(self) -> WorkbenchCaptureTarget {
        self.target
    }
    /// Returns explicit consent or denial.
    #[must_use]
    pub const fn consent(self) -> WorkbenchCaptureConsent {
        self.consent
    }
}

/// Optional exact pixel-space feedback anchor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchArtifactRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl WorkbenchArtifactRegion {
    /// Creates a nonempty pixel rectangle.
    ///
    /// # Errors
    /// Rejects zero area or coordinate overflow.
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Result<Self, AppProtocolError> {
        if width == 0
            || height == 0
            || x.checked_add(width).is_none()
            || y.checked_add(height).is_none()
        {
            Err(invalid())
        } else {
            Ok(Self { x, y, width, height })
        }
    }
    /// Returns `(x, y, width, height)`.
    #[must_use]
    pub const fn coordinates(self) -> (u32, u32, u32, u32) {
        (self.x, self.y, self.width, self.height)
    }
}

/// Read-only result-panel query for one governing run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchResultQuery {
    query: WorkbenchQuery,
    run: RunId,
}

impl WorkbenchResultQuery {
    /// Creates an exact conversation/workspace/run query.
    #[must_use]
    pub const fn new(query: WorkbenchQuery, run: RunId) -> Self {
        Self { query, run }
    }
    /// Returns conversation/workspace scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the governing run.
    #[must_use]
    pub const fn run(self) -> RunId {
        self.run
    }
}

/// Native capture backend capability projected honestly for the current daemon host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkbenchCaptureCapability {
    /// `ImageMagick` selected-window capture is available on the active X11 display.
    X11SelectedWindow,
    /// Automated capture is unavailable; the reason is inert bounded text.
    Unavailable(WorkbenchLaunchText),
}

/// Current owned-process state, independent from validation evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkbenchLaunchState {
    /// Accepted profile has no observed process birth.
    Accepted,
    /// C2 process birth was observed and remains live.
    Running,
    /// The process stopped after explicit user cancellation.
    Stopped,
    /// The process exited successfully on its own.
    Exited,
    /// The process failed, timed out, or has an indeterminate outcome.
    Failed,
}

impl WorkbenchLaunchState {
    pub(crate) const fn tag(self) -> u16 {
        match self {
            Self::Accepted => 1,
            Self::Running => 2,
            Self::Stopped => 3,
            Self::Exited => 4,
            Self::Failed => 5,
        }
    }
    pub(crate) const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Accepted),
            2 => Some(Self::Running),
            3 => Some(Self::Stopped),
            4 => Some(Self::Exited),
            5 => Some(Self::Failed),
            _ => None,
        }
    }
}

/// Accepted process-input receipt. Acceptance alone is not a behavior check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchInteractionReceipt {
    operation: ControlOperationId,
    digest: Sha256Digest,
    observed: bool,
}

impl WorkbenchInteractionReceipt {
    /// Creates one exact input receipt.
    #[must_use]
    pub const fn new(operation: ControlOperationId, digest: Sha256Digest, observed: bool) -> Self {
        Self { operation, digest, observed }
    }
    /// Returns interaction operation identity.
    #[must_use]
    pub const fn operation(self) -> ControlOperationId {
        self.operation
    }
    /// Returns the exact input digest without disclosing bytes.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Returns whether C2 accepted the write.
    #[must_use]
    pub const fn observed(self) -> bool {
        self.observed
    }
}

/// Terminal capture-attempt state; only `Captured` contains an artifact receipt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkbenchCaptureState {
    /// User denied this exact capture.
    Denied,
    /// No scoped backend is available.
    Unavailable,
    /// The scoped backend failed.
    Failed,
    /// Exact selected-window pixels were captured and published.
    Captured,
}

impl WorkbenchCaptureState {
    pub(crate) const fn tag(self) -> u16 {
        match self {
            Self::Denied => 1,
            Self::Unavailable => 2,
            Self::Failed => 3,
            Self::Captured => 4,
        }
    }
    pub(crate) const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Denied),
            2 => Some(Self::Unavailable),
            3 => Some(Self::Failed),
            4 => Some(Self::Captured),
            _ => None,
        }
    }
}

/// Source/build/process-bound selected-window capture receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchCaptureReceipt {
    operation: ControlOperationId,
    state: WorkbenchCaptureState,
    target: WorkbenchCaptureTarget,
    artifact: Option<ArtifactId>,
    image_digest: Option<Sha256Digest>,
    dimensions: Option<(u32, u32)>,
    captured_unix_millis: Option<u64>,
    detail: WorkbenchLaunchText,
}

impl WorkbenchCaptureReceipt {
    /// Validates that only a completed capture carries complete artifact facts.
    ///
    /// # Errors
    /// Rejects partial or contradictory artifact metadata.
    #[allow(clippy::too_many_arguments, reason = "capture evidence fields are independent")]
    pub fn new(
        operation: ControlOperationId,
        state: WorkbenchCaptureState,
        target: WorkbenchCaptureTarget,
        artifact: Option<ArtifactId>,
        image_digest: Option<Sha256Digest>,
        dimensions: Option<(u32, u32)>,
        captured_unix_millis: Option<u64>,
        detail: WorkbenchLaunchText,
    ) -> Result<Self, AppProtocolError> {
        let all = artifact.is_some()
            && image_digest.is_some()
            && dimensions.is_some_and(|(width, height)| width > 0 && height > 0)
            && captured_unix_millis.is_some();
        if (state == WorkbenchCaptureState::Captured) != all {
            return Err(invalid());
        }
        Ok(Self {
            operation,
            state,
            target,
            artifact,
            image_digest,
            dimensions,
            captured_unix_millis,
            detail,
        })
    }
    /// Returns capture operation identity.
    #[must_use]
    pub const fn operation(&self) -> ControlOperationId {
        self.operation
    }
    /// Returns attempt state.
    #[must_use]
    pub const fn state(&self) -> WorkbenchCaptureState {
        self.state
    }
    /// Returns the exact selected target.
    #[must_use]
    pub const fn target(&self) -> WorkbenchCaptureTarget {
        self.target
    }
    /// Returns published artifact identity only for a completed capture.
    #[must_use]
    pub const fn artifact(&self) -> Option<ArtifactId> {
        self.artifact
    }
    /// Returns exact image digest only for a completed capture.
    #[must_use]
    pub const fn image_digest(&self) -> Option<Sha256Digest> {
        self.image_digest
    }
    /// Returns captured dimensions only for a completed capture.
    #[must_use]
    pub const fn dimensions(&self) -> Option<(u32, u32)> {
        self.dimensions
    }
    /// Returns host-observed capture time only for a completed capture.
    #[must_use]
    pub const fn captured_unix_millis(&self) -> Option<u64> {
        self.captured_unix_millis
    }
    /// Borrows inert backend/result detail.
    #[must_use]
    pub const fn detail(&self) -> &WorkbenchLaunchText {
        &self.detail
    }
}

/// Human feedback bound to one immutable capture and optional exact region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchArtifactFeedback {
    operation: ControlOperationId,
    capture: ControlOperationId,
    feedback: WorkbenchReviewFeedback,
    message: WorkbenchInputText,
    region: Option<WorkbenchArtifactRegion>,
}

impl WorkbenchArtifactFeedback {
    /// Creates artifact feedback using the same closed user meaning as P3 review comments.
    #[must_use]
    pub const fn new(
        operation: ControlOperationId,
        capture: ControlOperationId,
        feedback: WorkbenchReviewFeedback,
        message: WorkbenchInputText,
        region: Option<WorkbenchArtifactRegion>,
    ) -> Self {
        Self { operation, capture, feedback, message, region }
    }
    /// Returns feedback operation identity.
    #[must_use]
    pub const fn operation(&self) -> ControlOperationId {
        self.operation
    }
    /// Returns the exact capture anchor.
    #[must_use]
    pub const fn capture(&self) -> ControlOperationId {
        self.capture
    }
    /// Returns reused P3 feedback meaning.
    #[must_use]
    pub const fn feedback(&self) -> WorkbenchReviewFeedback {
        self.feedback
    }
    /// Borrows exact user-authored feedback.
    #[must_use]
    pub const fn message(&self) -> &WorkbenchInputText {
        &self.message
    }
    /// Returns an optional exact image region.
    #[must_use]
    pub const fn region(&self) -> Option<WorkbenchArtifactRegion> {
        self.region
    }
}

//! Explicit shutdown request, acceptance, draining, and completion truth.

use crate::{CorrelationId, RequestId};

use super::{DaemonControlError, DaemonControlErrorKind, error::reject};

/// Closed category of externally active work remaining during shutdown.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RemainingWorkKind {
    /// An application request is still active.
    Request,
    /// A subscription remains active.
    Subscription,
    /// An artifact transfer remains active.
    ArtifactTransfer,
    /// A terminal attachment remains active.
    TerminalAttachment,
    /// Another explicitly described external activity remains.
    Other,
}

/// One bounded externally active work descriptor.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RemainingWork {
    kind: RemainingWorkKind,
    descriptor: String,
}

impl RemainingWork {
    /// Creates a nonempty bounded remaining-work descriptor.
    ///
    /// # Errors
    ///
    /// Rejects a zero text bound or empty/oversized text.
    pub fn new(
        kind: RemainingWorkKind,
        descriptor: String,
        maximum_descriptor_bytes: usize,
    ) -> Result<Self, DaemonControlError> {
        if maximum_descriptor_bytes == 0 {
            return Err(reject(
                DaemonControlErrorKind::InvalidLimit,
                "remaining-work descriptor limit is zero",
            ));
        }
        if descriptor.is_empty() || descriptor.len() > maximum_descriptor_bytes {
            return Err(reject(
                DaemonControlErrorKind::InvalidInput,
                "remaining-work descriptor is empty or exceeds its bound",
            ));
        }
        Ok(Self { kind, descriptor })
    }
    /// Returns the stable work category.
    #[must_use]
    pub const fn kind(&self) -> RemainingWorkKind {
        self.kind
    }
    /// Borrows the inert work descriptor.
    #[must_use]
    pub fn descriptor(&self) -> &str {
        &self.descriptor
    }
}

/// Correlated shutdown request; construction does not imply acceptance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ShutdownRequest {
    request_id: RequestId,
    correlation_id: CorrelationId,
}

impl ShutdownRequest {
    /// Creates an exact shutdown request.
    #[must_use]
    pub const fn new(request_id: RequestId, correlation_id: CorrelationId) -> Self {
        Self { request_id, correlation_id }
    }
    /// Returns the request identity.
    #[must_use]
    pub const fn request_id(self) -> RequestId {
        self.request_id
    }
    /// Returns the correlation identity.
    #[must_use]
    pub const fn correlation_id(self) -> CorrelationId {
        self.correlation_id
    }
}

/// Explicit acceptance of one exact shutdown request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ShutdownAccepted(ShutdownRequest);

impl ShutdownAccepted {
    /// Creates acceptance of an exact request.
    #[must_use]
    pub const fn new(request: ShutdownRequest) -> Self {
        Self(request)
    }
    /// Returns the accepted request.
    #[must_use]
    pub const fn request(self) -> ShutdownRequest {
        self.0
    }
}

/// One bounded draining progress observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShutdownProgress {
    request: ShutdownRequest,
    completed_steps: u32,
    total_steps: u32,
    remaining: Vec<RemainingWork>,
}

impl ShutdownProgress {
    /// Creates progress with bounded remaining work and consistent step accounting.
    ///
    /// # Errors
    ///
    /// Rejects zero bounds/total steps, completed steps above total, or too many work records.
    pub fn new(
        request: ShutdownRequest,
        completed_steps: u32,
        total_steps: u32,
        remaining: Vec<RemainingWork>,
        maximum_remaining_work: usize,
    ) -> Result<Self, DaemonControlError> {
        if maximum_remaining_work == 0 || total_steps == 0 {
            return Err(reject(
                DaemonControlErrorKind::InvalidLimit,
                "shutdown progress bound or total steps is zero",
            ));
        }
        if completed_steps > total_steps || remaining.len() > maximum_remaining_work {
            return Err(reject(
                DaemonControlErrorKind::InvalidInput,
                "shutdown progress is inconsistent or remaining work exceeds its bound",
            ));
        }
        Ok(Self { request, completed_steps, total_steps, remaining })
    }
    /// Returns the shutdown request being drained.
    #[must_use]
    pub const fn request(&self) -> ShutdownRequest {
        self.request
    }
    /// Returns completed progress steps.
    #[must_use]
    pub const fn completed_steps(&self) -> u32 {
        self.completed_steps
    }
    /// Returns total progress steps.
    #[must_use]
    pub const fn total_steps(&self) -> u32 {
        self.total_steps
    }
    /// Borrows bounded externally active work.
    #[must_use]
    pub fn remaining(&self) -> &[RemainingWork] {
        &self.remaining
    }
}

/// Truthful shutdown completion classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShutdownCompletionDisposition {
    /// Shutdown completed cleanly with no externally active work.
    Clean,
    /// Shutdown completed uncleanly; remaining work may be reported.
    Unclean,
}

/// Terminal shutdown completion observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShutdownComplete {
    request: ShutdownRequest,
    disposition: ShutdownCompletionDisposition,
    remaining: Vec<RemainingWork>,
}

impl ShutdownComplete {
    /// Creates bounded completion truth.
    ///
    /// # Errors
    ///
    /// Rejects a zero/exceeded work-record bound or a clean claim with remaining active work.
    pub fn new(
        request: ShutdownRequest,
        disposition: ShutdownCompletionDisposition,
        remaining: Vec<RemainingWork>,
        maximum_remaining_work: usize,
    ) -> Result<Self, DaemonControlError> {
        if maximum_remaining_work == 0 {
            return Err(reject(
                DaemonControlErrorKind::InvalidLimit,
                "shutdown completion remaining-work bound is zero",
            ));
        }
        if remaining.len() > maximum_remaining_work
            || (disposition == ShutdownCompletionDisposition::Clean && !remaining.is_empty())
        {
            return Err(reject(
                DaemonControlErrorKind::InvalidInput,
                "clean completion retains work or work count exceeds its bound",
            ));
        }
        Ok(Self { request, disposition, remaining })
    }
    /// Returns the completed shutdown request.
    #[must_use]
    pub const fn request(&self) -> ShutdownRequest {
        self.request
    }
    /// Returns the exact clean/unclean disposition.
    #[must_use]
    pub const fn disposition(&self) -> ShutdownCompletionDisposition {
        self.disposition
    }
    /// Borrows bounded externally active work remaining at completion.
    #[must_use]
    pub fn remaining(&self) -> &[RemainingWork] {
        &self.remaining
    }
}

/// Observable shutdown lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShutdownPhase {
    /// No shutdown request is retained.
    Running,
    /// A request was observed but is not yet accepted.
    Requested(ShutdownRequest),
    /// The exact request was accepted, but completion is not implied.
    Accepted(ShutdownAccepted),
    /// The daemon reports bounded progress and remaining work.
    Draining(ShutdownProgress),
    /// A clean or unclean completion was explicitly observed.
    Completed(ShutdownComplete),
}

/// Pure shutdown truth state machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShutdownState {
    phase: ShutdownPhase,
}

impl ShutdownState {
    /// Creates a running shutdown-control state.
    #[must_use]
    pub const fn running() -> Self {
        Self { phase: ShutdownPhase::Running }
    }
    /// Borrows the current shutdown phase.
    #[must_use]
    pub const fn phase(&self) -> &ShutdownPhase {
        &self.phase
    }

    /// Records a shutdown request without implying acceptance.
    ///
    /// # Errors
    ///
    /// Rejects a second request after shutdown has started.
    pub fn request(&mut self, request: ShutdownRequest) -> Result<(), DaemonControlError> {
        if self.phase != ShutdownPhase::Running {
            return Err(reject(
                DaemonControlErrorKind::IllegalTransition,
                "shutdown request already exists",
            ));
        }
        self.phase = ShutdownPhase::Requested(request);
        Ok(())
    }

    /// Records explicit acceptance of the exact retained request.
    ///
    /// # Errors
    ///
    /// Rejects wrong correlation or acceptance outside `Requested`.
    pub fn accept(&mut self, accepted: ShutdownAccepted) -> Result<(), DaemonControlError> {
        match self.phase {
            ShutdownPhase::Requested(request) if request == accepted.request() => {
                self.phase = ShutdownPhase::Accepted(accepted);
                Ok(())
            }
            ShutdownPhase::Requested(_) => Err(reject(
                DaemonControlErrorKind::BindingMismatch,
                "acceptance names another shutdown request",
            )),
            _ => Err(reject(
                DaemonControlErrorKind::IllegalTransition,
                "shutdown acceptance requires a retained request",
            )),
        }
    }

    /// Records draining progress for the exact accepted request.
    ///
    /// # Errors
    ///
    /// Rejects wrong correlation, progress regression, or progress outside accepted/draining.
    pub fn progress(&mut self, progress: ShutdownProgress) -> Result<(), DaemonControlError> {
        match &self.phase {
            ShutdownPhase::Accepted(accepted) if accepted.request() == progress.request() => {}
            ShutdownPhase::Draining(previous)
                if previous.request() == progress.request()
                    && progress.completed_steps() >= previous.completed_steps()
                    && progress.total_steps() == previous.total_steps() => {}
            ShutdownPhase::Accepted(_) => {
                return Err(reject(
                    DaemonControlErrorKind::BindingMismatch,
                    "progress names another shutdown request",
                ));
            }
            ShutdownPhase::Draining(previous) if previous.request() != progress.request() => {
                return Err(reject(
                    DaemonControlErrorKind::BindingMismatch,
                    "progress names another shutdown request",
                ));
            }
            ShutdownPhase::Draining(_) => {
                return Err(reject(
                    DaemonControlErrorKind::IllegalTransition,
                    "shutdown progress regresses or changes total step accounting",
                ));
            }
            _ => {
                return Err(reject(
                    DaemonControlErrorKind::IllegalTransition,
                    "shutdown progress requires acceptance",
                ));
            }
        }
        self.phase = ShutdownPhase::Draining(progress);
        Ok(())
    }

    /// Records terminal completion after explicit acceptance or draining.
    ///
    /// # Errors
    ///
    /// Rejects wrong correlation, premature completion, or a conflicting second completion.
    pub fn complete(&mut self, complete: ShutdownComplete) -> Result<(), DaemonControlError> {
        match &self.phase {
            ShutdownPhase::Accepted(accepted) if accepted.request() == complete.request() => {}
            ShutdownPhase::Draining(progress) if progress.request() == complete.request() => {}
            ShutdownPhase::Completed(retained) if retained == &complete => return Ok(()),
            ShutdownPhase::Completed(_) => {
                return Err(reject(
                    DaemonControlErrorKind::TerminalConflict,
                    "completion conflicts with the retained terminal fact",
                ));
            }
            ShutdownPhase::Accepted(_) | ShutdownPhase::Draining(_) => {
                return Err(reject(
                    DaemonControlErrorKind::BindingMismatch,
                    "completion names another shutdown request",
                ));
            }
            ShutdownPhase::Running | ShutdownPhase::Requested(_) => {
                return Err(reject(
                    DaemonControlErrorKind::IllegalTransition,
                    "shutdown completion requires explicit acceptance",
                ));
            }
        }
        self.phase = ShutdownPhase::Completed(complete);
        Ok(())
    }
}

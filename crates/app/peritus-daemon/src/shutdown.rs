//! Pure, bounded coordination state for truthful daemon shutdown observations.

mod work;

use peritus_app_protocol::{
    RemainingWork, ShutdownAccepted, ShutdownComplete, ShutdownCompletionDisposition,
    ShutdownProgress, ShutdownRequest, ShutdownState,
};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

const MAX_SHUTDOWN_FAILURES: usize = 16;

pub(crate) use work::{ShutdownBounds, ShutdownWorkCounts};

/// Source that caused the daemon to begin shutdown.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShutdownTrigger {
    /// An authenticated A3 client supplied an exact correlated request.
    Client(ShutdownRequest),
    /// The operating system requested termination without an A3 correlation.
    OperatingSystemSignal,
}

impl From<Option<ShutdownRequest>> for ShutdownTrigger {
    fn from(request: Option<ShutdownRequest>) -> Self {
        request.map_or(Self::OperatingSystemSignal, Self::Client)
    }
}

/// Ordered shutdown checkpoint established by the effect-owning runtime.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum ShutdownStage {
    /// New client and worker admission has been closed.
    AdmissionClosed,
    /// The local endpoint and its owned connection tasks have joined.
    ConnectionsJoined,
    /// The durable outbox owner has stopped after bounded settlement.
    OutboxSettled,
    /// Supervised worker tasks have undergone bounded shutdown and joining.
    WorkersJoined,
    /// Owned process state has been observed and reconciled.
    ProcessesReconciled,
    /// The serialized authority owner has stopped and joined.
    AuthorityStopped,
}

impl ShutdownStage {
    const TOTAL: u32 = 6;

    /// Returns the stable redaction-safe checkpoint name.
    #[must_use]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::AdmissionClosed => "admission-closed",
            Self::ConnectionsJoined => "connections-joined",
            Self::OutboxSettled => "outbox-settled",
            Self::WorkersJoined => "workers-joined",
            Self::ProcessesReconciled => "processes-reconciled",
            Self::AuthorityStopped => "authority-stopped",
        }
    }

    const fn completed_steps(self) -> u32 {
        match self {
            Self::AdmissionClosed => 1,
            Self::ConnectionsJoined => 2,
            Self::OutboxSettled => 3,
            Self::WorkersJoined => 4,
            Self::ProcessesReconciled => 5,
            Self::AuthorityStopped => Self::TOTAL,
        }
    }
}

/// Final coordinator result for either a client request or an operating-system signal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShutdownOutcome {
    disposition: ShutdownCompletionDisposition,
    remaining: Vec<RemainingWork>,
    failures: Vec<DaemonErrorCode>,
    protocol: Option<ShutdownComplete>,
}

impl ShutdownOutcome {
    /// Returns clean only when every supplied registry count was zero.
    #[must_use]
    pub const fn disposition(&self) -> ShutdownCompletionDisposition {
        self.disposition
    }

    /// Borrows the exact bounded final aggregate summaries.
    #[must_use]
    pub fn remaining(&self) -> &[RemainingWork] {
        &self.remaining
    }

    /// Borrows stable cleanup failure categories in the order they were observed.
    #[must_use]
    pub fn failures(&self) -> &[DaemonErrorCode] {
        &self.failures
    }

    /// Borrows the correlated A3 completion, absent for an operating-system signal.
    #[must_use]
    pub const fn protocol(&self) -> Option<&ShutdownComplete> {
        self.protocol.as_ref()
    }
}

/// Pure shutdown coordinator fed with observations by [`crate::DaemonRuntime`].
///
/// This type neither owns nor executes shutdown effects. A caller records a stage only after the
/// corresponding runtime action and supplies point-in-time counts derived from owned registries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownCoordinator {
    trigger: ShutdownTrigger,
    bounds: ShutdownBounds,
    protocol: Option<ShutdownState>,
    stage: Option<ShutdownStage>,
    counts: ShutdownWorkCounts,
    remaining: Vec<RemainingWork>,
    failures: Vec<DaemonErrorCode>,
    completion: Option<(ShutdownWorkCounts, ShutdownOutcome)>,
}

impl ShutdownCoordinator {
    /// Begins shutdown for an optional client request and establishes its A3 acceptance.
    ///
    /// `None` means an operating-system signal. It deliberately creates no synthetic protocol
    /// identity and therefore emits no A3 progress or completion value.
    ///
    /// # Errors
    ///
    /// Returns an invalid-input error when the bounds cannot encode every exact work category.
    pub(crate) fn begin(
        request: Option<ShutdownRequest>,
        bounds: ShutdownBounds,
    ) -> Result<Self, DaemonError> {
        bounds.validate()?;
        let trigger = ShutdownTrigger::from(request);
        let protocol = if let Some(request) = request {
            let mut state = ShutdownState::running();
            state
                .request(request)
                .map_err(|error| protocol_error("retain shutdown request", error))?;
            state
                .accept(ShutdownAccepted::new(request))
                .map_err(|error| protocol_error("accept shutdown request", error))?;
            Some(state)
        } else {
            None
        };
        Ok(Self {
            trigger,
            bounds,
            protocol,
            stage: None,
            counts: ShutdownWorkCounts::empty(),
            remaining: Vec::new(),
            failures: Vec::new(),
            completion: None,
        })
    }

    /// Returns what caused shutdown without inventing correlation for a signal.
    #[must_use]
    pub(crate) const fn trigger(&self) -> ShutdownTrigger {
        self.trigger
    }

    /// Returns the last runtime checkpoint, if any has been established.
    #[must_use]
    pub(crate) const fn stage(&self) -> Option<ShutdownStage> {
        self.stage
    }

    /// Returns the latest exact registry counts supplied by the runtime.
    #[must_use]
    pub(crate) const fn counts(&self) -> ShutdownWorkCounts {
        self.counts
    }

    /// Borrows the latest fixed-vocabulary, bounded remaining-work summaries.
    #[must_use]
    pub(crate) fn remaining(&self) -> &[RemainingWork] {
        &self.remaining
    }

    /// Borrows A3 shutdown state for a client request, absent for an OS signal.
    #[must_use]
    pub(crate) const fn protocol_state(&self) -> Option<&ShutdownState> {
        self.protocol.as_ref()
    }

    /// Retains one stable operational cleanup failure for the final embedding report.
    ///
    /// # Errors
    ///
    /// Rejects failures recorded after completion or beyond the fixed cleanup-failure bound.
    pub(crate) fn record_failure(&mut self, code: DaemonErrorCode) -> Result<(), DaemonError> {
        if self.completion.is_some() {
            return Err(illegal("shutdown failure cannot be recorded after completion"));
        }
        if self.failures.len() >= MAX_SHUTDOWN_FAILURES {
            return Err(illegal("shutdown failure inventory exceeds its fixed bound"));
        }
        self.failures.push(code);
        Ok(())
    }

    /// Records a non-regressing named checkpoint and the current exact registry counts.
    ///
    /// Repeating the current stage is allowed so a runtime can refresh counts while one bounded
    /// shutdown action is settling. Skipping a stage is also allowed when the omitted subsystem was
    /// not configured; moving backwards is never allowed.
    ///
    /// # Errors
    ///
    /// Returns corrupt state after completion, on stage regression, or if A3 construction rejects
    /// an internally generated bounded observation.
    pub(crate) fn record_stage(
        &mut self,
        stage: ShutdownStage,
        counts: ShutdownWorkCounts,
    ) -> Result<Option<ShutdownProgress>, DaemonError> {
        if self.completion.is_some() {
            return Err(illegal("shutdown stage cannot change after completion"));
        }
        if self.stage.is_some_and(|current| stage < current) {
            return Err(illegal("shutdown stage cannot move backwards"));
        }
        let remaining = work::summarize(counts, self.bounds)?;
        let progress = self
            .client_request()
            .map(|request| {
                ShutdownProgress::new(
                    request,
                    stage.completed_steps(),
                    ShutdownStage::TOTAL,
                    remaining.clone(),
                    self.bounds.maximum_remaining_work(),
                )
            })
            .transpose()
            .map_err(|error| protocol_error("build shutdown progress", error))?;
        if let (Some(state), Some(progress)) = (&mut self.protocol, progress.as_ref()) {
            state
                .progress(progress.clone())
                .map_err(|error| protocol_error("record shutdown progress", error))?;
        }
        self.stage = Some(stage);
        self.counts = counts;
        self.remaining = remaining;
        Ok(progress)
    }

    /// Completes after the final checkpoint using exact final registry counts.
    ///
    /// An identical repeated call is idempotent. A changed final count is a terminal conflict.
    ///
    /// # Errors
    ///
    /// Returns corrupt state before `authority-stopped`, for conflicting repeated completion, or
    /// if A3 rejects the internally generated terminal fact.
    pub(crate) fn complete(
        &mut self,
        counts: ShutdownWorkCounts,
    ) -> Result<ShutdownOutcome, DaemonError> {
        if let Some((retained_counts, outcome)) = &self.completion {
            return if *retained_counts == counts {
                Ok(outcome.clone())
            } else {
                Err(illegal("shutdown completion conflicts with retained final counts"))
            };
        }
        if self.stage != Some(ShutdownStage::AuthorityStopped) {
            return Err(illegal("shutdown completion requires the final named stage"));
        }
        let remaining = work::summarize(counts, self.bounds)?;
        let disposition = if counts.is_empty() {
            ShutdownCompletionDisposition::Clean
        } else {
            ShutdownCompletionDisposition::Unclean
        };
        let protocol = self
            .client_request()
            .map(|request| {
                ShutdownComplete::new(
                    request,
                    disposition,
                    remaining.clone(),
                    self.bounds.maximum_remaining_work(),
                )
            })
            .transpose()
            .map_err(|error| protocol_error("build shutdown completion", error))?;
        if let (Some(state), Some(complete)) = (&mut self.protocol, protocol.as_ref()) {
            state
                .complete(complete.clone())
                .map_err(|error| protocol_error("record shutdown completion", error))?;
        }
        let outcome = ShutdownOutcome {
            disposition,
            remaining: remaining.clone(),
            failures: self.failures.clone(),
            protocol,
        };
        self.counts = counts;
        self.remaining = remaining;
        self.completion = Some((counts, outcome.clone()));
        Ok(outcome)
    }

    const fn client_request(&self) -> Option<ShutdownRequest> {
        match self.trigger {
            ShutdownTrigger::Client(request) => Some(request),
            ShutdownTrigger::OperatingSystemSignal => None,
        }
    }
}

fn protocol_error(
    operation: &'static str,
    error: peritus_app_protocol::DaemonControlError,
) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        operation,
        "shutdown coordinator generated an invalid A3 observation",
        error,
    )
}

fn illegal(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "coordinate daemon shutdown",
        detail,
    )
}

#[cfg(test)]
mod tests;

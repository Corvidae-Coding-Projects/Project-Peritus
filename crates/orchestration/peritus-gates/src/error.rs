//! Stable D1 rejection, error, and recovery vocabulary.

use core::fmt;

/// Pure command/replay rejection category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateRejection {
    /// Contract, descriptor, or snapshot bindings disagree.
    BindingMismatch,
    /// A command is illegal in the current phase.
    IllegalTransition,
    /// A dependency has not passed for this run.
    DependencyUnsatisfied,
    /// A gate, execution, event, or command identity is absent or different.
    IdentityMismatch,
    /// A bounded collection or attempt counter is exhausted.
    LimitExceeded,
    /// Retry was requested without a retryable terminal and required recovery.
    IllegalRetry,
    /// Canonical order, sequence, predecessor, or state digest differs.
    ReplayMismatch,
    /// Evidence is missing, stale, partial, or bound to another attempt.
    EvidenceInvalid,
    /// Cancellation is incomplete or a new effect was requested after cancellation.
    CancellationIncomplete,
}

/// Stable top-level D1 error category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateErrorKind {
    /// Pure domain input or transition rejection.
    Rejected(GateRejection),
    /// B3 canonical encoding or decoding failed.
    Codec,
    /// C0 journal persistence or integrity failed.
    Journal,
    /// C0 artifact persistence failed.
    Artifact,
    /// C0 evidence admission failed.
    Evidence,
    /// C1 immutable workspace inspection failed.
    Workspace,
    /// C4 quality integration failed.
    Quality,
    /// An effect adapter returned an invalid or unavailable observation.
    Executor,
}

/// Stable caller action after a D1 failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateRecoveryAction {
    /// Correct the contract, descriptor, command, or snapshot input.
    CorrectInput,
    /// Reload and replay the complete C0 aggregate.
    ReplayAggregate,
    /// Reconcile an attempt before any fresh dispatch.
    ReconcileAttempt,
    /// Retry a fresh authorized action after the reducer permits it.
    FreshAction,
    /// Repair artifact or evidence publication without rerunning a passing check.
    RepublishEvidence,
    /// Stop the run and require operator integrity handling.
    Quarantine,
}

/// Typed D1 error with bounded safe diagnostic context and optional source.
#[derive(Debug)]
pub struct GateError {
    kind: GateErrorKind,
    recovery: GateRecoveryAction,
    detail: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl GateError {
    pub(crate) fn new(
        kind: GateErrorKind,
        recovery: GateRecoveryAction,
        detail: impl Into<String>,
    ) -> Self {
        let mut detail = detail.into();
        truncate_utf8(&mut detail, 4_096);
        Self { kind, recovery, detail, source: None }
    }

    pub(crate) fn sourced(
        kind: GateErrorKind,
        recovery: GateRecoveryAction,
        detail: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        let mut error = Self::new(kind, recovery, detail);
        error.source = Some(Box::new(source));
        error
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> GateErrorKind {
        self.kind
    }

    /// Returns the required caller recovery action.
    #[must_use]
    pub const fn recovery(&self) -> GateRecoveryAction {
        self.recovery
    }

    /// Borrows bounded context that never includes captured process or workspace content.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for GateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for GateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn std::error::Error + 'static))
    }
}

pub fn reject(kind: GateRejection, detail: &'static str) -> GateError {
    let recovery = match kind {
        GateRejection::ReplayMismatch => GateRecoveryAction::Quarantine,
        GateRejection::CancellationIncomplete | GateRejection::IllegalRetry => {
            GateRecoveryAction::ReconcileAttempt
        }
        GateRejection::EvidenceInvalid => GateRecoveryAction::RepublishEvidence,
        GateRejection::BindingMismatch
        | GateRejection::IllegalTransition
        | GateRejection::DependencyUnsatisfied
        | GateRejection::IdentityMismatch
        | GateRejection::LimitExceeded => GateRecoveryAction::CorrectInput,
    };
    GateError::new(GateErrorKind::Rejected(kind), recovery, detail)
}

fn truncate_utf8(value: &mut String, maximum: usize) {
    if value.len() <= maximum {
        return;
    }
    let mut end = maximum;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
}

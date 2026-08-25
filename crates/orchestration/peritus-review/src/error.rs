//! Stable D2 error and recovery vocabulary.

use core::fmt;

/// Stable top-level review-engine failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReviewErrorKind {
    /// An immutable contract, run, or revision binding differs.
    BindingMismatch,
    /// A zero or above-ceiling limit was supplied.
    InvalidLimit,
    /// A checked collection or byte budget was exhausted.
    LimitExceeded,
    /// A required value is empty, malformed, or contradictory.
    InvalidInput,
    /// A set is duplicated or not in canonical order.
    NonCanonical,
    /// A stable identity was reused or conflicts with prior history.
    IdentityConflict,
    /// A command predecessor, sequence, revision, or digest fence is stale.
    StaleFence,
    /// The requested lifecycle transition is not legal.
    IllegalTransition,
    /// A referenced cycle, finding, reviewer, response, or request is absent.
    UnknownIdentity,
    /// Evidence is missing, stale, or bound to another fact.
    EvidenceInvalid,
    /// An external waiver observation does not match the contract and request.
    WaiverInvalid,
    /// Independent review quorum is not complete.
    QuorumIncomplete,
    /// At least one current finding lacks an authorized conservation disposition.
    FindingUnconserved,
    /// Oscillation or review-cycle exhaustion prevents autonomous completion.
    Oscillation,
    /// Canonical event replay differs from deterministic reduction.
    ReplayMismatch,
    /// Canonical protocol handling failed.
    Codec,
    /// Durable journal handling failed.
    Journal,
    /// Evidence-store handling failed.
    Evidence,
}

/// Stable caller action after a D2 failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReviewRecoveryAction {
    /// Correct a rejected structured input or command.
    CorrectInput,
    /// Reload and replay the complete aggregate.
    ReplayAggregate,
    /// Continue with a fresh independent review cycle.
    ContinueReview,
    /// Obtain an exact external authority observation.
    RequestAuthority,
    /// Escalate the retained history for human disposition.
    NeedsHuman,
    /// Stop and quarantine integrity-sensitive state.
    Quarantine,
}

/// Typed D2 error with bounded, inert diagnostic context.
#[derive(Debug)]
pub struct ReviewError {
    kind: ReviewErrorKind,
    recovery: ReviewRecoveryAction,
    detail: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ReviewError {
    pub(super) fn new(
        kind: ReviewErrorKind,
        recovery: ReviewRecoveryAction,
        detail: impl Into<String>,
    ) -> Self {
        let mut detail = detail.into();
        truncate_utf8(&mut detail, 4_096);
        Self { kind, recovery, detail, source: None }
    }

    pub(super) fn sourced(
        kind: ReviewErrorKind,
        recovery: ReviewRecoveryAction,
        detail: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        let mut error = Self::new(kind, recovery, detail);
        error.source = Some(Box::new(source));
        error
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ReviewErrorKind {
        self.kind
    }

    /// Returns the required caller recovery action.
    #[must_use]
    pub const fn recovery(&self) -> ReviewRecoveryAction {
        self.recovery
    }

    /// Borrows bounded diagnostic context.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ReviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for ReviewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn std::error::Error + 'static))
    }
}

pub fn reject(kind: ReviewErrorKind, detail: &'static str) -> ReviewError {
    let recovery = match kind {
        ReviewErrorKind::WaiverInvalid => ReviewRecoveryAction::RequestAuthority,
        ReviewErrorKind::Oscillation => ReviewRecoveryAction::NeedsHuman,
        ReviewErrorKind::QuorumIncomplete | ReviewErrorKind::FindingUnconserved => {
            ReviewRecoveryAction::ContinueReview
        }
        ReviewErrorKind::BindingMismatch
        | ReviewErrorKind::InvalidLimit
        | ReviewErrorKind::LimitExceeded
        | ReviewErrorKind::InvalidInput
        | ReviewErrorKind::NonCanonical
        | ReviewErrorKind::IdentityConflict
        | ReviewErrorKind::IllegalTransition
        | ReviewErrorKind::UnknownIdentity
        | ReviewErrorKind::EvidenceInvalid => ReviewRecoveryAction::CorrectInput,
        ReviewErrorKind::StaleFence
        | ReviewErrorKind::ReplayMismatch
        | ReviewErrorKind::Codec
        | ReviewErrorKind::Journal
        | ReviewErrorKind::Evidence => ReviewRecoveryAction::Quarantine,
    };
    ReviewError::new(kind, recovery, detail)
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

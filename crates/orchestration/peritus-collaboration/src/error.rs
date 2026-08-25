//! Stable D3 collaboration errors and caller recovery actions.

use core::fmt;

/// Stable collaboration failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CollaborationErrorKind {
    /// An immutable run, scheduler, task, or revision binding differs.
    BindingMismatch,
    /// A zero or above-ceiling limit was supplied.
    InvalidLimit,
    /// A configured allocation or byte budget was exhausted.
    LimitExceeded,
    /// A required value is empty, malformed, or contradictory.
    InvalidInput,
    /// A collection or ordinal is duplicated or noncanonical.
    NonCanonical,
    /// A stable identity has already been consumed.
    IdentityConflict,
    /// A command sequence, predecessor, revision, or digest fence is stale.
    StaleFence,
    /// The requested closed lifecycle transition is illegal.
    IllegalTransition,
    /// A task, message, predecessor, or owner is unknown.
    UnknownIdentity,
    /// The caller is not the retained owner permitted by the domain rule.
    OwnerMismatch,
    /// A child relation would violate root, parent, depth, fan-out, or acyclicity.
    CausalityViolation,
    /// Required-child outcomes do not satisfy the retained join policy.
    JoinUnsatisfied,
    /// Canonical replay differs from deterministic reduction.
    ReplayMismatch,
    /// Canonical protocol handling failed.
    Codec,
    /// Durable C0 journal handling failed.
    Journal,
}

/// Stable caller action after a collaboration failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CollaborationRecoveryAction {
    /// Correct rejected structured input.
    CorrectInput,
    /// Reload and replay the complete aggregate.
    ReplayAggregate,
    /// Wait for required child, delivery, or cancellation acknowledgements.
    AwaitProgress,
    /// Stop and quarantine integrity-sensitive state.
    Quarantine,
}

/// Typed collaboration error with bounded inert detail.
#[derive(Debug)]
pub struct CollaborationError {
    kind: CollaborationErrorKind,
    recovery: CollaborationRecoveryAction,
    detail: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl CollaborationError {
    pub(super) fn new(
        kind: CollaborationErrorKind,
        recovery: CollaborationRecoveryAction,
        detail: impl Into<String>,
    ) -> Self {
        let mut detail = detail.into();
        truncate_utf8(&mut detail, 4_096);
        Self { kind, recovery, detail, source: None }
    }

    pub(super) fn sourced(
        kind: CollaborationErrorKind,
        recovery: CollaborationRecoveryAction,
        detail: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        let mut error = Self::new(kind, recovery, detail);
        error.source = Some(Box::new(source));
        error
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn kind(&self) -> CollaborationErrorKind {
        self.kind
    }

    /// Returns the required caller action.
    #[must_use]
    pub const fn recovery(&self) -> CollaborationRecoveryAction {
        self.recovery
    }

    /// Borrows bounded diagnostic context.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for CollaborationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for CollaborationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn std::error::Error + 'static))
    }
}

pub fn reject(kind: CollaborationErrorKind, detail: &'static str) -> CollaborationError {
    let recovery = match kind {
        CollaborationErrorKind::JoinUnsatisfied => CollaborationRecoveryAction::AwaitProgress,
        CollaborationErrorKind::StaleFence
        | CollaborationErrorKind::ReplayMismatch
        | CollaborationErrorKind::Codec
        | CollaborationErrorKind::Journal => CollaborationRecoveryAction::Quarantine,
        _ => CollaborationRecoveryAction::CorrectInput,
    };
    CollaborationError::new(kind, recovery, detail)
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

//! Stable scheduler failure and recovery vocabulary.

use core::fmt;

/// Stable scheduler failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SchedulerErrorKind {
    /// A run, revision, scheduler, or owner binding differs.
    BindingMismatch,
    /// A configured limit is zero or above its production ceiling.
    InvalidLimit,
    /// A configured collection or byte budget is exhausted.
    LimitExceeded,
    /// A required input is empty, malformed, or contradictory.
    InvalidInput,
    /// A collection is duplicated or not in canonical order.
    NonCanonical,
    /// A stable identity conflicts with retained history.
    IdentityConflict,
    /// A command fence does not name the exact current state.
    StaleFence,
    /// A lifecycle transition is not legal.
    IllegalTransition,
    /// A referenced worker, work item, dependency, parent, or dispatch is absent.
    UnknownIdentity,
    /// Resource arithmetic or capacity validation failed.
    ResourceConflict,
    /// No deterministic feasible dispatch exists.
    NoFeasibleWork,
    /// Canonical replay differs from deterministic reduction.
    ReplayMismatch,
    /// Canonical protocol handling failed.
    Codec,
    /// Durable journal handling failed.
    Journal,
}

/// Stable caller action after scheduler failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SchedulerRecoveryAction {
    /// Correct rejected structured input.
    CorrectInput,
    /// Retry after worker/resource/dependency state changes.
    RetryLater,
    /// Reload and replay the aggregate.
    ReplayAggregate,
    /// Stop and quarantine integrity-sensitive bytes.
    Quarantine,
}

/// Typed bounded scheduler error.
#[derive(Debug)]
pub struct SchedulerError {
    kind: SchedulerErrorKind,
    recovery: SchedulerRecoveryAction,
    detail: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl SchedulerError {
    pub(crate) fn new(
        kind: SchedulerErrorKind,
        recovery: SchedulerRecoveryAction,
        detail: impl Into<String>,
    ) -> Self {
        let mut detail = detail.into();
        truncate_utf8(&mut detail, 4_096);
        Self { kind, recovery, detail, source: None }
    }

    pub(crate) fn sourced(
        kind: SchedulerErrorKind,
        recovery: SchedulerRecoveryAction,
        detail: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        let mut value = Self::new(kind, recovery, detail);
        value.source = Some(Box::new(source));
        value
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> SchedulerErrorKind {
        self.kind
    }

    /// Returns the caller recovery action.
    #[must_use]
    pub const fn recovery(&self) -> SchedulerRecoveryAction {
        self.recovery
    }

    /// Borrows bounded inert diagnostic context.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for SchedulerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn std::error::Error + 'static))
    }
}

pub fn reject(kind: SchedulerErrorKind, detail: &'static str) -> SchedulerError {
    let recovery = match kind {
        SchedulerErrorKind::NoFeasibleWork => SchedulerRecoveryAction::RetryLater,
        SchedulerErrorKind::StaleFence | SchedulerErrorKind::ReplayMismatch => {
            SchedulerRecoveryAction::ReplayAggregate
        }
        SchedulerErrorKind::Codec | SchedulerErrorKind::Journal => {
            SchedulerRecoveryAction::Quarantine
        }
        _ => SchedulerRecoveryAction::CorrectInput,
    };
    SchedulerError::new(kind, recovery, detail)
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

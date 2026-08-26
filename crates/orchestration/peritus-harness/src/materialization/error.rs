//! Typed materialization failures and recovery guidance.

use core::fmt;

/// Stable materialization failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MaterializationErrorKind {
    /// A plan field or relationship was invalid.
    InvalidPlan,
    /// A bounded collection or byte total exceeded its limit.
    LimitExceeded,
    /// The workspace identity no longer matched the plan.
    StaleWorkspace,
    /// A path was not proven to belong to the harness.
    PathOwnership,
    /// A content artifact was missing, inactive, corrupt, or mismatched.
    Artifact,
    /// C1 rejected construction or application of the patch.
    Patch,
    /// C1 rejected authorization or candidate creation.
    Workspace,
    /// A receipt did not bind the exact plan and C1 observations.
    Receipt,
    /// A canonical representation was malformed or noncanonical.
    Codec,
    /// An identity was reused with a different payload.
    Conflict,
}

/// Required caller response to a materialization failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MaterializationRecovery {
    /// Correct the requested manifest, plan, or receipt.
    CorrectInput,
    /// Reobserve the immutable target and build a fresh plan.
    Reobserve,
    /// Obtain fresh target-owned authorization.
    Reauthorize,
    /// Retry the same idempotent directive.
    Retry,
    /// Reconcile exact C1 and C0 observations before continuing.
    Reconcile,
    /// Isolate the conflicting durable state for operator review.
    Quarantine,
}

/// Comparable typed materialization error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializationError {
    kind: MaterializationErrorKind,
    recovery: MaterializationRecovery,
    detail: String,
}

impl MaterializationError {
    pub(crate) fn new(
        kind: MaterializationErrorKind,
        recovery: MaterializationRecovery,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, recovery, detail: detail.into() }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> MaterializationErrorKind {
        self.kind
    }

    /// Returns the required recovery action.
    #[must_use]
    pub const fn recovery(&self) -> MaterializationRecovery {
        self.recovery
    }

    /// Returns bounded diagnostic context.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for MaterializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "harness materialization failed ({:?}): {}", self.kind, self.detail)
    }
}

impl std::error::Error for MaterializationError {}

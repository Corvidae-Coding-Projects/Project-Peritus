//! Stable redaction-safe product runner failures.

use core::fmt;

/// Stable product-run failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunnerErrorKind {
    /// Caller input or a required initial authority binding was invalid.
    InvalidPrecondition,
    /// Managed repository inspection failed.
    Repository,
    /// A provider request or response failed.
    Provider,
    /// A model response did not satisfy the edit/review contract.
    InvalidModelOutput,
    /// A concrete developer workspace edit or tool operation failed structurally.
    Apply,
    /// Repository gates could not be executed.
    Gate,
    /// Accounting, execution capacity, or a caller-selected resource policy rejected work.
    Budget,
    /// The user cancelled the run.
    Cancelled,
    /// A supposedly impossible internal state transition was rejected.
    InternalInvariant,
}

/// Exact failure provenance, separate from the compatibility error category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunnerFailureCause {
    /// Initial input or current authority was rejected.
    Admission,
    /// Repository evidence could not be inspected or retained.
    RepositoryIntegrity,
    /// A provider reached a failure boundary; acceptance must remain independently known.
    Provider,
    /// Typed host context preparation or checked compaction failed.
    ContextPreparation,
    /// A model or report adapter rejected its input.
    Adapter,
    /// An owned effect requires reconciliation.
    OwnedEffect,
    /// Gate execution or qualification failed.
    Gate,
    /// A legacy budget-category failure supplied no evidence of elapsed expiry.
    UnclassifiedBudget,
    /// An exact accounting counter cannot represent the next observation.
    AccountingRepresentation,
    /// Host telemetry could not produce a trustworthy observation.
    ResourceObservation,
    /// The caller's explicitly selected elapsed horizon expired.
    SelectedDeadline,
    /// The user cancelled work.
    Cancellation,
    /// An internal invariant was rejected.
    InternalInvariant,
}

impl ProductRunnerFailureCause {
    /// Stable redaction-safe diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::RepositoryIntegrity => "repository_integrity",
            Self::Provider => "provider",
            Self::ContextPreparation => "context_preparation",
            Self::Adapter => "adapter",
            Self::OwnedEffect => "owned_effect",
            Self::Gate => "gate",
            Self::UnclassifiedBudget => "unclassified_budget",
            Self::AccountingRepresentation => "accounting_representation",
            Self::ResourceObservation => "resource_observation",
            Self::SelectedDeadline => "selected_deadline",
            Self::Cancellation => "cancellation",
            Self::InternalInvariant => "internal_invariant",
        }
    }
}

/// Redaction-safe product runner error.
#[derive(Clone, Debug)]
pub struct ProductRunnerError {
    kind: ProductRunnerErrorKind,
    failure_cause: ProductRunnerFailureCause,
    provider_failure: Option<Box<peritus_model_protocol::ModelFailure>>,
    operation: &'static str,
    detail: String,
}

impl ProductRunnerError {
    pub(crate) fn new(
        kind: ProductRunnerErrorKind,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        let failure_cause = match kind {
            ProductRunnerErrorKind::InvalidPrecondition => ProductRunnerFailureCause::Admission,
            ProductRunnerErrorKind::Repository => ProductRunnerFailureCause::RepositoryIntegrity,
            ProductRunnerErrorKind::Provider => ProductRunnerFailureCause::Provider,
            ProductRunnerErrorKind::InvalidModelOutput => ProductRunnerFailureCause::Adapter,
            ProductRunnerErrorKind::Apply => ProductRunnerFailureCause::OwnedEffect,
            ProductRunnerErrorKind::Gate => ProductRunnerFailureCause::Gate,
            ProductRunnerErrorKind::Budget => ProductRunnerFailureCause::UnclassifiedBudget,
            ProductRunnerErrorKind::Cancelled => ProductRunnerFailureCause::Cancellation,
            ProductRunnerErrorKind::InternalInvariant => {
                ProductRunnerFailureCause::InternalInvariant
            }
        };
        Self { kind, failure_cause, provider_failure: None, operation, detail: detail.into() }
    }

    pub(crate) const fn with_failure_cause(mut self, cause: ProductRunnerFailureCause) -> Self {
        self.failure_cause = cause;
        self
    }

    pub(crate) fn with_provider_failure(
        mut self,
        failure: Box<peritus_model_protocol::ModelFailure>,
    ) -> Self {
        self.provider_failure = Some(failure);
        self
    }

    /// Exact typed origin; a Budget category alone is never proof of a deadline.
    #[must_use]
    pub const fn failure_cause(&self) -> ProductRunnerFailureCause {
        self.failure_cause
    }

    /// Original normalized provider terminal, including acceptance and exact response identity.
    #[must_use]
    pub fn provider_failure(&self) -> Option<&peritus_model_protocol::ModelFailure> {
        self.provider_failure.as_deref()
    }

    /// Recovery for this cause always refers to the retained logical run.
    #[must_use]
    pub const fn recovery_action(&self) -> &'static str {
        match self.failure_cause {
            ProductRunnerFailureCause::SelectedDeadline => {
                "review the selected elapsed policy and resume this retained run"
            }
            ProductRunnerFailureCause::AccountingRepresentation => {
                "reconcile the exact accounting state before resuming this run"
            }
            ProductRunnerFailureCause::ResourceObservation => {
                "restore resource observation without replacing this run or its owned effects"
            }
            ProductRunnerFailureCause::ContextPreparation => {
                "restore retained context preparation within the selected provider envelope"
            }
            ProductRunnerFailureCause::Provider => {
                "reconcile provider acceptance before retrying or resuming this run"
            }
            ProductRunnerFailureCause::RepositoryIntegrity => {
                "restore repository inspection and reconcile the retained candidate"
            }
            ProductRunnerFailureCause::Admission => {
                "restore required authority before continuing the retained work"
            }
            ProductRunnerFailureCause::Cancellation => {
                "reconcile any owned effects before explicitly resuming this run"
            }
            _ => "reconcile the original failure and owned effects before resuming this run",
        }
    }

    pub(crate) fn settlement_detail(&self) -> String {
        let mut detail = format!(
            "{}: {} [cause={}; recovery={}]",
            self.operation,
            self.detail,
            self.failure_cause.code(),
            self.recovery_action(),
        );
        if let Some(failure) = self.provider_failure() {
            detail.push_str(&format!(
                " [provider_phase={:?}; acceptance={:?}; retryability={:?}]",
                failure.phase(),
                failure.certainty(),
                failure.retryability(),
            ));
        }
        detail
    }

    /// Stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ProductRunnerErrorKind {
        self.kind
    }
    /// Failed operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }
    /// Redaction-safe diagnostic.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for ProductRunnerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)
    }
}

impl std::error::Error for ProductRunnerError {}

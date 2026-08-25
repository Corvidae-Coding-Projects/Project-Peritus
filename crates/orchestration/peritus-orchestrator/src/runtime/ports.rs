//! Narrow effect and observation ports; no concrete provider or workspace dependencies.

use peritus_quality_policy::{AcceptanceDecision, AcceptanceEvidence};
use peritus_spec::AcceptanceContract;
use peritus_types::Sha256Digest;

use crate::{
    CandidateBinding, ChildAggregateKind, ChildHead, ChildObservation, DirectiveId,
    OrchestratorError, PendingDirective,
};

/// Exact acknowledgement returned after publishing a committed directive.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DirectiveReceipt {
    directive_id: DirectiveId,
    payload_digest: Sha256Digest,
}

impl DirectiveReceipt {
    /// Creates an inert transport acknowledgement for later reducer validation.
    #[must_use]
    pub const fn new(directive_id: DirectiveId, payload_digest: Sha256Digest) -> Self {
        Self { directive_id, payload_digest }
    }

    /// Returns the acknowledged directive.
    #[must_use]
    pub const fn directive_id(self) -> DirectiveId {
        self.directive_id
    }

    /// Returns the acknowledged exact payload digest.
    #[must_use]
    pub const fn payload_digest(self) -> Sha256Digest {
        self.payload_digest
    }

    /// Returns whether the receipt acknowledges the exact committed directive.
    #[must_use]
    pub fn matches(self, directive: &PendingDirective) -> bool {
        self.directive_id == directive.id() && self.payload_digest == directive.payload_digest()
    }
}

/// Publishes only an already-committed stable directive.
pub trait DirectivePublisher {
    /// Performs at most one idempotent delivery attempt.
    ///
    /// # Errors
    /// Returns a typed external failure without mutating E0 authoritative state.
    fn publish(
        &mut self,
        directive: &PendingDirective,
    ) -> Result<DirectiveReceipt, OrchestratorError>;
}

/// Current child truth observed during restart reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChildReconciliation {
    /// The destination has not durably admitted the directive.
    Absent,
    /// Exact work exists and remains active.
    Active,
    /// Exact terminal truth is ready for an E0 observation command.
    Completed(Box<ChildObservation>),
    /// Evidence-backed non-success truth can settle cancellation without a child terminal.
    Classified(crate::child::CancellationChildClassification),
    /// Child identity or revision conflicts with the directive.
    Conflicting,
    /// External ownership or outcome cannot be established automatically.
    Ambiguous,
}

/// Loads exact public child projections without executing new work.
pub trait ChildProjectionPort {
    /// Loads the exact current nonterminal head for one actively owned child aggregate.
    ///
    /// # Errors
    ///
    /// Returns a typed load, codec, or identity failure.
    fn active_head(&mut self, kind: ChildAggregateKind) -> Result<ChildHead, OrchestratorError>;

    /// Reconciles one committed directive against its destination aggregate.
    ///
    /// # Errors
    /// Returns a typed load/codec failure distinct from an observed ambiguous child.
    fn reconcile(
        &mut self,
        directive: &PendingDirective,
    ) -> Result<ChildReconciliation, OrchestratorError>;
}

/// Pure B2 evaluation boundary used by the runtime without retaining the private decision shape.
pub trait AcceptanceEvaluationPort {
    /// Evaluates the exact contract, candidate revision, and canonical evidence.
    fn evaluate(
        &mut self,
        contract: &AcceptanceContract,
        candidate: &CandidateBinding,
        evidence: &AcceptanceEvidence,
    ) -> AcceptanceDecision;
}

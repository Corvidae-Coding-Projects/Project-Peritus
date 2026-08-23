//! Private representation and public unprivileged lifecycle projections.

use peritus_policy::AuthorityInstant;
use peritus_types::{CommandId, Generation, RevisionNumber, Sha256Digest};
use vstd::prelude::*;

verus! {

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct Resolution {
    pub(super) decision_digest: crate::ApprovalDecisionDigest,
    pub(super) command_id: CommandId,
    pub(super) choice: crate::ApprovalChoice,
    pub(super) registry_revision: RevisionNumber,
    pub(super) registry_digest: Sha256Digest,
    pub(super) credential_generation: Generation,
    pub(super) valid_until: AuthorityInstant,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ApprovalState {
    Pending,
    ApprovedOnce(Resolution),
    AmendmentAuthorized(Resolution),
    Consumed(Resolution),
    Amended(Resolution),
    Denied(Resolution),
    Expired(Option<Resolution>),
    Cancelled,
}

/// Immutable unprivileged facts about the accepted terminal response.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ApprovalResolutionFacts {
    decision_digest: crate::ApprovalDecisionDigest,
    command_id: CommandId,
    choice: crate::ApprovalChoice,
    registry_revision: RevisionNumber,
    registry_digest: Sha256Digest,
    credential_generation: Generation,
    valid_until: AuthorityInstant,
}

impl ApprovalResolutionFacts {
    /// Returns the exact semantic response digest.
    #[must_use]
    pub const fn decision_digest(&self) -> crate::ApprovalDecisionDigest { self.decision_digest }

    /// Returns the accepted command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId { self.command_id }

    /// Returns the accepted decision choice and amendment identity, if any.
    #[must_use]
    pub const fn choice(&self) -> crate::ApprovalChoice { self.choice }

    /// Returns the supplied registry revision used for authentication.
    #[must_use]
    pub const fn registry_revision(&self) -> RevisionNumber { self.registry_revision }

    /// Returns the digest of the exact registry snapshot used for authentication.
    #[must_use]
    pub const fn registry_digest(&self) -> Sha256Digest { self.registry_digest }

    /// Returns the authenticated credential generation.
    #[must_use]
    pub const fn credential_generation(&self) -> Generation { self.credential_generation }

    /// Returns the accepted response's exclusive authority expiry.
    #[must_use]
    pub const fn valid_until(&self) -> AuthorityInstant { self.valid_until }
}

/// Complete move-only approval aggregate for one exact request.
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalAggregate {
    pub(super) request: crate::ApprovalRequest,
    pub(super) state: ApprovalState,
}

#[allow(
    non_shorthand_field_patterns,
    reason = "pinned Verus expands move-only destructures to explicit field patterns"
)]
impl ApprovalAggregate {
    /// Returns exact immutable accepted-response facts, when this aggregate retains a resolution.
    pub closed spec fn spec_resolution_facts(&self) -> Option<ApprovalResolutionFacts> {
        match self.state {
            ApprovalState::ApprovedOnce(value)
            | ApprovalState::AmendmentAuthorized(value)
            | ApprovalState::Consumed(value)
            | ApprovalState::Amended(value)
            | ApprovalState::Denied(value)
            | ApprovalState::Expired(Some(value)) => Some(ApprovalResolutionFacts {
                decision_digest: value.decision_digest,
                command_id: value.command_id,
                choice: value.choice,
                registry_revision: value.registry_revision,
                registry_digest: value.registry_digest,
                credential_generation: value.credential_generation,
                valid_until: value.valid_until,
            }),
            ApprovalState::Pending
            | ApprovalState::Expired(None)
            | ApprovalState::Cancelled => None,
        }
    }

    /// Returns the exact lifecycle phase used by public reducer contracts.
    pub closed spec fn spec_phase(&self) -> crate::ApprovalPhase {
        super::exact::state_phase(self.state)
    }

    /// Borrows the exact request and current authority-time floor.
    #[must_use]
    pub const fn request(&self) -> &crate::ApprovalRequest { &self.request }

    /// Returns the public lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> (phase: crate::ApprovalPhase)
        ensures phase == self.spec_phase(),
    {
        match self.state {
            ApprovalState::Pending => crate::ApprovalPhase::Pending,
            ApprovalState::ApprovedOnce(_) => crate::ApprovalPhase::ApprovedOnce,
            ApprovalState::AmendmentAuthorized(_) => crate::ApprovalPhase::AmendmentAuthorized,
            ApprovalState::Consumed(_) => crate::ApprovalPhase::Consumed,
            ApprovalState::Amended(_) => crate::ApprovalPhase::Amended,
            ApprovalState::Denied(_) => crate::ApprovalPhase::Denied,
            ApprovalState::Expired(_) => crate::ApprovalPhase::Expired,
            ApprovalState::Cancelled => crate::ApprovalPhase::Cancelled,
        }
    }

    /// Returns immutable accepted-response facts without granting transition authority.
    #[must_use]
    pub const fn resolution(&self) -> (facts: Option<ApprovalResolutionFacts>)
        ensures facts == self.spec_resolution_facts(),
    {
        match self.state {
            ApprovalState::ApprovedOnce(value)
            | ApprovalState::AmendmentAuthorized(value)
            | ApprovalState::Consumed(value)
            | ApprovalState::Amended(value)
            | ApprovalState::Denied(value)
            | ApprovalState::Expired(Some(value)) => Some(ApprovalResolutionFacts {
                decision_digest: value.decision_digest,
                command_id: value.command_id,
                choice: value.choice,
                registry_revision: value.registry_revision,
                registry_digest: value.registry_digest,
                credential_generation: value.credential_generation,
                valid_until: value.valid_until,
            }),
            ApprovalState::Pending
            | ApprovalState::Expired(None)
            | ApprovalState::Cancelled => None,
        }
    }
}

/// Accepted logical state transition kind.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApprovalTransitionKind {
    /// A pending request accepted its first semantic decision.
    Resolved,
    /// An exact repeated decision produced no second transition.
    Idempotent,
    /// An unresolved or unconsumed request reached an exclusive expiry.
    Expired,
    /// A pending request was cancelled.
    Cancelled,
}

/// One unprivileged logical approval transition record.
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalTransition {
    pub(super) kind: ApprovalTransitionKind,
    pub(super) from: crate::ApprovalPhase,
    pub(super) to: crate::ApprovalPhase,
    pub(super) decision_digest: Option<crate::ApprovalDecisionDigest>,
    pub(super) registry_revision: Option<RevisionNumber>,
}

impl ApprovalTransition {
    /// Returns the transition kind.
    #[must_use]
    pub const fn kind(&self) -> ApprovalTransitionKind { self.kind }

    /// Returns the source phase.
    #[must_use]
    pub const fn from(&self) -> crate::ApprovalPhase { self.from }

    /// Returns the successor phase.
    #[must_use]
    pub const fn to(&self) -> crate::ApprovalPhase { self.to }

    /// Returns the exact decision digest, when this transition has one.
    #[must_use]
    pub const fn decision_digest(&self) -> Option<crate::ApprovalDecisionDigest> {
        self.decision_digest
    }

    /// Returns the supplied credential-registry revision, when authenticated.
    #[must_use]
    pub const fn registry_revision(&self) -> Option<RevisionNumber> { self.registry_revision }
}

/// Successful move-only state transition.
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalTransitionOutcome {
    pub(super) aggregate: ApprovalAggregate,
    pub(super) transition: ApprovalTransition,
}

impl ApprovalTransitionOutcome {
    /// Returns the accepted aggregate's closed logical model projection.
    pub closed spec fn spec_model(&self) -> crate::model::ApprovalModelState {
        self.aggregate.spec_model()
    }

    pub(super) proof fn prove_model(&self)
        ensures self.spec_model() == self.aggregate.spec_model(),
    {
    }

    /// Borrows the exact successor aggregate.
    #[must_use]
    pub const fn aggregate(&self) -> &ApprovalAggregate { &self.aggregate }

    /// Borrows the unprivileged transition record.
    #[must_use]
    pub const fn transition(&self) -> &ApprovalTransition { &self.transition }

    /// Consumes the outcome into its move-only parts.
    #[must_use]
    pub fn into_parts(self) -> (ApprovalAggregate, ApprovalTransition) {
        (self.aggregate, self.transition)
    }
}

} // verus!

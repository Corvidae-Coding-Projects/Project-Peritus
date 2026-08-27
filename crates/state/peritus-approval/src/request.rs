//! Canonical bounded approval requests and participant facts.

use peritus_policy::{
    ActorRole, ApprovalRequirement, AuthorityInstant, AuthorityTimeState, CapabilityScope,
    EscalationChallenge, RiskSet, ValidityWindow,
};
use peritus_types::{ActionId, ActorId, ApprovalRequestId, Generation, Sha256Digest};
use vstd::prelude::*;

mod participant;
mod projection;
mod specification;

verus! {

/// Maximum exact permissions bound into one request.
pub const MAX_APPROVAL_PERMISSIONS: usize = 256;
/// Maximum structured risk variants bound into one request.
pub const MAX_RISK_CLASSES: usize = 10;
/// Maximum exact independence requirements bound into one request.
pub const MAX_INDEPENDENCE_REQUIREMENTS: usize = 4;
/// Maximum producing-attempt participants bound into one request.
pub const MAX_PRODUCING_PARTICIPANTS: usize = 256;
/// Maximum review participants bound into one request.
pub const MAX_REVIEW_PARTICIPANTS: usize = 256;
/// Maximum canonical request digest preimage size.
pub const MAX_APPROVAL_REQUEST_PREIMAGE_BYTES: usize = 65_536;

/// Canonical duplicate-free actor participants for one provenance category.
#[derive(Debug, Eq, PartialEq)]
pub struct ParticipantSet {
    pub(crate) values: Vec<ActorId>,
}

/// One exact bounded approval request with its move-only authority-time floor.
#[derive(Debug, Eq, PartialEq)]
pub struct ApprovalRequest {
    pub(crate) request_id: ApprovalRequestId,
    pub(crate) action_id: ActionId,
    pub(crate) action_digest: crate::ActionDigest,
    pub(crate) requester: ActorId,
    pub(crate) requester_role: ActorRole,
    pub(crate) scope: CapabilityScope,
    pub(crate) requirement: ApprovalRequirement,
    pub(crate) evaluated_at: AuthorityInstant,
    pub(crate) challenge_epoch: Generation,
    pub(crate) challenge_tick_millis: u64,
    pub(crate) authority_time: AuthorityTimeState,
    pub(crate) risks: RiskSet,
    pub(crate) risk_details_digest: Sha256Digest,
    pub(crate) producing_participants: ParticipantSet,
    pub(crate) review_participants: ParticipantSet,
    pub(crate) validity: ValidityWindow,
    pub(crate) digest: crate::ApprovalRequestDigest,
}

impl ApprovalRequest {
    /// Returns the exact challenged scope used by specifications.
    pub closed spec fn spec_scope(&self) -> CapabilityScope { self.scope }

    /// Returns the exact conjoined approval requirement used by specifications.
    pub closed spec fn spec_requirement(&self) -> ApprovalRequirement { self.requirement }

    /// Returns the exact request validity used by specifications.
    pub closed spec fn spec_validity(&self) -> ValidityWindow { self.validity }

    /// Returns the exact ordered authority-time failure for one observation.
    pub closed spec fn spec_observation_time_error(
        &self,
        observed_at: AuthorityInstant,
    ) -> Option<crate::ApprovalError> {
        if observed_at.spec_epoch() != self.authority_time.spec_epoch() {
            Some(crate::ApprovalError::ClockEpochMismatch)
        } else if observed_at.spec_tick_millis()
            < self.authority_time.spec_greatest_tick_millis()
        {
            Some(crate::ApprovalError::ClockRegression)
        } else {
            None
        }
    }

    /// Returns the request identity.
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId { self.request_id }

    /// Returns the action identity.
    #[must_use]
    pub const fn action_id(&self) -> ActionId { self.action_id }

    /// Returns the exact externally refined action digest.
    #[must_use]
    pub const fn action_digest(&self) -> crate::ActionDigest { self.action_digest }

    /// Returns the requesting actor.
    #[must_use]
    pub const fn requester(&self) -> ActorId { self.requester }

    /// Returns the requesting actor's policy role.
    #[must_use]
    pub const fn requester_role(&self) -> ActorRole { self.requester_role }

    /// Borrows the complete challenged capability scope.
    #[must_use]
    pub const fn scope(&self) -> (scope: &CapabilityScope)
        ensures *scope == self.spec_scope(),
    { &self.scope }

    /// Borrows the complete conjoined approval requirement.
    #[must_use]
    pub const fn requirement(&self) -> (requirement: &ApprovalRequirement)
        ensures *requirement == self.spec_requirement(),
    { &self.requirement }

    /// Returns the policy evaluation instant.
    #[must_use]
    pub const fn evaluated_at(&self) -> AuthorityInstant { self.evaluated_at }

    /// Borrows the current move-only authority-time floor.
    #[must_use]
    pub const fn authority_time(&self) -> &AuthorityTimeState { &self.authority_time }

    /// Borrows canonical structured risk classes.
    #[must_use]
    pub const fn risks(&self) -> &RiskSet { &self.risks }

    /// Returns the redacted structured-risk details digest.
    #[must_use]
    pub const fn risk_details_digest(&self) -> Sha256Digest { self.risk_details_digest }

    /// Borrows producing-attempt participant claims.
    #[must_use]
    pub const fn producing_participants(&self) -> &ParticipantSet {
        &self.producing_participants
    }

    /// Borrows review participant claims.
    #[must_use]
    pub const fn review_participants(&self) -> &ParticipantSet { &self.review_participants }

    /// Returns the request's own validity window.
    #[must_use]
    pub const fn validity(&self) -> (validity: ValidityWindow)
        ensures validity == self.spec_validity(),
    { self.validity }

    /// Returns the exact semantic request digest.
    #[must_use]
    pub const fn digest(&self) -> crate::ApprovalRequestDigest { self.digest }

    pub(crate) const fn validate_observation_time(
        &self,
        observed_at: AuthorityInstant,
    ) -> (result: Result<(), crate::ApprovalError>)
        ensures
            match result {
                Ok(()) => self.spec_observation_time_error(observed_at).is_none(),
                Err(error) => self.spec_observation_time_error(observed_at) == Some(error),
            },
    {
        if observed_at.epoch().get() != self.authority_time.epoch().get() {
            Err(crate::ApprovalError::ClockEpochMismatch)
        } else if observed_at.tick_millis() < self.authority_time.greatest_tick_millis() {
            Err(crate::ApprovalError::ClockRegression)
        } else {
            Ok(())
        }
    }

    pub(crate) proof fn observation_time_ok(
        &self,
        observed_at: AuthorityInstant,
    )
        requires self.spec_observation_time_error(observed_at).is_none(),
        ensures
            observed_at.spec_epoch() == self.authority_time.spec_epoch(),
            observed_at.spec_tick_millis()
                >= self.authority_time.spec_greatest_tick_millis(),
    {
        reveal(ApprovalRequest::spec_observation_time_error);
    }

}

impl ApprovalRequest {
    #[allow(clippy::too_many_arguments, reason = "every authority field is explicit and digest-bound")]
    fn new_with_digest(
        request_id: ApprovalRequestId,
        action_id: ActionId,
        action_digest: crate::ActionDigest,
        requester: ActorId,
        requester_role: ActorRole,
        challenge: EscalationChallenge,
        risk_details_digest: Sha256Digest,
        producing_participants: ParticipantSet,
        review_participants: ParticipantSet,
        validity: ValidityWindow,
        digest: crate::ApprovalRequestDigest,
    ) -> Result<Self, crate::ApprovalError> {
        let (scope, requirement, risks, evaluated_at, authority_time) = challenge.into_parts();
        if scope.permissions().as_slice().len() > MAX_APPROVAL_PERMISSIONS {
            return Err(crate::ApprovalError::CollectionTooLarge(
                crate::CanonicalCollection::Permissions,
            ));
        }
        if risks.as_slice().len() > MAX_RISK_CLASSES {
            return Err(crate::ApprovalError::CollectionTooLarge(
                crate::CanonicalCollection::Risks,
            ));
        }
        if requirement.independence().as_slice().len() > MAX_INDEPENDENCE_REQUIREMENTS {
            return Err(crate::ApprovalError::CollectionTooLarge(
                crate::CanonicalCollection::IndependenceRequirements,
            ));
        }
        let challenge_epoch = authority_time.epoch();
        let challenge_tick_millis = authority_time.greatest_tick_millis();
        Ok(Self {
            request_id,
            action_id,
            action_digest,
            requester,
            requester_role,
            scope,
            requirement,
            evaluated_at,
            challenge_epoch,
            challenge_tick_millis,
            authority_time,
            risks,
            risk_details_digest,
            producing_participants,
            review_participants,
            validity,
            digest,
        })
    }
}

} // verus!

impl ApprovalRequest {
    /// Reconstructs one canonical decoded request from already checked policy value types.
    ///
    /// This crate-visible path exists solely for the strict B1 decoder. It rechecks every
    /// collection bound and authority-time binding, then recomputes the semantic digest rather
    /// than trusting bytes supplied by the caller.
    #[allow(
        clippy::too_many_arguments,
        reason = "every decoded authority field remains explicit and digest-bound"
    )]
    pub(crate) fn from_canonical_parts(
        request_id: ApprovalRequestId,
        action_id: ActionId,
        action_digest: crate::ActionDigest,
        requester: ActorId,
        requester_role: ActorRole,
        scope: CapabilityScope,
        requirement: ApprovalRequirement,
        evaluated_at: AuthorityInstant,
        challenge_epoch: Generation,
        challenge_tick_millis: u64,
        authority_time: AuthorityTimeState,
        risks: RiskSet,
        risk_details_digest: Sha256Digest,
        producing_participants: ParticipantSet,
        review_participants: ParticipantSet,
        validity: ValidityWindow,
    ) -> Result<Self, crate::ApprovalError> {
        if scope.permissions().as_slice().len() > MAX_APPROVAL_PERMISSIONS {
            return Err(crate::ApprovalError::CollectionTooLarge(
                crate::CanonicalCollection::Permissions,
            ));
        }
        if risks.as_slice().len() > MAX_RISK_CLASSES {
            return Err(crate::ApprovalError::CollectionTooLarge(
                crate::CanonicalCollection::Risks,
            ));
        }
        if requirement.independence().as_slice().len() > MAX_INDEPENDENCE_REQUIREMENTS {
            return Err(crate::ApprovalError::CollectionTooLarge(
                crate::CanonicalCollection::IndependenceRequirements,
            ));
        }
        if producing_participants.as_slice().len() > MAX_PRODUCING_PARTICIPANTS {
            return Err(crate::ApprovalError::CollectionTooLarge(
                crate::CanonicalCollection::ProducingParticipants,
            ));
        }
        if review_participants.as_slice().len() > MAX_REVIEW_PARTICIPANTS {
            return Err(crate::ApprovalError::CollectionTooLarge(
                crate::CanonicalCollection::ReviewParticipants,
            ));
        }
        if challenge_epoch != authority_time.epoch()
            || challenge_tick_millis != authority_time.greatest_tick_millis()
        {
            return Err(crate::ApprovalError::InvalidCanonicalEncoding);
        }
        let placeholder = crate::ApprovalRequestDigest::from_sha256(Sha256Digest::new([0; 32]));
        let mut request = Self {
            request_id,
            action_id,
            action_digest,
            requester,
            requester_role,
            scope,
            requirement,
            evaluated_at,
            challenge_epoch,
            challenge_tick_millis,
            authority_time,
            risks,
            risk_details_digest,
            producing_participants,
            review_participants,
            validity,
            digest: placeholder,
        };
        request.digest = crate::ApprovalRequestDigest::compute(&request)?;
        Ok(request)
    }

    /// Validates and constructs one bounded digest-bound request.
    ///
    /// # Errors
    ///
    /// Returns an exact canonical/bound failure or digest-preimage failure.
    #[allow(
        clippy::too_many_arguments,
        reason = "every authority field is explicit and digest-bound"
    )]
    pub fn new(
        request_id: ApprovalRequestId,
        action_id: ActionId,
        action_digest: crate::ActionDigest,
        requester: ActorId,
        requester_role: ActorRole,
        challenge: EscalationChallenge,
        risk_details_digest: Sha256Digest,
        producing_participants: ParticipantSet,
        review_participants: ParticipantSet,
        validity: ValidityWindow,
    ) -> Result<Self, crate::ApprovalError> {
        let placeholder = crate::ApprovalRequestDigest::from_sha256(Sha256Digest::new([0; 32]));
        let mut request = Self::new_with_digest(
            request_id,
            action_id,
            action_digest,
            requester,
            requester_role,
            challenge,
            risk_details_digest,
            producing_participants,
            review_participants,
            validity,
            placeholder,
        )?;
        request.digest = crate::ApprovalRequestDigest::compute(&request)?;
        Ok(request)
    }
}

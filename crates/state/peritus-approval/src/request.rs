//! Canonical bounded approval requests and participant facts.

use peritus_policy::{
    ActorRole, ApprovalRequirement, AuthorityInstant, AuthorityTimeState, CapabilityScope,
    EscalationChallenge, RiskSet, ValidityWindow,
};
use peritus_types::{ActionId, ActorId, ApprovalRequestId, Generation, Sha256Digest};

mod participant;
mod projection;
mod specification;

#[doc = " Maximum exact permissions bound into one request."]
pub const MAX_APPROVAL_PERMISSIONS: usize = 256;
#[doc = " Maximum structured risk variants bound into one request."]
pub const MAX_RISK_CLASSES: usize = 10;
#[doc = " Maximum exact independence requirements bound into one request."]
pub const MAX_INDEPENDENCE_REQUIREMENTS: usize = 4;
#[doc = " Maximum producing-attempt participants bound into one request."]
pub const MAX_PRODUCING_PARTICIPANTS: usize = 256;
#[doc = " Maximum review participants bound into one request."]
pub const MAX_REVIEW_PARTICIPANTS: usize = 256;
#[doc = " Maximum canonical request digest preimage size."]
pub const MAX_APPROVAL_REQUEST_PREIMAGE_BYTES: usize = 65_536;
#[doc = " Canonical duplicate-free actor participants for one provenance category."]
#[derive(Debug, Eq, PartialEq)]
pub struct ParticipantSet {
    pub(crate) values: Vec<ActorId>,
}
#[doc = " One exact bounded approval request with its move-only authority-time floor."]
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
    #[doc = " Returns the request identity."]
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId {
        self.request_id
    }
    #[doc = " Returns the action identity."]
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    #[doc = " Returns the exact externally refined action digest."]
    #[must_use]
    pub const fn action_digest(&self) -> crate::ActionDigest {
        self.action_digest
    }
    #[doc = " Returns the requesting actor."]
    #[must_use]
    pub const fn requester(&self) -> ActorId {
        self.requester
    }
    #[doc = " Returns the requesting actor\'s policy role."]
    #[must_use]
    pub const fn requester_role(&self) -> ActorRole {
        self.requester_role
    }
    #[doc = " Borrows the complete challenged capability scope."]
    #[must_use]
    pub const fn scope(&self) -> &CapabilityScope {
        &self.scope
    }
    #[doc = " Borrows the complete conjoined approval requirement."]
    #[must_use]
    pub const fn requirement(&self) -> &ApprovalRequirement {
        &self.requirement
    }
    #[doc = " Returns the policy evaluation instant."]
    #[must_use]
    pub const fn evaluated_at(&self) -> AuthorityInstant {
        self.evaluated_at
    }
    #[doc = " Borrows the current move-only authority-time floor."]
    #[must_use]
    pub const fn authority_time(&self) -> &AuthorityTimeState {
        &self.authority_time
    }
    #[doc = " Borrows canonical structured risk classes."]
    #[must_use]
    pub const fn risks(&self) -> &RiskSet {
        &self.risks
    }
    #[doc = " Returns the redacted structured-risk details digest."]
    #[must_use]
    pub const fn risk_details_digest(&self) -> Sha256Digest {
        self.risk_details_digest
    }
    #[doc = " Borrows producing-attempt participant claims."]
    #[must_use]
    pub const fn producing_participants(&self) -> &ParticipantSet {
        &self.producing_participants
    }
    #[doc = " Borrows review participant claims."]
    #[must_use]
    pub const fn review_participants(&self) -> &ParticipantSet {
        &self.review_participants
    }
    #[doc = " Returns the request\'s own validity window."]
    #[must_use]
    pub const fn validity(&self) -> ValidityWindow {
        self.validity
    }
    #[doc = " Returns the exact semantic request digest."]
    #[must_use]
    pub const fn digest(&self) -> crate::ApprovalRequestDigest {
        self.digest
    }
    pub(crate) const fn validate_observation_time(
        &self,
        observed_at: AuthorityInstant,
    ) -> Result<(), crate::ApprovalError> {
        if observed_at.epoch().get() != self.authority_time.epoch().get() {
            Err(crate::ApprovalError::ClockEpochMismatch)
        } else if observed_at.tick_millis() < self.authority_time.greatest_tick_millis() {
            Err(crate::ApprovalError::ClockRegression)
        } else {
            Ok(())
        }
    }
}
impl ApprovalRequest {
    #[allow(
        clippy::too_many_arguments,
        reason = "every authority field is explicit and digest-bound"
    )]
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

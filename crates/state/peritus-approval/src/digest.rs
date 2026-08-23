//! Domain-separated SHA-256 approval digests and canonical semantic encoders.

use peritus_policy::{
    ActorRole, AuthorityInstant, AuthorityTier, IndependenceRequirement, PolicyTier, RiskClass,
    ValidityWindow,
};
use peritus_types::{RevisionTuple, Sha256Digest};
use sha2::{Digest, Sha256};
use vstd::prelude::*;

mod canonical;
mod registry;
#[cfg(test)]
mod tests;
pub use canonical::CanonicalEncoder;
use canonical::{be_u32, be_u64};
pub use registry::{credential_registry_bytes, credential_registry_digest};

verus! {

/// Maximum canonical decision digest preimage size.
pub const MAX_APPROVAL_DECISION_PREIMAGE_BYTES: usize = 4_096;
/// Maximum canonical approval-key identifier preimage size.
pub const MAX_APPROVAL_KEY_ID_PREIMAGE_BYTES: usize = 256;
/// Maximum canonical credential-registry snapshot preimage size.
pub const MAX_CREDENTIAL_REGISTRY_PREIMAGE_BYTES: usize = 4 * 1024 * 1024;

/// Exact externally computed SHA-256 digest of canonical action bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActionDigest(Sha256Digest);

impl ActionDigest {
    /// Returns the exact action-digest bytes used by reducer specifications.
    pub closed spec fn spec_bytes(&self) -> [u8; 32] { self.0.spec_bytes() }

    /// Stores an externally computed digest without claiming byte provenance.
    #[must_use]
    pub const fn from_sha256(digest: Sha256Digest) -> Self { Self(digest) }

    /// Returns the exact SHA-256 value.
    #[must_use]
    pub const fn sha256(self) -> (digest: Sha256Digest)
        ensures digest.spec_bytes() == self.spec_bytes(),
    { self.0 }
}

/// SHA-256 binding every authority-relevant request field.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ApprovalRequestDigest(Sha256Digest);

impl ApprovalRequestDigest {
    /// Returns the exact request-digest bytes used by reducer specifications.
    pub closed spec fn spec_bytes(&self) -> [u8; 32] { self.0.spec_bytes() }

    pub(crate) const fn from_sha256(digest: Sha256Digest) -> Self { Self(digest) }

    /// Returns the exact SHA-256 value.
    #[must_use]
    pub const fn sha256(self) -> (digest: Sha256Digest)
        ensures digest.spec_bytes() == self.spec_bytes(),
    { self.0 }
}

/// SHA-256 binding every signed decision field.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ApprovalDecisionDigest(Sha256Digest);

impl ApprovalDecisionDigest {
    /// Returns the exact decision-digest bytes used by reducer specifications.
    pub closed spec fn spec_bytes(&self) -> [u8; 32] { self.0.spec_bytes() }

    pub(crate) const fn from_sha256(digest: Sha256Digest) -> Self { Self(digest) }

    /// Returns the exact SHA-256 value.
    #[must_use]
    pub const fn sha256(self) -> (digest: Sha256Digest)
        ensures digest.spec_bytes() == self.spec_bytes(),
    { self.0 }
}

const fn enum_byte(value: u8) -> [u8; 1] { [value] }

pub const fn role_tag(role: ActorRole) -> (tag: u8)
    ensures tag as int == role.spec_rank(),
{
    match role {
        ActorRole::Writer => 0,
        ActorRole::Fixer => 1,
        ActorRole::Reviewer => 2,
        ActorRole::Evaluator => 3,
        ActorRole::GateRunner => 4,
        ActorRole::Orchestrator => 5,
        ActorRole::EvolutionAgent => 6,
        ActorRole::HumanAuthority => 7,
        ActorRole::DaemonService => 8,
        ActorRole::ProviderToolWorker => 9,
        ActorRole::Plugin => 10,
    }
}

const fn risk_tag(risk: RiskClass) -> u8 {
    match risk {
        RiskClass::Read => 0,
        RiskClass::ScopedWrite => 1,
        RiskClass::Execution => 2,
        RiskClass::Network => 3,
        RiskClass::DependencyEnvironment => 4,
        RiskClass::RepositoryHistoryMutation => 5,
        RiskClass::SecretUse => 6,
        RiskClass::ExternalSideEffect => 7,
        RiskClass::PolicyAuthority => 8,
        RiskClass::HarnessPromotion => 9,
    }
}

const fn independence_tag(value: IndependenceRequirement) -> u8 {
    match value {
        IndependenceRequirement::NotRequester => 0,
        IndependenceRequirement::NotActionActor => 1,
        IndependenceRequirement::NoProducingAttemptParticipation => 2,
        IndependenceRequirement::NoReviewParticipation => 3,
    }
}

const fn authority_tier_tag(value: AuthorityTier) -> u8 {
    match value {
        AuthorityTier::Project => 0,
        AuthorityTier::User => 1,
        AuthorityTier::Organization => 2,
        AuthorityTier::System => 3,
    }
}

const fn policy_tier_tag(value: PolicyTier) -> u8 {
    match value {
        PolicyTier::System => 0,
        PolicyTier::User => 1,
        PolicyTier::Project => 2,
        PolicyTier::Run => 3,
        PolicyTier::Session => 4,
        PolicyTier::RoleHarness => 5,
    }
}

fn instant_bytes(value: AuthorityInstant) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&be_u64(value.epoch().get()));
    bytes.extend_from_slice(&be_u64(value.tick_millis()));
    bytes
}

fn validity_bytes(value: ValidityWindow) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&instant_bytes(value.not_before()));
    bytes.extend_from_slice(&instant_bytes(value.expires_at()));
    bytes
}

fn revision_bytes(value: RevisionTuple) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(value.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(value.harness_id().as_bytes());
    bytes.extend_from_slice(value.workspace_id().as_bytes());
    bytes.extend_from_slice(&be_u64(value.workspace_generation().get()));
    bytes.extend_from_slice(&be_u64(value.workspace_revision().get()));
    bytes.extend_from_slice(value.policy_id().as_bytes());
    bytes.extend_from_slice(value.provider_profile_id().as_bytes());
    bytes
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "each u32 conversion is dominated by its explicit u32::MAX length guard"
)]
fn list_vec_bytes(values: &[Vec<u8>]) -> Result<Vec<u8>, crate::ApprovalError> {
    if values.len() > u32::MAX as usize {
        return Err(crate::ApprovalError::PreimageTooLarge);
    }
    let count = values.len() as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&be_u32(count));
    let mut index = 0;
    while index < values.len()
        invariant 0 <= index <= values.len(),
        decreases values.len() - index,
    {
        if values[index].len() > u32::MAX as usize {
            return Err(crate::ApprovalError::PreimageTooLarge);
        }
        let length = values[index].len() as u32;
        bytes.extend_from_slice(&be_u32(length));
        bytes.extend_from_slice(&values[index]);
        index += 1;
    }
    Ok(bytes)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the u32 conversion is dominated by the explicit u32::MAX length guard"
)]
fn list_tag_bytes(values: &[[u8; 1]]) -> Result<Vec<u8>, crate::ApprovalError> {
    if values.len() > u32::MAX as usize {
        return Err(crate::ApprovalError::PreimageTooLarge);
    }
    let count = values.len() as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&be_u32(count));
    let mut index = 0;
    while index < values.len()
        invariant 0 <= index <= values.len(),
        decreases values.len() - index,
    {
        bytes.extend_from_slice(&be_u32(1));
        bytes.extend_from_slice(&values[index]);
        index += 1;
    }
    Ok(bytes)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the u32 conversion is dominated by the explicit u32::MAX length guard"
)]
fn list_actor_bytes(values: &[peritus_types::ActorId]) -> Result<Vec<u8>, crate::ApprovalError> {
    if values.len() > u32::MAX as usize {
        return Err(crate::ApprovalError::PreimageTooLarge);
    }
    let count = values.len() as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&be_u32(count));
    let mut index = 0;
    while index < values.len()
        invariant 0 <= index <= values.len(),
        decreases values.len() - index,
    {
        bytes.extend_from_slice(&be_u32(16));
        bytes.extend_from_slice(values[index].as_bytes());
        index += 1;
    }
    Ok(bytes)
}

} // verus!

impl ApprovalRequestDigest {
    /// Computes the domain-separated digest of every exact request field.
    ///
    /// # Errors
    ///
    /// Returns `PreimageTooLarge` rather than hashing a truncated value.
    pub fn compute(request: &crate::ApprovalRequest) -> Result<Self, crate::ApprovalError> {
        let mut encoder = CanonicalEncoder::record(
            b"approval-request-digest",
            crate::MAX_APPROVAL_REQUEST_PREIMAGE_BYTES,
        )?;
        encoder.field(1, request.request_id.as_bytes())?;
        encoder.field(2, request.action_id.as_bytes())?;
        encoder.field(3, request.action_digest.sha256().as_bytes())?;
        encoder.field(4, request.requester.as_bytes())?;
        encoder.field(5, &enum_byte(role_tag(request.requester_role)))?;
        encoder.field(6, request.scope.actor_id().as_bytes())?;
        encoder.field(7, &enum_byte(role_tag(request.scope.role())))?;
        encoder.field(8, request.scope.environment_id().as_bytes())?;
        let mut permissions = Vec::new();
        let permission_values = request.scope.permissions().as_slice();
        let mut permission_index = 0;
        while permission_index < permission_values.len() {
            let permission = &permission_values[permission_index];
            let mut item = Vec::new();
            item.extend_from_slice(permission.resource_id().as_bytes());
            let name = permission.capability_name().as_str().as_bytes();
            if name.len() > u32::MAX as usize {
                return Err(crate::ApprovalError::PreimageTooLarge);
            }
            let name_length =
                u32::try_from(name.len()).map_err(|_| crate::ApprovalError::PreimageTooLarge)?;
            item.extend_from_slice(&be_u32(name_length));
            item.extend_from_slice(name);
            permissions.push(item);
            permission_index += 1;
        }
        encoder.field(9, &list_vec_bytes(permissions.as_slice())?)?;
        encoder.field(10, &revision_bytes(request.scope.revision()))?;
        encoder.field(11, &validity_bytes(request.scope.validity()))?;
        match request.scope.use_limit().remaining() {
            None => encoder.field(12, &[0])?,
            Some(value) => {
                let mut bytes = [0_u8; 9];
                bytes[0] = 1;
                bytes[1..].copy_from_slice(&be_u64(value));
                encoder.field(12, &bytes)?;
            }
        }
        encoder.field(13, &enum_byte(authority_tier_tag(request.requirement.minimum_tier())))?;
        let roles: Vec<[u8; 1]> = request
            .requirement
            .approver_roles()
            .iter()
            .map(|role| enum_byte(role_tag(*role)))
            .collect();
        encoder.field(14, &list_tag_bytes(roles.as_slice())?)?;
        let independence: Vec<[u8; 1]> = request
            .requirement
            .independence()
            .as_slice()
            .iter()
            .map(|value| enum_byte(independence_tag(*value)))
            .collect();
        encoder.field(15, &list_tag_bytes(independence.as_slice())?)?;
        encoder.field(16, &validity_bytes(request.requirement.validity()))?;
        encoder.field(17, &instant_bytes(request.evaluated_at))?;
        let mut floor = [0_u8; 16];
        floor[..8].copy_from_slice(&be_u64(request.challenge_epoch.get()));
        floor[8..].copy_from_slice(&be_u64(request.challenge_tick_millis));
        encoder.field(18, &floor)?;
        let risks: Vec<[u8; 1]> =
            request.risks.as_slice().iter().map(|risk| enum_byte(risk_tag(*risk))).collect();
        encoder.field(19, &list_tag_bytes(risks.as_slice())?)?;
        encoder.field(20, request.risk_details_digest.as_bytes())?;
        encoder.field(21, &list_actor_bytes(request.producing_participants.as_slice())?)?;
        encoder.field(22, &list_actor_bytes(request.review_participants.as_slice())?)?;
        encoder.field(23, &validity_bytes(request.validity))?;
        let mut hasher = Sha256::new();
        hasher.update(encoder.finish());
        Ok(Self::from_sha256(Sha256Digest::new(hasher.finalize().into())))
    }
}

impl ApprovalDecisionDigest {
    /// Computes the domain-separated digest of every signed decision field.
    ///
    /// # Errors
    ///
    /// Returns `PreimageTooLarge` rather than hashing a truncated value.
    pub fn compute(decision: &crate::ApprovalDecision) -> Result<Self, crate::ApprovalError> {
        let mut encoder = CanonicalEncoder::record(
            b"approval-decision-digest",
            MAX_APPROVAL_DECISION_PREIMAGE_BYTES,
        )?;
        encoder.field(1, decision.command_id.as_bytes())?;
        encoder.field(2, decision.responder.as_bytes())?;
        encoder.field(3, &enum_byte(role_tag(decision.approver_role)))?;
        encoder.field(4, decision.request_id.as_bytes())?;
        encoder.field(5, decision.request_digest.sha256().as_bytes())?;
        match decision.choice {
            crate::ApprovalChoice::Deny => encoder.field(6, &[0])?,
            crate::ApprovalChoice::ApproveOnce => encoder.field(6, &[1])?,
            crate::ApprovalChoice::Amend(identity) => {
                encoder.field(6, &[2])?;
                encoder.field(7, identity.base_policy_id().as_bytes())?;
                encoder.field(8, identity.successor_policy_id().as_bytes())?;
                encoder.field(9, &enum_byte(policy_tier_tag(identity.tier())))?;
                encoder.field(10, identity.amendment_digest().as_bytes())?;
            }
        }
        encoder.field(11, &instant_bytes(decision.expires_at))?;
        encoder.field(12, decision.key_id.sha256().as_bytes())?;
        encoder.field(13, &be_u64(decision.credential_generation.get()))?;
        encoder.field(14, &be_u64(decision.registry_revision.get()))?;
        let mut hasher = Sha256::new();
        hasher.update(encoder.finish());
        Ok(Self::from_sha256(Sha256Digest::new(hasher.finalize().into())))
    }
}

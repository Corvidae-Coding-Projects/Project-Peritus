//! Shared exact value encoders and checked decoders for approval codec records.

use peritus_policy::{
    ActorRole, AuthorityInstant, AuthorityTier, IndependenceRequirement, IndependenceSet,
    Permission, PermissionSet, PolicyTier, RiskClass, RiskSet, UseLimit, ValidityWindow,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, Generation, HarnessId, PolicyId, ProviderProfileId, ResourceId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

use super::reader::{ListReader, encode_list, invalid};
use crate::{
    ApprovalError, MAX_APPROVAL_PERMISSIONS, MAX_INDEPENDENCE_REQUIREMENTS,
    MAX_PRODUCING_PARTICIPANTS, MAX_REVIEW_PARTICIPANTS, MAX_RISK_CLASSES, ParticipantSet,
};

const MAX_ACTOR_ROLES: usize = 11;

pub(super) fn exact<const N: usize>(bytes: &[u8]) -> Result<[u8; N], ApprovalError> {
    bytes.try_into().map_err(|_| invalid())
}

pub(super) fn decode_u64(bytes: &[u8]) -> Result<u64, ApprovalError> {
    Ok(u64::from_be_bytes(exact(bytes)?))
}

pub(super) fn decode_generation(bytes: &[u8]) -> Result<Generation, ApprovalError> {
    Generation::new(decode_u64(bytes)?).map_err(|_| invalid())
}

pub(super) fn decode_revision_number(bytes: &[u8]) -> Result<RevisionNumber, ApprovalError> {
    RevisionNumber::new(decode_u64(bytes)?).map_err(|_| invalid())
}

pub(super) const fn role_tag(role: ActorRole) -> u8 {
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

pub(super) fn decode_role(bytes: &[u8]) -> Result<ActorRole, ApprovalError> {
    match bytes {
        [0] => Ok(ActorRole::Writer),
        [1] => Ok(ActorRole::Fixer),
        [2] => Ok(ActorRole::Reviewer),
        [3] => Ok(ActorRole::Evaluator),
        [4] => Ok(ActorRole::GateRunner),
        [5] => Ok(ActorRole::Orchestrator),
        [6] => Ok(ActorRole::EvolutionAgent),
        [7] => Ok(ActorRole::HumanAuthority),
        [8] => Ok(ActorRole::DaemonService),
        [9] => Ok(ActorRole::ProviderToolWorker),
        [10] => Ok(ActorRole::Plugin),
        _ => Err(invalid()),
    }
}

pub(super) const fn authority_tier_tag(tier: AuthorityTier) -> u8 {
    match tier {
        AuthorityTier::Project => 0,
        AuthorityTier::User => 1,
        AuthorityTier::Organization => 2,
        AuthorityTier::System => 3,
    }
}

pub(super) fn decode_authority_tier(bytes: &[u8]) -> Result<AuthorityTier, ApprovalError> {
    match bytes {
        [0] => Ok(AuthorityTier::Project),
        [1] => Ok(AuthorityTier::User),
        [2] => Ok(AuthorityTier::Organization),
        [3] => Ok(AuthorityTier::System),
        _ => Err(invalid()),
    }
}

pub(super) const fn policy_tier_tag(tier: PolicyTier) -> u8 {
    match tier {
        PolicyTier::System => 0,
        PolicyTier::User => 1,
        PolicyTier::Project => 2,
        PolicyTier::Run => 3,
        PolicyTier::Session => 4,
        PolicyTier::RoleHarness => 5,
    }
}

pub(super) fn decode_policy_tier(bytes: &[u8]) -> Result<PolicyTier, ApprovalError> {
    match bytes {
        [0] => Ok(PolicyTier::System),
        [1] => Ok(PolicyTier::User),
        [2] => Ok(PolicyTier::Project),
        [3] => Ok(PolicyTier::Run),
        [4] => Ok(PolicyTier::Session),
        [5] => Ok(PolicyTier::RoleHarness),
        _ => Err(invalid()),
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

fn decode_independence(bytes: &[u8]) -> Result<IndependenceRequirement, ApprovalError> {
    match bytes {
        [0] => Ok(IndependenceRequirement::NotRequester),
        [1] => Ok(IndependenceRequirement::NotActionActor),
        [2] => Ok(IndependenceRequirement::NoProducingAttemptParticipation),
        [3] => Ok(IndependenceRequirement::NoReviewParticipation),
        _ => Err(invalid()),
    }
}

const fn risk_tag(value: RiskClass) -> u8 {
    match value {
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

fn decode_risk(bytes: &[u8]) -> Result<RiskClass, ApprovalError> {
    match bytes {
        [0] => Ok(RiskClass::Read),
        [1] => Ok(RiskClass::ScopedWrite),
        [2] => Ok(RiskClass::Execution),
        [3] => Ok(RiskClass::Network),
        [4] => Ok(RiskClass::DependencyEnvironment),
        [5] => Ok(RiskClass::RepositoryHistoryMutation),
        [6] => Ok(RiskClass::SecretUse),
        [7] => Ok(RiskClass::ExternalSideEffect),
        [8] => Ok(RiskClass::PolicyAuthority),
        [9] => Ok(RiskClass::HarnessPromotion),
        _ => Err(invalid()),
    }
}

pub(super) fn instant_bytes(value: AuthorityInstant) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(16);
    bytes.extend_from_slice(&value.epoch().get().to_be_bytes());
    bytes.extend_from_slice(&value.tick_millis().to_be_bytes());
    bytes
}

pub(super) fn decode_instant(bytes: &[u8]) -> Result<AuthorityInstant, ApprovalError> {
    if bytes.len() != 16 {
        return Err(invalid());
    }
    let epoch = Generation::new(u64::from_be_bytes(exact(&bytes[..8])?)).map_err(|_| invalid())?;
    let tick_millis = u64::from_be_bytes(exact(&bytes[8..])?);
    Ok(AuthorityInstant::new(epoch, tick_millis))
}

pub(super) fn validity_bytes(value: ValidityWindow) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(&instant_bytes(value.not_before()));
    bytes.extend_from_slice(&instant_bytes(value.expires_at()));
    bytes
}

pub(super) fn decode_validity(bytes: &[u8]) -> Result<ValidityWindow, ApprovalError> {
    if bytes.len() != 32 {
        return Err(invalid());
    }
    ValidityWindow::new(decode_instant(&bytes[..16])?, decode_instant(&bytes[16..])?)
        .map_err(|_| invalid())
}

pub(super) fn revision_bytes(value: RevisionTuple) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(96);
    bytes.extend_from_slice(value.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(value.harness_id().as_bytes());
    bytes.extend_from_slice(value.workspace_id().as_bytes());
    bytes.extend_from_slice(&value.workspace_generation().get().to_be_bytes());
    bytes.extend_from_slice(&value.workspace_revision().get().to_be_bytes());
    bytes.extend_from_slice(value.policy_id().as_bytes());
    bytes.extend_from_slice(value.provider_profile_id().as_bytes());
    bytes
}

pub(super) fn decode_revision(bytes: &[u8]) -> Result<RevisionTuple, ApprovalError> {
    if bytes.len() != 96 {
        return Err(invalid());
    }
    let acceptance_spec_id = AcceptanceSpecId::new(exact(&bytes[..16])?).map_err(|_| invalid())?;
    let harness_id = HarnessId::new(exact(&bytes[16..32])?).map_err(|_| invalid())?;
    let workspace_id = WorkspaceId::new(exact(&bytes[32..48])?).map_err(|_| invalid())?;
    let workspace_generation =
        Generation::new(u64::from_be_bytes(exact(&bytes[48..56])?)).map_err(|_| invalid())?;
    let workspace_revision =
        RevisionNumber::new(u64::from_be_bytes(exact(&bytes[56..64])?)).map_err(|_| invalid())?;
    let policy_id = PolicyId::new(exact(&bytes[64..80])?).map_err(|_| invalid())?;
    let provider_profile_id =
        ProviderProfileId::new(exact(&bytes[80..96])?).map_err(|_| invalid())?;
    Ok(RevisionTuple::new(
        acceptance_spec_id,
        harness_id,
        workspace_id,
        workspace_generation,
        workspace_revision,
        policy_id,
        provider_profile_id,
    ))
}

pub(super) fn use_limit_bytes(value: UseLimit) -> Vec<u8> {
    value.remaining().map_or_else(
        || vec![0],
        |remaining| {
            let mut bytes = Vec::with_capacity(9);
            bytes.push(1);
            bytes.extend_from_slice(&remaining.to_be_bytes());
            bytes
        },
    )
}

pub(super) fn decode_use_limit(bytes: &[u8]) -> Result<UseLimit, ApprovalError> {
    match bytes {
        [0] => Ok(UseLimit::unlimited()),
        [1, remaining @ ..] if remaining.len() == 8 => {
            UseLimit::limited(u64::from_be_bytes(exact(remaining)?)).map_err(|_| invalid())
        }
        _ => Err(invalid()),
    }
}

pub(super) fn encode_permissions(values: &[Permission]) -> Result<Vec<u8>, ApprovalError> {
    encode_list(values, |permission| {
        let name = permission.capability_name().as_str().as_bytes();
        let name_length = u32::try_from(name.len()).map_err(|_| ApprovalError::PreimageTooLarge)?;
        let mut item = Vec::with_capacity(20 + name.len());
        item.extend_from_slice(permission.resource_id().as_bytes());
        item.extend_from_slice(&name_length.to_be_bytes());
        item.extend_from_slice(name);
        Ok(item)
    })
}

pub(super) fn decode_permissions(bytes: &[u8]) -> Result<PermissionSet, ApprovalError> {
    let mut list = ListReader::new(bytes, MAX_APPROVAL_PERMISSIONS)?;
    let mut values = Vec::with_capacity(list.len());
    while list.len() != 0 {
        let item = list.item()?;
        if item.len() < 20 {
            return Err(invalid());
        }
        let resource_id = ResourceId::new(exact(&item[..16])?).map_err(|_| invalid())?;
        let name_length =
            usize::try_from(u32::from_be_bytes(exact(&item[16..20])?)).map_err(|_| invalid())?;
        if item.len().checked_sub(20) != Some(name_length) {
            return Err(invalid());
        }
        let name = core::str::from_utf8(&item[20..]).map_err(|_| invalid())?;
        let capability_name =
            peritus_types::CapabilityName::new(name.to_owned()).map_err(|_| invalid())?;
        values.push(Permission::new(resource_id, capability_name));
    }
    list.finish()?;
    PermissionSet::new(values).map_err(|_| invalid())
}

pub(super) fn encode_roles(values: &[ActorRole]) -> Result<Vec<u8>, ApprovalError> {
    encode_list(values, |role| Ok(vec![role_tag(*role)]))
}

pub(super) fn decode_roles(bytes: &[u8], maximum: usize) -> Result<Vec<ActorRole>, ApprovalError> {
    if maximum > MAX_ACTOR_ROLES {
        return Err(invalid());
    }
    let mut list = ListReader::new(bytes, maximum)?;
    let mut values = Vec::with_capacity(list.len());
    while list.len() != 0 {
        values.push(decode_role(list.item()?)?);
    }
    list.finish()?;
    Ok(values)
}

pub(super) fn encode_independence(
    values: &[IndependenceRequirement],
) -> Result<Vec<u8>, ApprovalError> {
    encode_list(values, |value| Ok(vec![independence_tag(*value)]))
}

pub(super) fn decode_independence_set(bytes: &[u8]) -> Result<IndependenceSet, ApprovalError> {
    let mut list = ListReader::new(bytes, MAX_INDEPENDENCE_REQUIREMENTS)?;
    let mut values = Vec::with_capacity(list.len());
    while list.len() != 0 {
        values.push(decode_independence(list.item()?)?);
    }
    list.finish()?;
    IndependenceSet::new(values).map_err(|_| invalid())
}

pub(super) fn encode_risks(values: &[RiskClass]) -> Result<Vec<u8>, ApprovalError> {
    encode_list(values, |risk| Ok(vec![risk_tag(*risk)]))
}

pub(super) fn decode_risks(bytes: &[u8]) -> Result<RiskSet, ApprovalError> {
    let mut list = ListReader::new(bytes, MAX_RISK_CLASSES)?;
    let mut values = Vec::with_capacity(list.len());
    while list.len() != 0 {
        values.push(decode_risk(list.item()?)?);
    }
    list.finish()?;
    RiskSet::new(values).map_err(|_| invalid())
}

pub(super) fn encode_participants(values: &[ActorId]) -> Result<Vec<u8>, ApprovalError> {
    encode_list(values, |actor| Ok(actor.as_bytes().to_vec()))
}

fn decode_actor_values(bytes: &[u8], maximum: usize) -> Result<Vec<ActorId>, ApprovalError> {
    let mut list = ListReader::new(bytes, maximum)?;
    let mut values = Vec::with_capacity(list.len());
    while list.len() != 0 {
        values.push(ActorId::new(exact(list.item()?)?).map_err(|_| invalid())?);
    }
    list.finish()?;
    Ok(values)
}

pub(super) fn decode_producing_participants(bytes: &[u8]) -> Result<ParticipantSet, ApprovalError> {
    ParticipantSet::producing(decode_actor_values(bytes, MAX_PRODUCING_PARTICIPANTS)?)
        .map_err(|_| invalid())
}

pub(super) fn decode_review_participants(bytes: &[u8]) -> Result<ParticipantSet, ApprovalError> {
    ParticipantSet::review(decode_actor_values(bytes, MAX_REVIEW_PARTICIPANTS)?)
        .map_err(|_| invalid())
}

pub(super) fn decode_sha256(bytes: &[u8]) -> Result<Sha256Digest, ApprovalError> {
    Ok(Sha256Digest::new(exact(bytes)?))
}

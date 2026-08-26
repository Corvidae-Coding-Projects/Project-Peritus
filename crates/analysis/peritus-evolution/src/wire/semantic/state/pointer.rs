//! Complete canonical production-pointer checkpoint semantics.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};
use peritus_harness::domain::HarnessRevisionIdentity;

use crate::{
    ActivationId, ActivationKind, ActivationRecord, EvolutionCampaignId, EvolutionError,
    PendingActivation, PointerPhase, ProductionHarnessState,
};

use super::super::{super::scalar, binding, proposal};
use super::shared::read_vec;

pub(crate) fn encode_pointer_state(
    state: &ProductionHarnessState,
) -> Result<Vec<u8>, EvolutionError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_fixed(state.project_id().as_bytes()).map_err(scalar::codec)?;
    binding::write_production(&mut writer, state.current())?;
    binding::write_limits(&mut writer, state.limits())?;
    binding::write_policy(&mut writer, state.policy())?;
    writer.write_u64(state.generation()).map_err(scalar::codec)?;
    writer.write_u64(state.sequence()).map_err(scalar::codec)?;
    writer.write_fixed(state.last_event().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(state.state_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_u8(state.phase().tag()).map_err(scalar::codec)?;
    writer.write_collection_len(state.history().len()).map_err(scalar::codec)?;
    for value in state.history() {
        write_activation(&mut writer, value)?;
    }
    match state.pending() {
        None => writer.write_u8(0).map_err(scalar::codec)?,
        Some(PendingActivation::Promotion(value)) => {
            writer.write_u8(1).map_err(scalar::codec)?;
            proposal::write_promotion(&mut writer, value)?;
        }
        Some(PendingActivation::Rollback(value)) => {
            writer.write_u8(2).map_err(scalar::codec)?;
            proposal::write_rollback(&mut writer, value)?;
        }
    }
    Ok(writer.into_bytes())
}

pub(crate) fn decode_pointer_state(bytes: &[u8]) -> Result<ProductionHarnessState, EvolutionError> {
    let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
    let project_id = scalar::project_id(&mut reader).map_err(scalar::codec)?;
    let current = binding::production(&mut reader)?;
    let limits = binding::limits(&mut reader)?;
    let policy = binding::policy(&mut reader, limits)?;
    let generation = reader.read_u64().map_err(scalar::codec)?;
    let sequence = reader.read_u64().map_err(scalar::codec)?;
    let last_event = scalar::event_id(&mut reader).map_err(scalar::codec)?;
    let encoded_state = scalar::digest(&mut reader)?;
    let phase = pointer_phase(reader.read_u8().map_err(scalar::codec)?)?;
    let history = read_vec(&mut reader, usize::from(limits.activation_history()), activation)?;
    let pending = match reader.read_u8().map_err(scalar::codec)? {
        0 => None,
        1 => Some(PendingActivation::Promotion(proposal::promotion(&mut reader)?)),
        2 => Some(PendingActivation::Rollback(proposal::rollback(&mut reader)?)),
        _ => return Err(scalar::protocol()),
    };
    reader.finish().map_err(scalar::codec)?;

    if generation == 0
        || sequence == 0
        || history.is_empty()
        || !policy_origin_matches(policy.production_revision(), &history)
        || !pending_matches(phase, pending.as_ref(), project_id, current, policy.digest())
        || history
            .last()
            .is_none_or(|value| value.generation() != generation || value.successor() != current)
        || history.windows(2).any(|pair| {
            pair[1].generation() != pair[0].generation().saturating_add(1)
                || pair[1].predecessor() != Some(pair[0].successor())
        })
    {
        return Err(scalar::protocol());
    }

    let mut state = ProductionHarnessState {
        project_id,
        current,
        policy,
        limits,
        generation,
        sequence,
        last_event,
        state_digest: encoded_state,
        phase,
        history,
        pending,
    };
    state.refresh_digest();
    if state.state_digest() != encoded_state {
        return Err(scalar::protocol());
    }
    Ok(state)
}

fn policy_origin_matches(
    production_revision: HarnessRevisionIdentity,
    history: &[ActivationRecord],
) -> bool {
    let mut initializations = history
        .iter()
        .enumerate()
        .filter(|(_, value)| value.kind() == ActivationKind::Initialization);
    match (initializations.next(), initializations.next()) {
        (None, None) => true,
        (Some((0, value)), None) => {
            value.generation() == 1 && value.successor().harness_revision() == production_revision
        }
        _ => false,
    }
}

fn write_activation(
    writer: &mut CanonicalWriter,
    value: &ActivationRecord,
) -> Result<(), EvolutionError> {
    writer.write_u8(activation_kind_tag(value.kind())).map_err(scalar::codec)?;
    writer.write_u64(value.generation()).map_err(scalar::codec)?;
    writer.write_option_tag(value.predecessor().is_some()).map_err(scalar::codec)?;
    if let Some(predecessor) = value.predecessor() {
        binding::write_production(writer, predecessor)?;
    }
    binding::write_production(writer, value.successor())?;
    writer.write_option_tag(value.campaign_id().is_some()).map_err(scalar::codec)?;
    if let Some(campaign_id) = value.campaign_id() {
        writer.write_fixed(campaign_id.as_bytes()).map_err(scalar::codec)?;
    }
    writer.write_fixed(value.action_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_option_tag(value.authorization().is_some()).map_err(scalar::codec)?;
    if let Some(authorization) = value.authorization() {
        binding::write_authorization(writer, authorization)?;
    }
    writer.write_fixed(value.evidence_artifact().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(value.evidence_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_option_tag(value.rollback_of().is_some()).map_err(scalar::codec)?;
    if let Some(rollback_of) = value.rollback_of() {
        writer.write_fixed(rollback_of.as_bytes()).map_err(scalar::codec)?;
    }
    Ok(())
}

fn activation(reader: &mut CanonicalReader<'_>) -> Result<ActivationRecord, EvolutionError> {
    let kind = activation_kind(reader.read_u8().map_err(scalar::codec)?)?;
    let generation = reader.read_u64().map_err(scalar::codec)?;
    let predecessor = reader
        .read_option_tag()
        .map_err(scalar::codec)?
        .then(|| binding::production(reader))
        .transpose()?;
    let successor = binding::production(reader)?;
    let campaign_id = reader
        .read_option_tag()
        .map_err(scalar::codec)?
        .then(|| scalar::campaign_id(reader).map_err(scalar::codec))
        .transpose()?;
    let action_digest = scalar::digest(reader)?;
    let authorization = reader
        .read_option_tag()
        .map_err(scalar::codec)?
        .then(|| binding::authorization(reader))
        .transpose()?;
    let evidence_artifact = scalar::digest(reader)?;
    let evidence_digest = scalar::digest(reader)?;
    let rollback_of = reader
        .read_option_tag()
        .map_err(scalar::codec)?
        .then(|| ActivationId::new(reader.read_fixed().map_err(scalar::codec)?))
        .transpose()?;
    if generation == 0
        || !activation_shape(kind, predecessor, campaign_id, authorization, rollback_of)
    {
        return Err(scalar::protocol());
    }
    Ok(crate::pointer::activation_record(
        kind,
        generation,
        predecessor,
        successor,
        campaign_id,
        action_digest,
        authorization,
        evidence_artifact,
        evidence_digest,
        rollback_of,
    ))
}

const fn activation_shape(
    kind: ActivationKind,
    predecessor: Option<crate::ProductionHarnessBinding>,
    campaign_id: Option<EvolutionCampaignId>,
    authorization: Option<crate::ActivationAuthorization>,
    rollback_of: Option<ActivationId>,
) -> bool {
    matches!(
        (kind, predecessor, campaign_id, authorization, rollback_of),
        (ActivationKind::Initialization, None, None, None, None)
            | (ActivationKind::Promotion, Some(_), Some(_), Some(_), None)
            | (ActivationKind::Rollback, Some(_), None, Some(_), Some(_))
    )
}

fn pending_matches(
    phase: PointerPhase,
    pending: Option<&PendingActivation>,
    project_id: peritus_types::ProjectId,
    current: crate::ProductionHarnessBinding,
    policy_digest: peritus_types::Sha256Digest,
) -> bool {
    match (phase, pending) {
        (PointerPhase::Active, None) => true,
        (PointerPhase::PromotionPending, Some(PendingActivation::Promotion(value))) => {
            value.project_id() == project_id
                && value.current() == current
                && value.policy_digest() == policy_digest
        }
        (PointerPhase::RollbackPending, Some(PendingActivation::Rollback(value))) => {
            value.project_id() == project_id
                && value.current() == current
                && value.policy_digest() == policy_digest
        }
        _ => false,
    }
}

const fn pointer_phase(tag: u8) -> Result<PointerPhase, EvolutionError> {
    match tag {
        0 => Ok(PointerPhase::Active),
        1 => Ok(PointerPhase::PromotionPending),
        2 => Ok(PointerPhase::RollbackPending),
        _ => Err(scalar::protocol()),
    }
}

const fn activation_kind_tag(kind: ActivationKind) -> u8 {
    match kind {
        ActivationKind::Initialization => 0,
        ActivationKind::Promotion => 1,
        ActivationKind::Rollback => 2,
    }
}

const fn activation_kind(tag: u8) -> Result<ActivationKind, EvolutionError> {
    match tag {
        0 => Ok(ActivationKind::Initialization),
        1 => Ok(ActivationKind::Promotion),
        2 => Ok(ActivationKind::Rollback),
        _ => Err(scalar::protocol()),
    }
}

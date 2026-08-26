//! Complete canonical campaign checkpoint semantics.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};

use crate::{
    BaselineEvidence, CampaignPhase, CampaignState, CampaignTerminal, EvolutionError, PromotionId,
    VariantEvaluation, VariantId, identity::digest_parts,
};

use super::super::{super::scalar, attribution, binding, change, evaluation, proposal, selection};
use super::shared::{read_option, read_vec, write_option};

pub(crate) fn encode_campaign_state(state: &CampaignState) -> Result<Vec<u8>, EvolutionError> {
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_fixed(state.campaign_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(state.project_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(state.binding_digest().as_bytes()).map_err(scalar::codec)?;
    binding::write_production(&mut writer, state.baseline())?;
    binding::write_limits(&mut writer, state.limits())?;
    binding::write_policy(&mut writer, state.policy())?;
    writer.write_u64(state.sequence()).map_err(scalar::codec)?;
    writer.write_fixed(state.last_event().as_bytes()).map_err(scalar::codec)?;
    writer.write_fixed(state.state_digest().as_bytes()).map_err(scalar::codec)?;
    writer.write_u8(state.phase().tag()).map_err(scalar::codec)?;

    writer.write_collection_len(state.baseline_evidence().len()).map_err(scalar::codec)?;
    for value in state.baseline_evidence() {
        writer.write_fixed(value.artifact_digest().as_bytes()).map_err(scalar::codec)?;
        writer.write_fixed(value.evidence_digest().as_bytes()).map_err(scalar::codec)?;
    }
    writer.write_collection_len(state.diagnoses().len()).map_err(scalar::codec)?;
    for value in state.diagnoses() {
        binding::write_diagnosis(&mut writer, value)?;
    }
    writer.write_collection_len(state.manifests().len()).map_err(scalar::codec)?;
    for value in state.manifests() {
        change::write_manifest(&mut writer, value)?;
    }
    writer.write_collection_len(state.variants().len()).map_err(scalar::codec)?;
    for value in state.variants() {
        change::write_variant(&mut writer, value)?;
    }
    writer.write_collection_len(state.evaluations().len()).map_err(scalar::codec)?;
    for value in state.evaluations() {
        writer.write_fixed(value.variant_id().as_bytes()).map_err(scalar::codec)?;
        evaluation::write(&mut writer, value.evidence())?;
    }
    writer.write_collection_len(state.attributions().len()).map_err(scalar::codec)?;
    for value in state.attributions() {
        attribution::write(&mut writer, value)?;
    }
    writer.write_collection_len(state.assessments().len()).map_err(scalar::codec)?;
    for value in state.assessments() {
        selection::write_assessment(&mut writer, value)?;
    }
    write_option(&mut writer, state.selection(), selection::write_selection)?;
    write_option(&mut writer, state.proposal(), proposal::write_promotion)?;
    write_option(&mut writer, state.publication().as_ref(), |writer, value| {
        proposal::write_publication(writer, *value)
    })?;
    write_terminal(&mut writer, state.terminal())?;
    Ok(writer.into_bytes())
}

pub(crate) fn decode_campaign_state(bytes: &[u8]) -> Result<CampaignState, EvolutionError> {
    let mut reader = CanonicalReader::new(bytes, CodecLimits::PRODUCTION);
    let campaign_id = scalar::campaign_id(&mut reader).map_err(scalar::codec)?;
    let project_id = scalar::project_id(&mut reader).map_err(scalar::codec)?;
    let encoded_binding = scalar::digest(&mut reader)?;
    let baseline = binding::production(&mut reader)?;
    let limits = binding::limits(&mut reader)?;
    let policy = binding::policy(&mut reader, limits)?;
    let sequence = reader.read_u64().map_err(scalar::codec)?;
    let last_event = scalar::event_id(&mut reader).map_err(scalar::codec)?;
    let encoded_state = scalar::digest(&mut reader)?;
    let phase = campaign_phase(reader.read_u8().map_err(scalar::codec)?)?;
    let maximum_manifests = usize::from(limits.manifests());
    let maximum_variants = usize::from(limits.variants());

    let baseline_evidence = read_vec(&mut reader, maximum_manifests, |reader| {
        Ok(BaselineEvidence::new(scalar::digest(reader)?, scalar::digest(reader)?))
    })?;
    let diagnoses =
        read_vec(&mut reader, maximum_manifests, |reader| binding::diagnosis(reader, limits))?;
    let manifests =
        read_vec(&mut reader, maximum_manifests, |reader| change::manifest(reader, limits))?;
    let variants =
        read_vec(&mut reader, maximum_variants, |reader| change::variant(reader, limits))?;
    let evaluations = read_vec(&mut reader, maximum_variants, |reader| {
        Ok(VariantEvaluation::new(
            VariantId::new(reader.read_fixed().map_err(scalar::codec)?)?,
            evaluation::read(reader)?,
        ))
    })?;
    let attributions =
        read_vec(&mut reader, maximum_variants, |reader| attribution::read(reader, limits))?;
    let assessments =
        read_vec(&mut reader, maximum_variants, |reader| selection::assessment(reader, limits))?;
    let selected = read_option(&mut reader, selection::selection)?;
    let promotion = read_option(&mut reader, proposal::promotion)?;
    let publication = read_option(&mut reader, proposal::publication)?;
    let terminal = terminal(&mut reader)?;
    reader.finish().map_err(scalar::codec)?;

    let binding_digest = digest_parts(
        b"peritus.f0.campaign-binding.v1\0",
        &[
            campaign_id.as_bytes(),
            project_id.as_bytes(),
            baseline.digest().as_bytes(),
            policy.digest().as_bytes(),
            limits.digest().as_bytes(),
        ],
    );
    if sequence == 0
        || encoded_binding != binding_digest
        || !terminal_matches(phase, terminal)
        || baseline_evidence.windows(2).any(|pair| pair[0] >= pair[1])
        || diagnoses.windows(2).any(|pair| pair[0].digest() >= pair[1].digest())
        || manifests.windows(2).any(|pair| pair[0].id() >= pair[1].id())
        || variants.windows(2).any(|pair| pair[0].id() >= pair[1].id())
        || evaluations.windows(2).any(|pair| pair[0].variant_id() >= pair[1].variant_id())
        || attributions.windows(2).any(|pair| pair[0].variant_id() >= pair[1].variant_id())
        || assessments.windows(2).any(|pair| pair[0].variant_id() >= pair[1].variant_id())
    {
        return Err(scalar::protocol());
    }

    let mut state = CampaignState {
        campaign_id,
        project_id,
        binding_digest,
        baseline,
        policy,
        limits,
        sequence,
        last_event,
        state_digest: encoded_state,
        phase,
        baseline_evidence,
        diagnoses,
        manifests,
        variants,
        evaluations,
        attributions,
        assessments,
        selection: selected,
        proposal: promotion,
        publication,
        terminal,
    };
    state.refresh_digest();
    if state.state_digest() != encoded_state {
        return Err(scalar::protocol());
    }
    Ok(state)
}

fn write_terminal(
    writer: &mut CanonicalWriter,
    value: Option<CampaignTerminal>,
) -> Result<(), EvolutionError> {
    match value {
        None => writer.write_u8(0).map_err(scalar::codec),
        Some(CampaignTerminal::Promoted { promotion_id, activation_digest }) => {
            writer.write_u8(1).map_err(scalar::codec)?;
            writer.write_fixed(promotion_id.as_bytes()).map_err(scalar::codec)?;
            writer.write_fixed(activation_digest.as_bytes()).map_err(scalar::codec)
        }
        Some(CampaignTerminal::Rejected { selection_digest }) => {
            writer.write_u8(2).map_err(scalar::codec)?;
            writer.write_fixed(selection_digest.as_bytes()).map_err(scalar::codec)
        }
        Some(CampaignTerminal::Failed { reason_digest }) => {
            writer.write_u8(3).map_err(scalar::codec)?;
            writer.write_fixed(reason_digest.as_bytes()).map_err(scalar::codec)
        }
        Some(CampaignTerminal::Cancelled { reason_digest }) => {
            writer.write_u8(4).map_err(scalar::codec)?;
            writer.write_fixed(reason_digest.as_bytes()).map_err(scalar::codec)
        }
    }
}

fn terminal(reader: &mut CanonicalReader<'_>) -> Result<Option<CampaignTerminal>, EvolutionError> {
    match reader.read_u8().map_err(scalar::codec)? {
        0 => Ok(None),
        1 => Ok(Some(CampaignTerminal::Promoted {
            promotion_id: PromotionId::new(reader.read_fixed().map_err(scalar::codec)?)?,
            activation_digest: scalar::digest(reader)?,
        })),
        2 => Ok(Some(CampaignTerminal::Rejected { selection_digest: scalar::digest(reader)? })),
        3 => Ok(Some(CampaignTerminal::Failed { reason_digest: scalar::digest(reader)? })),
        4 => Ok(Some(CampaignTerminal::Cancelled { reason_digest: scalar::digest(reader)? })),
        _ => Err(scalar::protocol()),
    }
}

const fn terminal_matches(phase: CampaignPhase, value: Option<CampaignTerminal>) -> bool {
    matches!(
        (phase, value),
        (CampaignPhase::Promoted, Some(CampaignTerminal::Promoted { .. }))
            | (CampaignPhase::Rejected, Some(CampaignTerminal::Rejected { .. }))
            | (CampaignPhase::Failed, Some(CampaignTerminal::Failed { .. }))
            | (CampaignPhase::Cancelled, Some(CampaignTerminal::Cancelled { .. }))
    ) || (!phase.terminal() && value.is_none())
}

const fn campaign_phase(tag: u8) -> Result<CampaignPhase, EvolutionError> {
    match tag {
        0 => Ok(CampaignPhase::Draft),
        1 => Ok(CampaignPhase::Frozen),
        2 => Ok(CampaignPhase::BaselineRunning),
        3 => Ok(CampaignPhase::Diagnosing),
        4 => Ok(CampaignPhase::Proposing),
        5 => Ok(CampaignPhase::VariantsRunning),
        6 => Ok(CampaignPhase::Attributing),
        7 => Ok(CampaignPhase::PromotionReview),
        8 => Ok(CampaignPhase::Promoted),
        9 => Ok(CampaignPhase::Rejected),
        10 => Ok(CampaignPhase::Failed),
        11 => Ok(CampaignPhase::Cancelled),
        _ => Err(scalar::protocol()),
    }
}

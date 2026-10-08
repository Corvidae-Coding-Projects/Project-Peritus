//! Complete canonical campaign checkpoint semantics.

use std::sync::Arc;

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecLimits};

use crate::{
    BaselineEvidence, CampaignPhase, CampaignState, CampaignTerminal, EvaluationGeneration,
    EvolutionError, PromotionId, VariantEvaluation, VariantId, identity::digest_parts,
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
    if let Some(history) = state.evaluation_history() {
        writer.write_u8(1).map_err(scalar::codec)?;
        writer.write_collection_len(history.len()).map_err(scalar::codec)?;
        for generation in history {
            write_generation(&mut writer, generation)?;
        }
    }
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
    let maximum_manifests = limits.manifests_limit().map_or(usize::MAX, usize::from);
    let maximum_variants = limits.variants_limit().map_or(usize::MAX, usize::from);

    let baseline_evidence = read_vec(&mut reader, maximum_manifests, 2 * 32, |reader| {
        Ok(BaselineEvidence::new(scalar::digest(reader)?, scalar::digest(reader)?))
    })?;
    let diagnoses = read_vec(
        &mut reader,
        maximum_manifests,
        96 + 2 * 16 + 32 + 16 + 3 * 32 + 8 + 16 + 8 + 4,
        |reader| binding::diagnosis(reader, limits),
    )?;
    let manifests =
        read_vec(&mut reader, maximum_manifests, 3 * (16 + 8 + 32) + 6 * 4, |reader| {
            change::manifest(reader, limits)
        })?;
    let variants = read_vec(
        &mut reader,
        maximum_variants,
        2 * (96 + 56 + 32 + 32 + 2 * 33) + 3 * 4 + 3,
        |reader| change::variant(reader, limits),
    )?;
    // The fixed outer variant identity is a schema lower bound; evaluation validates its body.
    let evaluations = read_vec(&mut reader, maximum_variants, 16, |reader| {
        Ok(VariantEvaluation::new(
            VariantId::new(reader.read_fixed().map_err(scalar::codec)?)?,
            evaluation::read(reader)?,
        ))
    })?;
    let attributions = read_vec(&mut reader, maximum_variants, 16 + 32 + 1 + 4, |reader| {
        attribution::read(reader, limits)
    })?;
    let assessments = read_vec(&mut reader, maximum_variants, 2 * 16 + 2 * 32 + 4, |reader| {
        selection::assessment(reader)
    })?;
    let selected = read_option(&mut reader, selection::selection)?;
    let promotion = read_option(&mut reader, proposal::promotion)?;
    let publication = read_option(&mut reader, proposal::publication)?;
    let terminal = terminal(&mut reader)?;
    let evaluation_history = if reader.remaining() == 0 {
        None
    } else {
        if reader.read_u8().map_err(scalar::codec)? != 1 {
            return Err(scalar::protocol());
        }
        Some(read_vec(&mut reader, usize::MAX, 16 + 8, |reader| {
            read_generation(reader, limits)
        })?)
    };
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
        || evaluation_history.as_ref().is_some_and(|history| {
            !valid_history(history, &evaluations, &attributions, &assessments)
        })
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
        baseline_evidence: Arc::new(baseline_evidence),
        diagnoses: Arc::new(diagnoses),
        manifests: Arc::new(manifests),
        variants: Arc::new(variants),
        evaluations: Arc::new(evaluations),
        evaluation_history: evaluation_history.map(Arc::new),
        attributions: Arc::new(attributions),
        assessments: Arc::new(assessments),
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

fn write_generation(
    writer: &mut CanonicalWriter,
    value: &EvaluationGeneration,
) -> Result<(), EvolutionError> {
    writer.write_fixed(value.variant_id().as_bytes()).map_err(scalar::codec)?;
    writer.write_u64(value.generation()).map_err(scalar::codec)?;
    evaluation::write(writer, value.evidence())?;
    write_option(writer, value.supersession(), evaluation::write_supersession)?;
    write_option(writer, value.attribution(), attribution::write)?;
    write_option(writer, value.assessment(), selection::write_assessment)
}

fn read_generation(
    reader: &mut CanonicalReader<'_>,
    limits: crate::EvolutionLimits,
) -> Result<EvaluationGeneration, EvolutionError> {
    let variant_id = VariantId::new(reader.read_fixed().map_err(scalar::codec)?)?;
    let generation = reader.read_u64().map_err(scalar::codec)?;
    let evidence = evaluation::read(reader)?;
    let supersession = read_option(reader, evaluation::read_supersession)?;
    let attribution = read_option(reader, |reader| attribution::read(reader, limits))?;
    let assessment = read_option(reader, selection::assessment)?;
    EvaluationGeneration::from_exact_parts(
        variant_id,
        generation,
        evidence,
        supersession,
        attribution,
        assessment,
    )
}

fn valid_history(
    history: &[EvaluationGeneration],
    evaluations: &[VariantEvaluation],
    attributions: &[crate::AttributionRecord],
    assessments: &[crate::VariantAssessment],
) -> bool {
    if history.is_empty() || history.windows(2).any(|pair| {
        (pair[0].variant_id(), pair[0].generation())
            >= (pair[1].variant_id(), pair[1].generation())
    }) {
        return false;
    }
    for (index, generation) in history.iter().enumerate() {
        let predecessor = index.checked_sub(1).and_then(|value| history.get(value));
        if predecessor.is_none_or(|value| value.variant_id() != generation.variant_id()) {
            if generation.generation() != 1 || generation.supersession().is_some() {
                return false;
            }
        } else if predecessor.is_none_or(|value| {
            generation.generation() != value.generation().saturating_add(1)
                || generation
                    .supersession()
                    .is_none_or(|lineage| !lineage.matches_predecessor(value.evidence()))
        }) {
            return false;
        }
    }
    let mut active_generations = 0_usize;
    for index in 0..history.len() {
        if history
            .get(index + 1)
            .is_none_or(|next| next.variant_id() != history[index].variant_id())
        {
            active_generations = active_generations.saturating_add(1);
        }
    }
    if evaluations.len() != active_generations {
        return false;
    }
    evaluations.iter().all(|evaluation| {
        let Some(latest) = history
            .iter()
            .rev()
            .find(|value| value.variant_id() == evaluation.variant_id())
        else {
            return false;
        };
        let attribution = attributions
            .binary_search_by_key(&evaluation.variant_id(), crate::AttributionRecord::variant_id)
            .ok()
            .map(|index| &attributions[index]);
        let assessment = assessments
            .binary_search_by_key(&evaluation.variant_id(), crate::VariantAssessment::variant_id)
            .ok()
            .map(|index| &assessments[index]);
        latest.evidence() == evaluation.evidence()
            && latest.attribution() == attribution
            && latest.assessment() == assessment
    })
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

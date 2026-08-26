//! Closed campaign transition table and bounded collection mutations.

use crate::{
    BaselineEvidence, CampaignCommandKind, CampaignPhase, CampaignState, CampaignTerminal,
    EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery, SelectionDecision,
    VariantEvaluation, identity::digest_parts,
};
use peritus_types::Sha256Digest;

use self::collection::{arm_digest, insert_by, insert_unique};

mod collection;

#[allow(clippy::too_many_lines, reason = "the closed campaign transition table stays visible")]
pub(super) fn apply_kind(
    prior: Option<&CampaignState>,
    campaign_id: crate::EvolutionCampaignId,
    sequence: u64,
    event_id: peritus_types::EventId,
    policy_digest: Sha256Digest,
    kind: &CampaignCommandKind,
) -> Result<CampaignState, EvolutionError> {
    if let CampaignCommandKind::CreateCampaign { project_id, baseline, policy, limits } = kind {
        if prior.is_some() || sequence != 1 || policy.policy().digest() != policy_digest {
            return Err(transition());
        }
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
        return Ok(CampaignState {
            campaign_id,
            project_id: *project_id,
            binding_digest,
            baseline: *baseline,
            policy: policy.clone(),
            limits: *limits,
            sequence,
            last_event: event_id,
            state_digest: Sha256Digest::new([0; 32]),
            phase: CampaignPhase::Draft,
            baseline_evidence: Vec::new(),
            diagnoses: Vec::new(),
            manifests: Vec::new(),
            variants: Vec::new(),
            evaluations: Vec::new(),
            attributions: Vec::new(),
            assessments: Vec::new(),
            selection: None,
            proposal: None,
            publication: None,
            terminal: None,
        });
    }
    let mut state = prior.cloned().ok_or_else(transition)?;
    state.sequence = sequence;
    state.last_event = event_id;
    match kind {
        CampaignCommandKind::CreateCampaign { .. } => return Err(transition()),
        CampaignCommandKind::FreezeCampaign => {
            require(&state, &[CampaignPhase::Draft])?;
            state.phase = CampaignPhase::Frozen;
        }
        CampaignCommandKind::RecordBaselineEvidence { artifact_digest, evidence_digest } => {
            require(&state, &[CampaignPhase::Frozen, CampaignPhase::BaselineRunning])?;
            insert_unique(
                &mut state.baseline_evidence,
                BaselineEvidence::new(*artifact_digest, *evidence_digest),
                usize::from(state.limits.manifests()),
            )?;
            state.phase = CampaignPhase::BaselineRunning;
        }
        CampaignCommandKind::SubmitDiagnosis(evidence) => {
            require(&state, &[CampaignPhase::BaselineRunning, CampaignPhase::Diagnosing])?;
            if state.baseline_evidence.is_empty()
                || evidence.revision() != state.baseline.revision()
            {
                return Err(binding("diagnosis differs from the frozen baseline"));
            }
            insert_by(
                &mut state.diagnoses,
                evidence.clone(),
                crate::PublishedDebuggerEvidence::digest,
                usize::from(state.limits.manifests()),
            )?;
            state.phase = CampaignPhase::Diagnosing;
        }
        CampaignCommandKind::AdmitChangeManifest(manifest) => {
            require(&state, &[CampaignPhase::Diagnosing, CampaignPhase::Proposing])?;
            if manifest.baseline() != state.baseline.harness_revision()
                || manifest.diagnoses().iter().any(|value| {
                    state
                        .diagnoses
                        .binary_search_by_key(
                            &value.digest(),
                            crate::PublishedDebuggerEvidence::digest,
                        )
                        .is_err()
                })
            {
                return Err(binding(
                    "manifest baseline or diagnosis was not frozen by this campaign",
                ));
            }
            insert_by(
                &mut state.manifests,
                manifest.clone(),
                crate::ChangeManifest::id,
                usize::from(state.limits.manifests()),
            )?;
            state.phase = CampaignPhase::Proposing;
        }
        CampaignCommandKind::AdmitVariant(variant) => {
            require(&state, &[CampaignPhase::Proposing])?;
            if variant.baseline() != state.baseline
                || variant.manifest_ids().iter().any(|id| {
                    state.manifests.binary_search_by_key(id, crate::ChangeManifest::id).is_err()
                })
                || state.variants.iter().any(|value| value.candidate() == variant.candidate())
            {
                return Err(binding(
                    "variant differs from campaign manifests or duplicates a candidate",
                ));
            }
            let maximum = state.limits.variants().min(state.policy.policy().maximum_variants());
            insert_by(
                &mut state.variants,
                variant.clone(),
                crate::VariantDefinition::id,
                usize::from(maximum),
            )?;
        }
        CampaignCommandKind::AdmitEvaluation { variant_id, evidence } => {
            require(&state, &[CampaignPhase::Proposing, CampaignPhase::VariantsRunning])?;
            let variant = state
                .variants
                .binary_search_by_key(variant_id, crate::VariantDefinition::id)
                .ok()
                .map(|index| &state.variants[index])
                .ok_or_else(|| binding("unknown variant"))?;
            if evidence.baseline().digest() != arm_digest(variant.baseline())
                || evidence.candidate().digest() != arm_digest(variant.candidate())
            {
                return Err(binding("evaluation arms differ from the admitted variant"));
            }
            insert_by(
                &mut state.evaluations,
                VariantEvaluation::new(*variant_id, evidence.clone()),
                VariantEvaluation::variant_id,
                usize::from(state.limits.variants()),
            )?;
            state.phase = CampaignPhase::VariantsRunning;
        }
        CampaignCommandKind::CompleteAttribution { attribution, assessment } => {
            require(&state, &[CampaignPhase::VariantsRunning, CampaignPhase::Attributing])?;
            if attribution.variant_id() != assessment.variant_id()
                || attribution.id() != assessment.attribution_id()
                || state
                    .evaluations
                    .binary_search_by_key(&attribution.variant_id(), VariantEvaluation::variant_id)
                    .is_err()
            {
                return Err(binding("attribution, assessment, and evaluation differ"));
            }
            insert_by(
                &mut state.attributions,
                attribution.clone(),
                crate::AttributionRecord::variant_id,
                usize::from(state.limits.variants()),
            )?;
            insert_by(
                &mut state.assessments,
                assessment.clone(),
                crate::VariantAssessment::variant_id,
                usize::from(state.limits.variants()),
            )?;
            state.phase = CampaignPhase::Attributing;
        }
        CampaignCommandKind::RecordSelection(selection) => {
            require(&state, &[CampaignPhase::Attributing])?;
            if state.assessments.len() != state.variants.len()
                || selection.policy_digest() != state.policy.policy().digest()
                || selection.assessment_digests()
                    != state
                        .assessments
                        .iter()
                        .map(crate::VariantAssessment::digest)
                        .collect::<Vec<_>>()
            {
                return Err(binding("selection does not cover every admitted variant exactly"));
            }
            state.selection = Some(selection.clone());
            if matches!(selection.decision(), SelectionDecision::NoEligibleVariant(_)) {
                state.phase = CampaignPhase::Rejected;
                state.terminal =
                    Some(CampaignTerminal::Rejected { selection_digest: selection.digest() });
            } else {
                state.phase = CampaignPhase::PromotionReview;
            }
        }
        CampaignCommandKind::RequestPromotion(proposal) => {
            require(&state, &[CampaignPhase::PromotionReview])?;
            if proposal.project_id() != state.project_id
                || proposal.campaign_id() != state.campaign_id
                || proposal.current() != state.baseline
                || proposal.policy_digest() != state.policy.digest()
                || state.selection.as_ref().map(crate::SelectionRecord::digest)
                    != Some(proposal.selection_digest())
                || state
                    .variants
                    .binary_search_by_key(&proposal.variant_id(), crate::VariantDefinition::id)
                    .ok()
                    .map(|index| state.variants[index].digest())
                    != Some(proposal.variant_digest())
                || state
                    .attributions
                    .binary_search_by_key(
                        &proposal.variant_id(),
                        crate::AttributionRecord::variant_id,
                    )
                    .ok()
                    .map(|index| state.attributions[index].digest())
                    != Some(proposal.attribution_digest())
                || state
                    .evaluations
                    .binary_search_by_key(&proposal.variant_id(), VariantEvaluation::variant_id)
                    .ok()
                    .map(|index| state.evaluations[index].evidence().digest())
                    != Some(proposal.evaluation_digest())
            {
                return Err(binding("promotion proposal differs from frozen campaign truth"));
            }
            state.proposal = Some(proposal.clone());
        }
        CampaignCommandKind::ActivatePromotion { activation_digest } => {
            require(&state, &[CampaignPhase::PromotionReview])?;
            let proposal = state.proposal.as_ref().ok_or_else(transition)?;
            state.phase = CampaignPhase::Promoted;
            state.terminal = Some(CampaignTerminal::Promoted {
                promotion_id: proposal.id(),
                activation_digest: *activation_digest,
            });
        }
        CampaignCommandKind::RecordPublication(publication) => {
            if state.publication.is_some() || state.terminal.is_none() {
                return Err(transition());
            }
            state.publication = Some(*publication);
        }
        CampaignCommandKind::CancelCampaign { reason_digest } => {
            state.phase = CampaignPhase::Cancelled;
            state.terminal = Some(CampaignTerminal::Cancelled { reason_digest: *reason_digest });
        }
        CampaignCommandKind::FailCampaign { reason_digest } => {
            state.phase = CampaignPhase::Failed;
            state.terminal = Some(CampaignTerminal::Failed { reason_digest: *reason_digest });
        }
    }
    Ok(state)
}

fn require(state: &CampaignState, allowed: &[CampaignPhase]) -> Result<(), EvolutionError> {
    if allowed.contains(&state.phase()) { Ok(()) } else { Err(transition()) }
}

pub(super) const fn transition() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::IllegalTransition,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::CorrectInput,
        "campaign command is illegal in the current phase",
    )
}

const fn binding(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::CorrectInput,
        detail,
    )
}

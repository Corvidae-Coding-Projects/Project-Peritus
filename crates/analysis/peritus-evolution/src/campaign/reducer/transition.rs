//! Closed campaign transition table and bounded collection mutations.

use std::sync::Arc;

use crate::{
    BaselineEvidence, CampaignCommandKind, CampaignPhase, CampaignState, CampaignTerminal,
    EvaluationGeneration, EvolutionError, EvolutionErrorKind, EvolutionOperation,
    EvolutionRecovery, SelectionDecision, VariantEvaluation, identity::digest_parts,
    select_variant,
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
        let binding_digest =
            campaign_binding_digest(campaign_id, *project_id, *baseline, policy, *limits);
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
            baseline_evidence: Arc::new(Vec::new()),
            diagnoses: Arc::new(Vec::new()),
            manifests: Arc::new(Vec::new()),
            variants: Arc::new(Vec::new()),
            evaluations: Arc::new(Vec::new()),
            evaluation_history: None,
            attributions: Arc::new(Vec::new()),
            assessments: Arc::new(Vec::new()),
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
            if state.policy.policy().has_explicit_measurements() {
                return Err(measurement_capability());
            }
            state.phase = CampaignPhase::Frozen;
        }
        CampaignCommandKind::FreezeCampaignWithMeasurements { available_measurements } => {
            require(&state, &[CampaignPhase::Draft])?;
            if !available_measurements
                .satisfies(state.policy.policy().required_measurements())
            {
                return Err(measurement_capability());
            }
            state.phase = CampaignPhase::Frozen;
        }
        CampaignCommandKind::ExpandScope { limits } => {
            if !limits.is_strict_expansion_of(state.limits) {
                return Err(binding("campaign scope successor is not a strict expansion"));
            }
            state.limits = *limits;
            state.binding_digest = campaign_binding_digest(
                state.campaign_id,
                state.project_id,
                state.baseline,
                &state.policy,
                *limits,
            );
        }
        CampaignCommandKind::RecordBaselineEvidence { artifact_digest, evidence_digest } => {
            require_work_open(&state)?;
            insert_unique(
                Arc::make_mut(&mut state.baseline_evidence),
                BaselineEvidence::new(*artifact_digest, *evidence_digest),
                state.limits.manifests_limit().map(usize::from),
            )?;
            advance(&mut state, CampaignPhase::BaselineRunning);
        }
        CampaignCommandKind::SubmitDiagnosis(evidence) => {
            require_work_open(&state)?;
            if state.baseline_evidence.is_empty()
                || evidence.revision() != state.baseline.revision()
            {
                return Err(binding("diagnosis differs from the frozen baseline"));
            }
            insert_by(
                Arc::make_mut(&mut state.diagnoses),
                evidence.clone(),
                crate::PublishedDebuggerEvidence::digest,
                state.limits.manifests_limit().map(usize::from),
            )?;
            advance(&mut state, CampaignPhase::Diagnosing);
        }
        CampaignCommandKind::AdmitChangeManifest(manifest) => {
            require_work_open(&state)?;
            if state.diagnoses.is_empty()
                || manifest.baseline() != state.baseline.harness_revision()
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
                Arc::make_mut(&mut state.manifests),
                manifest.clone(),
                crate::ChangeManifest::id,
                state.limits.manifests_limit().map(usize::from),
            )?;
            advance(&mut state, CampaignPhase::Proposing);
        }
        CampaignCommandKind::AdmitVariant(variant) => {
            require_work_open(&state)?;
            if state.manifests.is_empty()
                || variant.baseline() != state.baseline
                || variant.manifest_ids().iter().zip(variant.manifest_digests()).any(
                    |(id, digest)| {
                        state
                            .manifests
                            .binary_search_by_key(id, crate::ChangeManifest::id)
                            .ok()
                            .is_none_or(|index| state.manifests[index].digest() != *digest)
                    },
                )
                || state.variants.iter().any(|value| value.candidate() == variant.candidate())
            {
                return Err(binding(
                    "variant differs from campaign manifests or duplicates a candidate",
                ));
            }
            let maximum = state.limits.variants_limit().map_or_else(
                || state.policy.policy().maximum_variants(),
                |limit| limit.min(state.policy.policy().maximum_variants()),
            );
            insert_by(
                Arc::make_mut(&mut state.variants),
                variant.clone(),
                crate::VariantDefinition::id,
                Some(usize::from(maximum)),
            )?;
        }
        CampaignCommandKind::AdmitEvaluation { variant_id, evidence } => {
            require_work_open(&state)?;
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
                Arc::make_mut(&mut state.evaluations),
                VariantEvaluation::new(*variant_id, evidence.clone()),
                VariantEvaluation::variant_id,
                state.limits.variants_limit().map(usize::from),
            )?;
            if let Some(history) = &mut state.evaluation_history {
                insert_generation(
                    Arc::make_mut(history),
                    EvaluationGeneration::initial(*variant_id, evidence.clone(), None, None)?,
                )?;
            }
            advance(&mut state, CampaignPhase::VariantsRunning);
        }
        CampaignCommandKind::SupersedeEvaluation(supersession) => {
            require_work_open(&state)?;
            let variant_id = supersession.variant_id();
            let variant = state
                .variants
                .binary_search_by_key(&variant_id, crate::VariantDefinition::id)
                .ok()
                .map(|index| &state.variants[index])
                .ok_or_else(|| binding("evaluation successor names an unknown variant"))?;
            let evaluation_index = state
                .evaluations
                .binary_search_by_key(&variant_id, VariantEvaluation::variant_id)
                .map_err(|_| binding("evaluation successor has no active predecessor"))?;
            let predecessor = state.evaluations[evaluation_index].evidence();
            if !supersession.matches_predecessor(predecessor)
                || supersession.successor().baseline().digest() != arm_digest(variant.baseline())
                || supersession.successor().candidate().digest() != arm_digest(variant.candidate())
            {
                return Err(binding(
                    "evaluation successor differs from the active predecessor or variant arms",
                ));
            }
            migrate_evaluation_history(&mut state)?;
            let history = state
                .evaluation_history
                .as_mut()
                .ok_or_else(|| binding("evaluation generation ledger is absent"))?;
            let history = Arc::make_mut(history);
            let generation = history
                .iter()
                .rev()
                .find(|value| value.variant_id() == variant_id)
                .map(EvaluationGeneration::generation)
                .ok_or_else(|| binding("active evaluation generation is absent"))?
                .checked_add(1)
                .ok_or_else(transition)?;
            insert_generation(
                history,
                EvaluationGeneration::successor(generation, supersession.clone())?,
            )?;
            Arc::make_mut(&mut state.evaluations)[evaluation_index] =
                VariantEvaluation::new(variant_id, supersession.successor().clone());
            if let Ok(index) = state
                .attributions
                .binary_search_by_key(&variant_id, crate::AttributionRecord::variant_id)
            {
                Arc::make_mut(&mut state.attributions).remove(index);
            }
            if let Ok(index) = state
                .assessments
                .binary_search_by_key(&variant_id, crate::VariantAssessment::variant_id)
            {
                Arc::make_mut(&mut state.assessments).remove(index);
            }
            advance(&mut state, CampaignPhase::VariantsRunning);
        }
        CampaignCommandKind::CompleteAttribution { attribution, assessment } => {
            require_work_open(&state)?;
            let evaluation = state
                .evaluations
                .binary_search_by_key(&attribution.variant_id(), VariantEvaluation::variant_id)
                .ok()
                .map(|index| &state.evaluations[index]);
            if attribution.variant_id() != assessment.variant_id()
                || attribution.id() != assessment.attribution_id()
                || assessment.policy_digest() != state.policy.policy().digest()
                || !state
                    .limits
                    .accepts_attribution_entries(attribution.entries().len())
                || !crate::selection::criteria_match_policy(
                    assessment.criteria(),
                    state.policy.policy(),
                )
                || evaluation.is_none_or(|value| {
                    attribution.evaluation_digest() != value.evidence().digest()
                        || assessment.evidence_digest() != value.evidence().digest()
                })
            {
                return Err(binding(
                    "attribution, assessment, evaluation, or representability contract differs",
                ));
            }
            insert_by(
                Arc::make_mut(&mut state.attributions),
                attribution.clone(),
                crate::AttributionRecord::variant_id,
                state.limits.variants_limit().map(usize::from),
            )?;
            insert_by(
                Arc::make_mut(&mut state.assessments),
                assessment.clone(),
                crate::VariantAssessment::variant_id,
                state.limits.variants_limit().map(usize::from),
            )?;
            if let Some(history) = &mut state.evaluation_history {
                let generation = Arc::make_mut(history)
                    .iter_mut()
                    .rev()
                    .find(|value| value.variant_id() == attribution.variant_id())
                    .ok_or_else(|| binding("active evaluation generation is absent"))?;
                generation.attach(attribution.clone(), assessment.clone())?;
            }
            advance(&mut state, CampaignPhase::Attributing);
        }
        CampaignCommandKind::RecordSelection(selection) => {
            require_work_open(&state)?;
            if state.assessments.len() != state.variants.len()
                || selection.policy_digest() != state.policy.policy().digest()
                || selection.assessment_digests()
                    != state
                        .assessments
                        .iter()
                        .map(crate::VariantAssessment::digest)
                        .collect::<Vec<_>>()
                || state.assessments.iter().any(|assessment| {
                    let evaluation_matches = state
                        .evaluations
                        .binary_search_by_key(
                            &assessment.variant_id(),
                            VariantEvaluation::variant_id,
                        )
                        .ok()
                        .is_some_and(|index| {
                            state.evaluations[index].evidence().digest()
                                == assessment.evidence_digest()
                        });
                    let attribution_matches = state
                        .attributions
                        .binary_search_by_key(
                            &assessment.variant_id(),
                            crate::AttributionRecord::variant_id,
                        )
                        .ok()
                        .is_some_and(|index| {
                            state.attributions[index].id() == assessment.attribution_id()
                                && state.attributions[index].evaluation_digest()
                                    == assessment.evidence_digest()
                        });
                    !evaluation_matches || !attribution_matches
                })
                || !matches!(
                    select_variant(state.policy.policy(), &state.assessments),
                    Ok(expected) if &expected == selection
                )
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

fn campaign_binding_digest(
    campaign_id: crate::EvolutionCampaignId,
    project_id: peritus_types::ProjectId,
    baseline: crate::ProductionHarnessBinding,
    policy: &crate::PromotionPolicyBinding,
    limits: crate::EvolutionLimits,
) -> Sha256Digest {
    digest_parts(
        b"peritus.f0.campaign-binding.v1\0",
        &[
            campaign_id.as_bytes(),
            project_id.as_bytes(),
            baseline.digest().as_bytes(),
            policy.digest().as_bytes(),
            limits.digest().as_bytes(),
        ],
    )
}

fn require(state: &CampaignState, allowed: &[CampaignPhase]) -> Result<(), EvolutionError> {
    if allowed.contains(&state.phase()) { Ok(()) } else { Err(transition()) }
}

const fn measurement_capability() -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::UnsupportedCapability,
        EvolutionOperation::TransitionCampaign,
        EvolutionRecovery::SuccessorCampaign,
        "evaluator cannot produce every measurement required by the promotion policy",
    )
}

fn require_work_open(state: &CampaignState) -> Result<(), EvolutionError> {
    if state.phase() != CampaignPhase::Draft
        && state.selection.is_none()
        && state.proposal.is_none()
        && state.terminal.is_none()
    {
        Ok(())
    } else {
        Err(transition())
    }
}

fn advance(state: &mut CampaignState, phase: CampaignPhase) {
    if !state.phase.terminal() && phase.tag() > state.phase.tag() {
        state.phase = phase;
    }
}

fn migrate_evaluation_history(state: &mut CampaignState) -> Result<(), EvolutionError> {
    if state.evaluation_history.is_some() {
        return Ok(());
    }
    let mut history = Vec::new();
    history
        .try_reserve_exact(state.evaluations.len())
        .map_err(|_| transition())?;
    for evaluation in state.evaluations.iter() {
        let variant_id = evaluation.variant_id();
        let attribution = state
            .attributions
            .binary_search_by_key(&variant_id, crate::AttributionRecord::variant_id)
            .ok()
            .map(|index| state.attributions[index].clone());
        let assessment = state
            .assessments
            .binary_search_by_key(&variant_id, crate::VariantAssessment::variant_id)
            .ok()
            .map(|index| state.assessments[index].clone());
        history.push(EvaluationGeneration::initial(
            variant_id,
            evaluation.evidence().clone(),
            attribution,
            assessment,
        )?);
    }
    state.evaluation_history = Some(Arc::new(history));
    Ok(())
}

fn insert_generation(
    history: &mut Vec<EvaluationGeneration>,
    generation: EvaluationGeneration,
) -> Result<(), EvolutionError> {
    let key = (generation.variant_id(), generation.generation());
    match history.binary_search_by_key(&key, |value| (value.variant_id(), value.generation())) {
        Ok(_) => Err(binding("evaluation generation already has an immutable snapshot")),
        Err(index) => {
            history.insert(index, generation);
            Ok(())
        }
    }
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

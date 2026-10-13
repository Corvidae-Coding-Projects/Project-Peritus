//! Deterministic production attribution, deny-wins selection, and inert proposal preparation.

use peritus_codec::{CodecLimits, encode_message};
use peritus_evolution::{
    CampaignCommandKind, CampaignPhase, CampaignState, EvolutionCampaignId, EvolutionError,
    PendingActivation, PointerCommandKind, PromotionProposal, PromotionReviewEvidence,
    SelectionDecision, VariantId, assess_variant, attribute, select_variant,
};
use peritus_types::RunId;

use super::{EvolutionOperator, EvolutionTransitionIds, incomplete, owner_error, pointer_command};

impl EvolutionOperator<'_> {
    /// Completes actual prediction attribution and independent policy assessment for all variants.
    ///
    /// Missing metrics and absent required reviews remain unavailable and deny eligibility. This
    /// operation never turns absent prerequisites into zero-valued observations or a passing review.
    ///
    /// # Errors
    /// Rejects incomplete evaluation populations, unknown/duplicate reviews, or evidence drift.
    pub fn complete_selection(
        &mut self,
        campaign: EvolutionCampaignId,
        reviews: &[(VariantId, RunId)],
        ids: EvolutionTransitionIds,
    ) -> Result<CampaignState, EvolutionError> {
        let state = self.campaign(campaign)?;
        if state.variants().is_empty()
            || state.evaluations().len() != state.variants().len()
            || reviews.windows(2).any(|pair| pair[0].0 >= pair[1].0)
            || reviews
                .iter()
                .any(|(variant, _)| !state.variants().iter().any(|value| value.id() == *variant))
        {
            return Err(incomplete(
                "selection needs every admitted variant evaluated and canonical review references",
            ));
        }
        let mut completed = Vec::with_capacity(state.variants().len());
        // Decide everything before the first mutation so an invalid review cannot leave partial
        // attribution commits. Already committed identical attribution remains restart-safe.
        for variant in state.variants() {
            self.bound_revision(variant.baseline())?;
            self.bound_revision(variant.candidate())?;
            let evaluation = state
                .evaluations()
                .iter()
                .find(|value| value.variant_id() == variant.id())
                .ok_or_else(|| incomplete("variant has no published evaluation"))?
                .evidence();
            let evidence =
                self.current_evidence(evaluation.evidence_id(), variant.baseline().revision())?;
            if evidence.payload_digest() != evaluation.report_digest() {
                return Err(incomplete(
                    "admitted evaluation is no longer current immutable evidence",
                ));
            }
            self.artifacts
                .verify(peritus_artifact_store::ArtifactDigest::from_sha256(
                    evaluation.report_artifact(),
                ))
                .map_err(owner_error)?;
            let manifests = state
                .manifests()
                .iter()
                .filter(|manifest| variant.manifest_ids().contains(&manifest.id()))
                .cloned()
                .collect::<Vec<_>>();
            let attribution = attribute(variant, &manifests, evaluation, state.limits())?;
            let review = reviews
                .iter()
                .find(|(id, _)| *id == variant.id())
                .map(|(_, run)| self.review(*run, variant.candidate()))
                .transpose()?;
            let assessment =
                assess_variant(variant, &attribution, evaluation, review, state.policy().policy())?;
            completed.push((attribution, assessment));
        }
        for (attribution, assessment) in completed {
            let latest = self.campaign(campaign)?;
            if let Some(existing) = latest
                .attributions()
                .iter()
                .find(|value| value.variant_id() == attribution.variant_id())
            {
                if existing != &attribution
                    || !latest.assessments().iter().any(|value| value == &assessment)
                {
                    return Err(incomplete(
                        "stored attribution or assessment differs from current owning evidence",
                    ));
                }
                continue;
            }
            self.apply_campaign(
                campaign,
                ids.child(b"attribution", attribution.digest())?,
                CampaignCommandKind::CompleteAttribution { attribution, assessment },
            )?;
        }
        let latest = self.campaign(campaign)?;
        let selection = select_variant(latest.policy().policy(), latest.assessments())?;
        if let Some(existing) = latest.selection() {
            if existing == &selection {
                return Ok(latest);
            }
            return Err(incomplete(
                "stored selection differs from recomputed frozen-policy decision",
            ));
        }
        self.apply_campaign(
            campaign,
            ids.child(b"selection", selection.digest())?,
            CampaignCommandKind::RecordSelection(selection),
        )
    }

    /// Prepares a selected promotion against the current pointer without activating it.
    ///
    /// The resulting exact action must still pass real B0/B1 capability and explicit approve-once
    /// authority. Candidate correctness alone never changes the production harness.
    ///
    /// # Errors
    /// Rejects no-selection/rejection, stale pointers, lost review, or changed evidence.
    pub fn prepare_promotion(
        &mut self,
        campaign: EvolutionCampaignId,
        review_run: Option<RunId>,
        ids: EvolutionTransitionIds,
    ) -> Result<PromotionProposal, EvolutionError> {
        let state = self.campaign(campaign)?;
        let pointer = self.pointer(state.project_id())?;
        if state.phase() != CampaignPhase::PromotionReview
            || pointer.current() != state.baseline()
            || pointer.policy() != state.policy()
        {
            return Err(incomplete("campaign has no selected current-baseline promotion"));
        }
        let selection = state.selection().ok_or_else(|| incomplete("campaign has no selection"))?;
        let SelectionDecision::Selected(id) = selection.decision() else {
            return Err(incomplete("frozen policy selected no eligible variant"));
        };
        let variant = state
            .variants()
            .iter()
            .find(|value| value.id() == *id)
            .ok_or_else(|| incomplete("selected variant is absent"))?;
        self.bound_revision(variant.baseline())?;
        self.bound_revision(variant.candidate())?;
        let attribution = state
            .attributions()
            .iter()
            .find(|value| value.variant_id() == *id)
            .ok_or_else(|| incomplete("selected attribution is absent"))?;
        let evaluation = state
            .evaluations()
            .iter()
            .find(|value| value.variant_id() == *id)
            .ok_or_else(|| incomplete("selected evaluation is absent"))?
            .evidence();
        self.current_evidence(evaluation.evidence_id(), variant.baseline().revision())?;
        let review: Option<PromotionReviewEvidence> =
            review_run.map(|run| self.review(run, variant.candidate())).transpose()?;
        let assessment =
            assess_variant(variant, attribution, evaluation, review, state.policy().policy())?;
        if !assessment.eligible() || !state.assessments().iter().any(|value| value == &assessment) {
            return Err(incomplete(
                "promotion assessment no longer matches exact current review/evidence",
            ));
        }
        let proposal = if let Some(existing) = state.proposal() {
            if existing.review() != review {
                return Err(incomplete("prepared proposal belongs to another independent review"));
            }
            existing.clone()
        } else {
            let frame =
                peritus_evolution::CampaignStateFrame::from_state(&state).map_err(owner_error)?;
            let bytes = encode_message(&frame, CodecLimits::PRODUCTION).map_err(owner_error)?;
            let artifact = peritus_evolution::finalize_evolution_artifact(
                self.artifacts,
                &bytes,
                state.state_digest(),
                ids.event,
            )?;
            let proposal = PromotionProposal::new(
                state.project_id(),
                campaign,
                pointer.current(),
                variant,
                attribution,
                evaluation,
                review,
                state.policy(),
                selection,
                artifact.artifact_digest().sha256(),
            )?;
            self.apply_campaign(
                campaign,
                ids.child(b"request-promotion", proposal.digest())?,
                CampaignCommandKind::RequestPromotion(proposal.clone()),
            )?;
            proposal
        };
        let current = self.pointer(state.project_id())?;
        match current.pending() {
            Some(PendingActivation::Promotion(value)) if value == &proposal => return Ok(proposal),
            Some(_) => return Err(incomplete("production pointer has another prepared action")),
            None => {}
        }
        let command = pointer_command(
            Some(&current),
            state.project_id(),
            current.policy().digest(),
            ids.child(b"prepare-pointer", proposal.digest())?,
            PointerCommandKind::PreparePromotion(proposal.clone()),
        )?;
        self.commit_pointer(&command)?;
        Ok(proposal)
    }
}

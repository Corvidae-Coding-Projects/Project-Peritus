//! Captures F0 inputs only from exact durable E1, E2, E3, and D2 owning states.

use peritus_artifact_store::ArtifactDigest;
use peritus_debugger::{DebuggerJobId, ValidatedReport};
use peritus_eval::{EvaluationCampaignId, FrozenEvaluationProfile, ValidatedEvaluationReport};
use peritus_evidence::{EvidenceRecord, FreshnessRequirement};
use peritus_evolution::{
    CampaignCommandKind, CampaignState, ChangeManifest, DiagnosisCitation, EvolutionCampaignId,
    EvolutionError, InteractionGroupId, ProductionHarnessBinding, PromotionReviewEvidence,
    PublishedDebuggerEvidence, PublishedEvaluationEvidence, VariantDefinition, VariantId,
};
use peritus_harness::{domain::HarnessRevision, runtime::GoverningHarnessBinding};
use peritus_types::{EvidenceId, RevisionTuple, RunId};

use super::{
    EvolutionOperator, EvolutionTransitionIds, MaterializedHarnessRef, incomplete, owner_error,
};

/// Resolves one frozen E3 arm through its actual retained E1 materialization.
pub(crate) fn resolve_materialized_arm(
    journal: &peritus_journal::SqliteJournal,
    artifacts: &peritus_artifact_store::ArtifactStore,
    arm: peritus_eval::HarnessArmBinding,
) -> Result<(HarnessRevision, ProductionHarnessBinding), EvolutionError> {
    let state = peritus_harness::load_harness_replay(journal, arm.revision().harness_id())
        .map_err(owner_error)?
        .rebuild()
        .map_err(owner_error)?
        .ok_or_else(|| incomplete("frozen arm E1 lineage is absent"))?;
    let receipt = state
        .receipts()
        .values()
        .find(|value| value.digest() == arm.receipt_digest())
        .ok_or_else(|| incomplete("frozen arm E1 receipt is absent or retired"))?;
    let resolved = resolve_materialized(
        journal,
        artifacts,
        MaterializedHarnessRef::new(arm.revision(), receipt.id()),
    )?;
    if resolved.1.harness_revision() != arm.harness_revision() {
        return Err(incomplete("frozen arm differs from exact materialized E1 revision"));
    }
    Ok(resolved)
}

/// Resolves the production pointer for each new invocation while captured old arms stay immutable.
pub(crate) fn resolve_current_binding(
    journal: &peritus_journal::SqliteJournal,
    artifacts: &peritus_artifact_store::ArtifactStore,
    project: peritus_types::ProjectId,
) -> Result<(HarnessRevision, ProductionHarnessBinding), EvolutionError> {
    let pointer = peritus_evolution::recover_pointer(journal, project)?;
    let binding =
        pointer.state().ok_or_else(|| incomplete("production pointer is absent"))?.current();
    let arm = peritus_eval::HarnessArmBinding::new(
        binding.revision(),
        binding.harness_revision(),
        binding.materialization_receipt_digest(),
    );
    resolve_materialized_arm(journal, artifacts, arm)
}

fn resolve_materialized(
    journal: &peritus_journal::SqliteJournal,
    artifacts: &peritus_artifact_store::ArtifactStore,
    reference: MaterializedHarnessRef,
) -> Result<(HarnessRevision, ProductionHarnessBinding), EvolutionError> {
    let state = peritus_harness::load_harness_replay(journal, reference.revision.harness_id())
        .map_err(owner_error)?
        .rebuild()
        .map_err(owner_error)?
        .ok_or_else(|| incomplete("materialized harness lineage does not exist"))?;
    let receipt = state
        .receipt(reference.receipt)
        .ok_or_else(|| incomplete("E1 materialization receipt is absent or retired"))?;
    let revision = state
        .history()
        .revision(receipt.revision_digest())
        .ok_or_else(|| incomplete("E1 receipt has no retained immutable revision"))?;
    let governing =
        GoverningHarnessBinding::new(reference.revision, revision, receipt).map_err(owner_error)?;
    let binding = ProductionHarnessBinding::capture(&governing)?;
    artifacts
        .verify(ArtifactDigest::from_sha256(receipt.workspace_manifest_artifact()))
        .map_err(owner_error)?;
    Ok((revision.clone(), binding))
}

impl EvolutionOperator<'_> {
    /// Admits an actual published E2 diagnosis after verifying report, artifact, and evidence.
    ///
    /// # Errors
    /// Rejects unpublished, invalidated, missing, or drifted owning-state evidence.
    pub fn submit_diagnosis(
        &mut self,
        campaign: EvolutionCampaignId,
        job: DebuggerJobId,
        report: &ValidatedReport,
        citations: Vec<DiagnosisCitation>,
        ids: EvolutionTransitionIds,
    ) -> Result<CampaignState, EvolutionError> {
        let campaign_state = self.campaign(campaign)?;
        let state = peritus_debugger::load_debugger_replay(self.journal, job)
            .map_err(owner_error)?
            .rebuild()
            .map_err(owner_error)?
            .ok_or_else(|| incomplete("diagnostic job does not exist"))?;
        let capture =
            PublishedDebuggerEvidence::capture(&state, report, citations, campaign_state.limits())?;
        let artifact = ArtifactDigest::from_sha256(capture.artifact_digest());
        if self.artifacts.read(artifact, capture.artifact_size()).map_err(owner_error)?
            != report.canonical_bytes()
        {
            return Err(incomplete("diagnostic report differs from the published artifact"));
        }
        let evidence = self.current_evidence(capture.evidence_id(), capture.revision())?;
        if evidence.payload_digest() != capture.report_digest()
            || evidence.provenance().global_position() != capture.journal_position()
            || !evidence.artifacts().contains(&artifact)
        {
            return Err(incomplete("diagnostic evidence differs from its actual publication"));
        }
        self.apply_campaign(campaign, ids, CampaignCommandKind::SubmitDiagnosis(capture))
    }

    /// Admits manifests and a variant using the exact retained E1 revisions and materializations.
    ///
    /// # Errors
    /// Rejects unmaterialized revisions, baseline/policy drift, absent diagnosis, or missing artifacts.
    pub fn admit_variant(
        &mut self,
        campaign: EvolutionCampaignId,
        manifests: &[ChangeManifest],
        candidate: MaterializedHarnessRef,
        interaction: Option<InteractionGroupId>,
        ids: EvolutionTransitionIds,
    ) -> Result<CampaignState, EvolutionError> {
        let state = self.campaign(campaign)?;
        let baseline_revision = self.bound_revision(state.baseline())?;
        let (candidate_revision, candidate_binding) = self.materialized(candidate)?;
        if !state.policy().preserved_by(&candidate_revision) {
            return Err(incomplete("candidate changes the protected frozen promotion policy"));
        }
        let variant = VariantDefinition::new(
            &baseline_revision,
            &candidate_revision,
            state.baseline(),
            candidate_binding,
            manifests,
            interaction,
            state.limits(),
        )?;
        // Validate every external dependency before committing any campaign admission.
        for manifest in manifests {
            let reconstructed = ChangeManifest::new(
                &baseline_revision,
                &candidate_revision,
                manifest.hypothesis().clone(),
                manifest.alternatives().to_vec(),
                manifest.diagnoses().to_vec(),
                manifest.deltas().to_vec(),
                manifest.predictions().to_vec(),
                manifest.falsification().clone(),
                manifest.rollback_target(),
                state.limits(),
            )?;
            if reconstructed != *manifest {
                return Err(incomplete("manifest differs from actual E1 declarations"));
            }
            for diagnosis in manifest.diagnoses() {
                self.validate_diagnosis(diagnosis)?;
                if !state.diagnoses().iter().any(|value| value == diagnosis) {
                    return Err(incomplete("manifest diagnosis was not admitted by this campaign"));
                }
            }
            for delta in manifest.deltas() {
                self.artifacts
                    .verify(ArtifactDigest::from_sha256(delta.semantic_diff_artifact()))
                    .map_err(owner_error)?;
                if let Some(digest) = delta.migration_artifact() {
                    self.artifacts
                        .verify(ArtifactDigest::from_sha256(digest))
                        .map_err(owner_error)?;
                }
            }
        }
        for manifest in manifests {
            let latest = self.campaign(campaign)?;
            if !latest.manifests().iter().any(|value| value == manifest) {
                self.apply_campaign(
                    campaign,
                    ids.child(b"manifest", manifest.digest())?,
                    CampaignCommandKind::AdmitChangeManifest(manifest.clone()),
                )?;
            }
        }
        let latest = self.campaign(campaign)?;
        if latest.variants().iter().any(|value| value == &variant) {
            return Ok(latest);
        }
        self.apply_campaign(
            campaign,
            ids.child(b"variant", variant.digest())?,
            CampaignCommandKind::AdmitVariant(variant),
        )
    }

    /// Admits an actual published E3 comparison; a ready but unpublished report cannot pass.
    ///
    /// # Errors
    /// Rejects unpublished state, arm/profile/report drift, invalidated evidence, or missing bytes.
    pub fn admit_evaluation(
        &mut self,
        campaign: EvolutionCampaignId,
        variant_id: VariantId,
        evaluation: EvaluationCampaignId,
        profile: &FrozenEvaluationProfile,
        report: &ValidatedEvaluationReport,
        ids: EvolutionTransitionIds,
    ) -> Result<CampaignState, EvolutionError> {
        let campaign_state = self.campaign(campaign)?;
        let variant = campaign_state
            .variants()
            .iter()
            .find(|value| value.id() == variant_id)
            .ok_or_else(|| incomplete("evaluated variant was not admitted"))?;
        self.bound_revision(variant.baseline())?;
        self.bound_revision(variant.candidate())?;
        let state = peritus_eval::load_evaluation_replay(self.journal, evaluation)
            .map_err(owner_error)?
            .rebuild()
            .map_err(owner_error)?
            .ok_or_else(|| incomplete("evaluation campaign does not exist"))?;
        let profile_bytes = profile.canonical_bytes().map_err(owner_error)?;
        let profile_size = u64::try_from(profile_bytes.len()).map_err(owner_error)?;
        if self.artifacts.read(state.profile_artifact(), profile_size).map_err(owner_error)?
            != profile_bytes
        {
            return Err(incomplete("evaluation profile differs from its frozen artifact"));
        }
        let capture = PublishedEvaluationEvidence::capture(
            &state,
            report,
            profile,
            variant.baseline(),
            variant.candidate(),
        )?;
        let artifact = ArtifactDigest::from_sha256(capture.report_artifact());
        let size = u64::try_from(report.bytes().len()).map_err(owner_error)?;
        if self.artifacts.read(artifact, size).map_err(owner_error)? != report.bytes() {
            return Err(incomplete("evaluation report differs from the published artifact"));
        }
        let evidence =
            self.current_evidence(capture.evidence_id(), variant.baseline().revision())?;
        if evidence.payload_digest() != capture.report_digest()
            || evidence.provenance().global_position() != capture.journal_position()
            || !evidence.artifacts().contains(&artifact)
        {
            return Err(incomplete("evaluation evidence differs from its actual publication"));
        }
        self.apply_campaign(
            campaign,
            ids,
            CampaignCommandKind::AdmitEvaluation { variant_id, evidence: capture },
        )
    }

    pub(super) fn materialized(
        &self,
        reference: MaterializedHarnessRef,
    ) -> Result<(HarnessRevision, ProductionHarnessBinding), EvolutionError> {
        resolve_materialized(self.journal, self.artifacts, reference)
    }

    pub(super) fn materialization_artifact(
        &self,
        reference: MaterializedHarnessRef,
    ) -> Result<ArtifactDigest, EvolutionError> {
        let state =
            peritus_harness::load_harness_replay(self.journal, reference.revision.harness_id())
                .map_err(owner_error)?
                .rebuild()
                .map_err(owner_error)?
                .ok_or_else(|| incomplete("materialized harness lineage does not exist"))?;
        let receipt = state
            .receipt(reference.receipt)
            .ok_or_else(|| incomplete("E1 materialization receipt is absent or retired"))?;
        Ok(ArtifactDigest::from_sha256(receipt.workspace_manifest_artifact()))
    }

    pub(super) fn bound_revision(
        &self,
        binding: ProductionHarnessBinding,
    ) -> Result<HarnessRevision, EvolutionError> {
        let state =
            peritus_harness::load_harness_replay(self.journal, binding.revision().harness_id())
                .map_err(owner_error)?
                .rebuild()
                .map_err(owner_error)?
                .ok_or_else(|| incomplete("bound E1 lineage does not exist"))?;
        let receipt = state
            .receipts()
            .values()
            .find(|receipt| receipt.digest() == binding.materialization_receipt_digest())
            .ok_or_else(|| incomplete("bound E1 receipt is absent or retired"))?;
        let (revision, actual) =
            self.materialized(MaterializedHarnessRef::new(binding.revision(), receipt.id()))?;
        if actual != binding {
            return Err(incomplete("durable E1 binding differs from F0 state"));
        }
        Ok(revision)
    }

    pub(super) fn current_evidence(
        &self,
        id: EvidenceId,
        revision: RevisionTuple,
    ) -> Result<EvidenceRecord, EvolutionError> {
        let record = self
            .evidence
            .load(id)
            .map_err(owner_error)?
            .ok_or_else(|| incomplete("published immutable evidence is absent"))?;
        if !self
            .evidence
            .freshness(id, &revision)
            .map_err(owner_error)?
            .satisfies(FreshnessRequirement::PresentAuthority)
        {
            return Err(incomplete("published immutable evidence is stale or invalidated"));
        }
        let records = self
            .journal
            .global_events_after(record.provenance().global_position().saturating_sub(1), 1)
            .map_err(owner_error)?;
        if !records.records().first().is_some_and(|event| {
            event.event_id() == record.provenance().event_id()
                && event.event_hash() == record.provenance().event_hash()
        }) {
            return Err(incomplete("evidence provenance is absent from this daemon's journal"));
        }
        Ok(record)
    }

    pub(super) fn validate_diagnosis(
        &self,
        value: &PublishedDebuggerEvidence,
    ) -> Result<(), EvolutionError> {
        let state = peritus_debugger::load_debugger_replay(self.journal, value.job_id())
            .map_err(owner_error)?
            .rebuild()
            .map_err(owner_error)?
            .ok_or_else(|| incomplete("manifest diagnostic job does not exist"))?;
        let publication = state
            .publication()
            .ok_or_else(|| incomplete("manifest diagnostic job is unpublished"))?;
        if state.phase() != peritus_debugger::DebuggerPhase::Published
            || publication.evidence_id() != value.evidence_id()
            || publication.report_id() != value.report_id()
            || publication.artifact_digest() != value.artifact_digest()
            || publication.journal_position() != value.journal_position()
        {
            return Err(incomplete("manifest diagnosis differs from published E2 owning state"));
        }
        let evidence = self.current_evidence(value.evidence_id(), value.revision())?;
        if evidence.payload_digest() != value.report_digest() {
            return Err(incomplete("manifest diagnosis differs from immutable evidence"));
        }
        self.artifacts
            .verify(ArtifactDigest::from_sha256(value.artifact_digest()))
            .map_err(owner_error)?;
        Ok(())
    }

    pub(super) fn review(
        &self,
        run: RunId,
        candidate: ProductionHarnessBinding,
    ) -> Result<PromotionReviewEvidence, EvolutionError> {
        let state = peritus_review::load_review_replay(self.journal, run)
            .map_err(owner_error)?
            .rebuild()
            .map_err(owner_error)?
            .ok_or_else(|| incomplete("independent review run does not exist"))?;
        PromotionReviewEvidence::capture(&state, candidate)
    }
}

//! Canonical evidence, variant, assessment, and proposal fixture.

use peritus_debugger::{ClaimId, DebuggerJobId, ReportId, SelectionManifestId};
use peritus_eval::{
    DatasetDigest, EvaluationCampaignId, EvaluationPlanId, EvaluationReportId, HarnessArmBinding,
    PlanDigest, ProfileDigest, ResultDigest, TaskId,
};
use peritus_journal::StoreId;
use peritus_types::{EvidenceId, ProjectId, Sha256Digest};

use crate::{
    BoundedText, ChangeManifest, CompatibilityEffect, ComponentDelta, DiagnosisCitation,
    EvaluationAnalysisSnapshot, EvaluationMetric, EvolutionLimits, MetricValue, Prediction,
    PredictionDirection, PredictionMetric, PredictionSubject, PromotionProposal,
    PublishedDebuggerEvidence, PublishedEvaluationEvidence, SelectionRecord, TaskPassAtKSnapshot,
    VariantAssessment, VariantDefinition, assess_variant, attribute, select_variant,
};

use super::{
    harness::HarnessFixture,
    identity::{digest, invalid, nominal},
};

pub(super) struct QualificationArtifacts {
    pub(super) initialization: Sha256Digest,
    pub(super) initialization_evidence: Sha256Digest,
    pub(super) baseline: Sha256Digest,
    pub(super) baseline_evidence: Sha256Digest,
    pub(super) diagnosis: Sha256Digest,
    pub(super) semantic_diff: Sha256Digest,
    pub(super) evaluation: Sha256Digest,
    pub(super) evidence_bundle: Sha256Digest,
}

pub(super) struct PromotionEvidence {
    pub(super) diagnosis: PublishedDebuggerEvidence,
    pub(super) manifest: ChangeManifest,
    pub(super) variant: VariantDefinition,
    pub(super) evaluation: PublishedEvaluationEvidence,
    pub(super) attribution: crate::AttributionRecord,
    pub(super) assessment: VariantAssessment,
    pub(super) selection: SelectionRecord,
    pub(super) proposal: PromotionProposal,
}

pub(super) fn build(
    fixture: &HarnessFixture,
    artifacts: &QualificationArtifacts,
    store: StoreId,
) -> Result<PromotionEvidence, crate::EvolutionError> {
    let limits = EvolutionLimits::compiled();
    let project = ProjectId::new(nominal(b"peritus/h1/promotion/project/v1\0", store))
        .map_err(|_| invalid("construct qualification project identity"))?;
    let campaign =
        crate::EvolutionCampaignId::new(nominal(b"peritus/h1/promotion/campaign/v1\0", store))
            .map_err(|_| invalid("construct qualification campaign identity"))?;
    let diagnosis = diagnosis(fixture, artifacts.diagnosis, store, limits)?;
    let manifest = manifest(fixture, &diagnosis, artifacts.semantic_diff, limits)?;
    let variant = VariantDefinition::new(
        &fixture.baseline_revision,
        &fixture.candidate_revision,
        fixture.baseline,
        fixture.candidate,
        core::slice::from_ref(&manifest),
        None,
        limits,
    )?;
    let evaluation = evaluation(fixture, artifacts.evaluation, store)?;
    let attribution = attribute(&variant, core::slice::from_ref(&manifest), &evaluation, limits)?;
    let assessment =
        assess_variant(&variant, &attribution, &evaluation, None, fixture.policy.policy())?;
    let selection = select_variant(fixture.policy.policy(), core::slice::from_ref(&assessment))?;
    let proposal = PromotionProposal::new(
        project,
        campaign,
        fixture.baseline,
        &variant,
        &attribution,
        &evaluation,
        None,
        &fixture.policy,
        &selection,
        artifacts.evidence_bundle,
    )?;
    Ok(PromotionEvidence {
        diagnosis,
        manifest,
        variant,
        evaluation,
        attribution,
        assessment,
        selection,
        proposal,
    })
}

fn diagnosis(
    fixture: &HarnessFixture,
    artifact: Sha256Digest,
    store: StoreId,
    limits: EvolutionLimits,
) -> Result<PublishedDebuggerEvidence, crate::EvolutionError> {
    let claim = ClaimId::new(nominal(b"peritus/h1/promotion/diagnosis-claim/v1\0", store))
        .map_err(|_| invalid("construct qualification diagnosis claim"))?;
    PublishedDebuggerEvidence::from_exact_parts(
        fixture.baseline.revision(),
        DebuggerJobId::new(nominal(b"peritus/h1/promotion/debugger-job/v1\0", store))
            .map_err(|_| invalid("construct qualification debugger job"))?,
        ReportId::new(nominal(b"peritus/h1/promotion/debugger-report/v1\0", store))
            .map_err(|_| invalid("construct qualification debugger report"))?,
        digest(b"peritus/h1/promotion/debugger-report-digest/v1\0", store),
        SelectionManifestId::new(nominal(b"peritus/h1/promotion/debugger-selection/v1\0", store))
            .map_err(|_| invalid("construct qualification debugger selection"))?,
        digest(b"peritus/h1/promotion/debugger-selection-digest/v1\0", store),
        digest(b"peritus/h1/promotion/debugger-query/v1\0", store),
        artifact,
        1,
        EvidenceId::new(nominal(b"peritus/h1/promotion/debugger-evidence/v1\0", store))
            .map_err(|_| invalid("construct qualification debugger evidence"))?,
        1,
        vec![DiagnosisCitation::Claim(claim)],
        limits,
    )
}

fn manifest(
    fixture: &HarnessFixture,
    diagnosis: &PublishedDebuggerEvidence,
    semantic_diff: Sha256Digest,
    limits: EvolutionLimits,
) -> Result<ChangeManifest, crate::EvolutionError> {
    let component = peritus_harness::domain::ComponentId::new("role.primary")
        .map_err(|_| invalid("construct qualification changed component"))?;
    let before = fixture
        .baseline_revision
        .graph()
        .declaration(&component)
        .ok_or_else(|| invalid("qualification baseline role component is absent"))?;
    let after = fixture
        .candidate_revision
        .graph()
        .declaration(&component)
        .ok_or_else(|| invalid("qualification candidate role component is absent"))?;
    let delta = ComponentDelta::capture(
        before,
        after,
        semantic_diff,
        CompatibilityEffect::Compatible,
        None,
    )?;
    let prediction = Prediction::new(
        PredictionSubject::Campaign,
        PredictionMetric::PairedEffectLower,
        PredictionDirection::AtLeast,
        MetricValue::SignedMillionths(-10_000),
        text("candidate correctness does not regress", limits)?,
        true,
        true,
    )?;
    ChangeManifest::new(
        &fixture.baseline_revision,
        &fixture.candidate_revision,
        text("replace the primary role prompt", limits)?,
        vec![text("retain the existing primary role prompt", limits)?],
        vec![diagnosis.clone()],
        vec![delta],
        vec![prediction],
        text("paired correctness falls below the frozen threshold", limits)?,
        fixture.baseline_revision.identity(),
        limits,
    )
}

fn evaluation(
    fixture: &HarnessFixture,
    report_artifact: Sha256Digest,
    store: StoreId,
) -> Result<PublishedEvaluationEvidence, crate::EvolutionError> {
    let baseline = HarnessArmBinding::new(
        fixture.baseline.revision(),
        fixture.baseline.harness_revision(),
        fixture.baseline.materialization_receipt_digest(),
    );
    let candidate = HarnessArmBinding::new(
        fixture.candidate.revision(),
        fixture.candidate.harness_revision(),
        fixture.candidate.materialization_receipt_digest(),
    );
    let analysis = EvaluationAnalysisSnapshot::from_exact_parts(
        ResultDigest::new(digest(b"peritus/h1/promotion/evaluation-result/v1\0", store)),
        EvaluationMetric::Available(1_000_000),
        EvaluationMetric::Available(vec![TaskPassAtKSnapshot::new(
            TaskId::new(nominal(b"peritus/h1/promotion/evaluation-task/v1\0", store))
                .map_err(|_| invalid("construct qualification evaluation task"))?,
            1,
            1_000_000,
        )]),
        EvaluationMetric::Available(0),
        0,
        EvaluationMetric::Available(1_000_000),
        EvaluationMetric::Available(1),
        EvaluationMetric::Available(1),
        EvaluationMetric::Available(1),
        EvaluationMetric::Available(1),
        1,
        1,
        1,
    );
    PublishedEvaluationEvidence::from_exact_parts(
        EvaluationCampaignId::new(nominal(b"peritus/h1/promotion/evaluation/v1\0", store))
            .map_err(|_| invalid("construct qualification evaluation campaign"))?,
        DatasetDigest::new(digest(b"peritus/h1/promotion/dataset/v1\0", store)),
        ProfileDigest::new(digest(b"peritus/h1/promotion/profile/v1\0", store)),
        EvaluationPlanId::new(nominal(b"peritus/h1/promotion/evaluation-plan/v1\0", store))
            .map_err(|_| invalid("construct qualification evaluation plan"))?,
        PlanDigest::new(digest(b"peritus/h1/promotion/evaluation-plan-digest/v1\0", store)),
        baseline,
        candidate,
        EvaluationReportId::new(nominal(b"peritus/h1/promotion/evaluation-report/v1\0", store))
            .map_err(|_| invalid("construct qualification evaluation report"))?,
        digest(b"peritus/h1/promotion/evaluation-report-digest/v1\0", store),
        report_artifact,
        EvidenceId::new(nominal(b"peritus/h1/promotion/evaluation-evidence/v1\0", store))
            .map_err(|_| invalid("construct qualification evaluation evidence"))?,
        1,
        analysis,
    )
}

fn text(value: &str, limits: EvolutionLimits) -> Result<BoundedText, crate::EvolutionError> {
    BoundedText::new(value.to_owned(), limits)
}

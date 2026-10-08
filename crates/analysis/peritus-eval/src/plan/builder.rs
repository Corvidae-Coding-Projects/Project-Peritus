//! Complete task/ordinal/arm matrix construction with bounded physical views.

use crate::{
    EvaluationArm, EvaluationCampaignId, EvaluationError, EvaluationErrorKind, EvaluationOperation,
    EvaluationPlanId, FrozenEvaluationProfile, PlanDigest, RolloutSeed, RolloutSpec,
};

/// Complete canonical rollout matrix plus deterministic dispatch view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationPlan {
    id: EvaluationPlanId,
    digest: PlanDigest,
    campaign_id: EvaluationCampaignId,
    specs: Vec<RolloutSpec>,
}

impl EvaluationPlan {
    /// Builds the complete paired rollout plan.
    ///
    /// # Errors
    /// Rejects identity derivation, cardinality, or canonical-binding failure.
    pub fn build(
        campaign_id: EvaluationCampaignId,
        profile: &FrozenEvaluationProfile,
    ) -> Result<Self, EvaluationError> {
        let mut specs = Vec::new();
        let selected = profile
            .dataset()
            .tasks()
            .iter()
            .filter(|task| profile.partitions().contains(&task.partition()))
            .count();
        let expected = selected
            .checked_mul(2)
            .and_then(|value| value.checked_mul(usize::from(profile.rollouts_per_task())))
            .ok_or_else(|| {
                crate::invalid(
                    EvaluationErrorKind::LimitExceeded,
                    EvaluationOperation::BuildPlan,
                    "rollout matrix cardinality overflowed",
                )
            })?;
        u32::try_from(expected).map_err(|_| {
            crate::invalid(
                EvaluationErrorKind::LimitExceeded,
                EvaluationOperation::BuildPlan,
                "rollout matrix exceeds its canonical representation",
            )
        })?;
        specs.try_reserve(expected).map_err(|_| {
            crate::invalid(
                EvaluationErrorKind::LimitExceeded,
                EvaluationOperation::BuildPlan,
                "rollout matrix allocation is unavailable",
            )
        })?;
        for task in profile
            .dataset()
            .tasks()
            .iter()
            .filter(|task| profile.partitions().contains(&task.partition()))
        {
            for ordinal in 1..=profile.rollouts_per_task() {
                let seed = RolloutSeed::derive(profile.digest(), task.id(), ordinal);
                for arm in [EvaluationArm::Baseline, EvaluationArm::Candidate] {
                    specs.push(RolloutSpec::derive(
                        campaign_id,
                        profile.digest(),
                        task.id(),
                        task.partition(),
                        arm,
                        ordinal,
                        seed,
                        task.candidate_input(),
                        task.evaluator_input().verifier_digest(),
                        profile.arm(arm).digest(),
                        profile.provider().digest(),
                        profile.model().digest(),
                        profile.execution().digest(),
                    )?);
                }
            }
        }
        if specs.is_empty() {
            return Err(crate::invalid(
                EvaluationErrorKind::LimitExceeded,
                EvaluationOperation::BuildPlan,
                "rollout matrix is empty",
            ));
        }
        let digest = PlanDigest::new(plan_digest(campaign_id, profile.digest(), &specs)?);
        let id = EvaluationPlanId::new(crate::identity::derived_identity(
            b"peritus.evaluation.plan-id.v1\0",
            &[campaign_id.as_bytes(), digest.as_bytes()],
        ))?;
        Ok(Self { id, digest, campaign_id, specs })
    }
    /// Plan identity.
    #[must_use]
    pub const fn id(&self) -> EvaluationPlanId {
        self.id
    }
    /// Complete plan digest.
    #[must_use]
    pub const fn digest(&self) -> PlanDigest {
        self.digest
    }
    /// Owning campaign.
    #[must_use]
    pub const fn campaign_id(&self) -> EvaluationCampaignId {
        self.campaign_id
    }
    /// Borrows canonical task/ordinal/arm storage order.
    #[must_use]
    pub fn specs(&self) -> &[RolloutSpec] {
        &self.specs
    }
    /// Iterates bounded physical rollout pages in canonical plan order.
    #[must_use]
    pub fn spec_pages(
        &self,
        limits: crate::EvaluationLimits,
    ) -> std::slice::Chunks<'_, RolloutSpec> {
        let page_size = usize::try_from(limits.rollouts_per_page())
            .expect("u32 rollout-page size fits every supported target");
        self.specs.chunks(page_size)
    }
    /// Returns stable blocked dispatch order without mutating canonical storage.
    #[must_use]
    pub fn dispatch_order(&self) -> Vec<&RolloutSpec> {
        let mut values: Vec<_> = self.specs.iter().collect();
        values.sort_by_key(|spec| spec.scheduling_key());
        values
    }
}

fn plan_digest(
    campaign: EvaluationCampaignId,
    profile: crate::ProfileDigest,
    specs: &[RolloutSpec],
) -> Result<peritus_types::Sha256Digest, EvaluationError> {
    let mut bytes = b"peritus.evaluation.plan.v1\0".to_vec();
    bytes.extend_from_slice(campaign.as_bytes());
    bytes.extend_from_slice(profile.as_bytes());
    let count = u32::try_from(specs.len()).map_err(|_| {
        crate::invalid(
            EvaluationErrorKind::LimitExceeded,
            EvaluationOperation::BuildPlan,
            "rollout matrix exceeds its canonical representation",
        )
    })?;
    bytes.extend_from_slice(&count.to_be_bytes());
    for spec in specs {
        bytes.extend_from_slice(spec.id().as_bytes());
        bytes.extend_from_slice(spec.work_id().as_bytes());
        bytes.extend_from_slice(spec.request_digest().as_bytes());
        bytes.extend_from_slice(spec.seed().as_bytes());
    }
    Ok(peritus_codec::sha256(&bytes))
}

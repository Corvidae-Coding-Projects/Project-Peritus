//! Derived baseline candidates that remain inert until explicit operator acceptance.

use peritus_benchmarks::{
    BaselineEntry, BaselineManifest, EvidenceManifest, NotReadyReason, QualificationError,
    QualificationEvaluation, StableId,
};

pub fn derive_candidate(
    manifest: &EvidenceManifest,
    evaluation: &QualificationEvaluation,
) -> Result<Option<BaselineManifest>, QualificationError> {
    // Measured performance can miss an SLO or differ from another baseline and still be offered
    // for explicit review. Missing execution, samples, or resource exercise cannot define one.
    let incomplete_evidence = evaluation.not_ready_reasons().iter().any(|reason| {
        !matches!(
            reason,
            NotReadyReason::ObjectiveMissed { .. }
                | NotReadyReason::RequiredBaselineMissing { .. }
                | NotReadyReason::BlockingRegression { .. }
        )
    });
    if incomplete_evidence
        || evaluation.objectives().iter().any(|objective| objective.observed().is_none())
    {
        return Ok(None);
    }
    let entries = evaluation
        .objectives()
        .iter()
        .map(|objective| {
            BaselineEntry::new(
                objective.workload_id().clone(),
                objective.metric(),
                objective.statistic(),
                objective.observed().expect("complete objective was checked above"),
                objective.sample_count(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let evidence_digest = manifest.digest()?;
    let id = StableId::new(format!("candidate.{}", &evidence_digest.as_str()[..24]))?;
    Ok(Some(BaselineManifest::new(
        id,
        evaluation.profile_id().clone(),
        manifest.subject().implementation_revision(),
        evidence_digest,
        entries,
    )?))
}

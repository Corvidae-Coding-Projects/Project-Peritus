//! Complete immutable evidence-backed change manifests.

use std::collections::BTreeSet;

use crate::{
    BoundedText, ChangeManifestId, ComponentDelta, EvolutionError, EvolutionErrorKind,
    EvolutionLimits, EvolutionOperation, EvolutionRecovery, Prediction, PublishedDebuggerEvidence,
    identity::{digest_parts, push_bytes},
};
use peritus_harness::domain::{ComponentId, HarnessRevision, HarnessRevisionIdentity};
use peritus_types::Sha256Digest;

/// One immutable diagnosis-backed exact candidate change declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeManifest {
    id: ChangeManifestId,
    baseline: HarnessRevisionIdentity,
    candidate: HarnessRevisionIdentity,
    hypothesis: BoundedText,
    alternatives: Vec<BoundedText>,
    diagnoses: Vec<PublishedDebuggerEvidence>,
    deltas: Vec<ComponentDelta>,
    predictions: Vec<Prediction>,
    falsification: BoundedText,
    rollback_target: HarnessRevisionIdentity,
    digest: Sha256Digest,
}

impl ChangeManifest {
    /// Validates exact E1 deltas, published E2 citations, predictions, and rollback binding.
    ///
    /// # Errors
    /// Rejects non-successor revisions, undeclared/protected deltas, empty evidence,
    /// noncanonical collections, over-limit inputs, or a rollback target other than the baseline.
    #[allow(
        clippy::too_many_arguments,
        reason = "every immutable manifest section remains explicit"
    )]
    pub fn new(
        baseline: &HarnessRevision,
        candidate: &HarnessRevision,
        hypothesis: BoundedText,
        alternatives: Vec<BoundedText>,
        diagnoses: Vec<PublishedDebuggerEvidence>,
        deltas: Vec<ComponentDelta>,
        predictions: Vec<Prediction>,
        falsification: BoundedText,
        rollback_target: HarnessRevisionIdentity,
        limits: EvolutionLimits,
    ) -> Result<Self, EvolutionError> {
        if !candidate.is_direct_successor_of(baseline) || rollback_target != baseline.identity() {
            return Err(binding("candidate is not a direct successor or rollback is not baseline"));
        }
        if alternatives.is_empty()
            || alternatives.windows(2).any(|pair| pair[0] >= pair[1])
            || diagnoses.is_empty()
            || diagnoses.windows(2).any(|pair| pair[0].digest() >= pair[1].digest())
            || !citation_counts_within_limit(
                diagnoses.iter().map(|value| value.citations().len()),
                limits.citations_per_manifest(),
            )
            || deltas.is_empty()
            || deltas.len() > usize::from(limits.deltas_per_manifest())
            || deltas.windows(2).any(|pair| pair[0].component_id() >= pair[1].component_id())
            || predictions.is_empty()
            || predictions.len() > usize::from(limits.predictions_per_manifest())
            || predictions.windows(2).any(|pair| pair[0].digest() >= pair[1].digest())
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::NonCanonical,
                EvolutionOperation::AdmitManifest,
                EvolutionRecovery::CorrectInput,
                "manifest collections are empty, noncanonical, or over limit",
            ));
        }
        let actual = changed_components(baseline, candidate);
        let declared = deltas.iter().map(|delta| delta.component_id().clone()).collect::<Vec<_>>();
        if declared.iter().any(|component| actual.binary_search(component).is_err()) {
            return Err(EvolutionError::new(
                EvolutionErrorKind::Contamination,
                EvolutionOperation::AdmitManifest,
                EvolutionRecovery::CorrectInput,
                "manifest invents a candidate component change",
            ));
        }
        for delta in &deltas {
            let before = baseline
                .graph()
                .declaration(delta.component_id())
                .ok_or_else(|| binding("delta baseline declaration is absent"))?;
            let after = candidate
                .graph()
                .declaration(delta.component_id())
                .ok_or_else(|| binding("delta candidate declaration is absent"))?;
            if !delta.matches(before, after) {
                return Err(binding("component delta differs from exact E1 declarations"));
            }
        }
        let digest = manifest_digest(
            baseline.identity(),
            candidate.identity(),
            &hypothesis,
            &alternatives,
            &diagnoses,
            &deltas,
            &predictions,
            &falsification,
            rollback_target,
        );
        let id = ChangeManifestId::derive(b"peritus.f0.change-manifest-id.v1\0", digest);
        Ok(Self {
            id,
            baseline: baseline.identity(),
            candidate: candidate.identity(),
            hypothesis,
            alternatives,
            diagnoses,
            deltas,
            predictions,
            falsification,
            rollback_target,
            digest,
        })
    }

    #[allow(clippy::too_many_arguments, reason = "all persisted manifest sections stay explicit")]
    pub(crate) fn from_exact_parts(
        baseline: HarnessRevisionIdentity,
        candidate: HarnessRevisionIdentity,
        hypothesis: BoundedText,
        alternatives: Vec<BoundedText>,
        diagnoses: Vec<PublishedDebuggerEvidence>,
        deltas: Vec<ComponentDelta>,
        predictions: Vec<Prediction>,
        falsification: BoundedText,
        rollback_target: HarnessRevisionIdentity,
        limits: EvolutionLimits,
    ) -> Result<Self, EvolutionError> {
        if baseline == candidate || rollback_target != baseline {
            return Err(binding("persisted manifest candidate or rollback binding is invalid"));
        }
        if alternatives.is_empty()
            || alternatives.windows(2).any(|pair| pair[0] >= pair[1])
            || diagnoses.is_empty()
            || diagnoses.windows(2).any(|pair| pair[0].digest() >= pair[1].digest())
            || !citation_counts_within_limit(
                diagnoses.iter().map(|value| value.citations().len()),
                limits.citations_per_manifest(),
            )
            || deltas.is_empty()
            || deltas.len() > usize::from(limits.deltas_per_manifest())
            || deltas.windows(2).any(|pair| pair[0].component_id() >= pair[1].component_id())
            || predictions.is_empty()
            || predictions.len() > usize::from(limits.predictions_per_manifest())
            || predictions.windows(2).any(|pair| pair[0].digest() >= pair[1].digest())
        {
            return Err(EvolutionError::new(
                EvolutionErrorKind::NonCanonical,
                EvolutionOperation::AdmitManifest,
                EvolutionRecovery::Quarantine,
                "persisted manifest collections are empty, noncanonical, or over limit",
            ));
        }
        let digest = manifest_digest(
            baseline,
            candidate,
            &hypothesis,
            &alternatives,
            &diagnoses,
            &deltas,
            &predictions,
            &falsification,
            rollback_target,
        );
        let id = ChangeManifestId::derive(b"peritus.f0.change-manifest-id.v1\0", digest);
        Ok(Self {
            id,
            baseline,
            candidate,
            hypothesis,
            alternatives,
            diagnoses,
            deltas,
            predictions,
            falsification,
            rollback_target,
            digest,
        })
    }

    /// Returns the content-derived stable identity.
    #[must_use]
    pub const fn id(&self) -> ChangeManifestId {
        self.id
    }
    /// Returns the exact E1 baseline identity.
    #[must_use]
    pub const fn baseline(&self) -> HarnessRevisionIdentity {
        self.baseline
    }
    /// Returns the exact E1 candidate identity.
    #[must_use]
    pub const fn candidate(&self) -> HarnessRevisionIdentity {
        self.candidate
    }
    /// Borrows the root-cause hypothesis.
    #[must_use]
    pub const fn hypothesis(&self) -> &BoundedText {
        &self.hypothesis
    }
    /// Borrows explicit alternative causes.
    #[must_use]
    pub fn alternatives(&self) -> &[BoundedText] {
        &self.alternatives
    }
    /// Borrows published E2 evidence.
    #[must_use]
    pub fn diagnoses(&self) -> &[PublishedDebuggerEvidence] {
        &self.diagnoses
    }
    /// Borrows complete exact E1 deltas.
    #[must_use]
    pub fn deltas(&self) -> &[ComponentDelta] {
        &self.deltas
    }
    /// Borrows falsifiable predictions.
    #[must_use]
    pub fn predictions(&self) -> &[Prediction] {
        &self.predictions
    }
    /// Borrows the explicit falsification rule.
    #[must_use]
    pub const fn falsification(&self) -> &BoundedText {
        &self.falsification
    }
    /// Returns the exact rollback target.
    #[must_use]
    pub const fn rollback_target(&self) -> HarnessRevisionIdentity {
        self.rollback_target
    }
    /// Returns the complete manifest digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

pub(crate) fn changed_components(
    baseline: &HarnessRevision,
    candidate: &HarnessRevision,
) -> Vec<ComponentId> {
    let mut ids = BTreeSet::new();
    for declaration in baseline.graph().declarations() {
        if candidate.graph().declaration(declaration.id()) != Some(declaration) {
            ids.insert(declaration.id().clone());
        }
    }
    for declaration in candidate.graph().declarations() {
        if baseline.graph().declaration(declaration.id()) != Some(declaration) {
            ids.insert(declaration.id().clone());
        }
    }
    ids.into_iter().collect()
}

fn citation_counts_within_limit(counts: impl IntoIterator<Item = usize>, maximum: u16) -> bool {
    counts
        .into_iter()
        .try_fold(0_usize, usize::checked_add)
        .is_some_and(|total| total <= usize::from(maximum))
}

#[allow(clippy::too_many_arguments)]
fn manifest_digest(
    baseline: HarnessRevisionIdentity,
    candidate: HarnessRevisionIdentity,
    hypothesis: &BoundedText,
    alternatives: &[BoundedText],
    diagnoses: &[PublishedDebuggerEvidence],
    deltas: &[ComponentDelta],
    predictions: &[Prediction],
    falsification: &BoundedText,
    rollback: HarnessRevisionIdentity,
) -> Sha256Digest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(baseline.digest().as_bytes());
    bytes.extend_from_slice(candidate.digest().as_bytes());
    push_bytes(&mut bytes, hypothesis.as_str().as_bytes());
    for value in alternatives {
        push_bytes(&mut bytes, value.as_str().as_bytes());
    }
    for value in diagnoses {
        bytes.extend_from_slice(value.digest().as_bytes());
    }
    for value in deltas {
        bytes.extend_from_slice(value.digest().as_bytes());
    }
    for value in predictions {
        bytes.extend_from_slice(value.digest().as_bytes());
    }
    push_bytes(&mut bytes, falsification.as_str().as_bytes());
    bytes.extend_from_slice(rollback.digest().as_bytes());
    digest_parts(b"peritus.f0.change-manifest.v1\0", &[&bytes])
}

const fn binding(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::BindingDrift,
        EvolutionOperation::AdmitManifest,
        EvolutionRecovery::CorrectInput,
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::citation_counts_within_limit;

    #[test]
    fn citation_limit_applies_to_the_whole_manifest() {
        assert!(citation_counts_within_limit([2, 2], 4));
        assert!(!citation_counts_within_limit([2, 2], 3));
        assert!(!citation_counts_within_limit([usize::MAX, 1], u16::MAX));
    }
}

//! Fail-closed final H4 report and policy handoff.

mod validation;

use serde::Serialize;

use peritus_release_artifacts::{
    ArtifactInventory, ReleaseBinding, ReproducibilityComparison, Sha256Digest, digest_bytes,
};

use self::validation::{
    PolicyDigests, available_references, collect_records, evaluate_policy,
    validate_artifact_inventory, validate_audit, validate_criterion_map, validate_manifest,
    validate_reproducibility, validate_required_records,
};
use crate::{
    AcceptanceCriterion, CollectionRun, CriterionEvidenceMap, DeterministicReleasePolicy,
    EvidenceKind, EvidenceManifest, FinalAudit, PolicyDecision, QualificationError,
    SignedEvidenceRecord,
};

/// Top-level H4 input whose absence blocks policy evaluation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RequiredInput {
    /// Fresh-subject campaign run.
    CollectionRun,
    /// Canonical artifact inventory.
    ArtifactInventory,
    /// Independent-builder comparison.
    ReproducibilityComparison,
    /// Complete AC-01 through AC-25 map.
    CriterionEvidenceMap,
    /// Content-addressed evidence manifest.
    EvidenceManifest,
    /// Signature-verified independent final audit.
    FinalAudit,
}

/// Deterministic reason final H4 readiness was withheld.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Blocker {
    /// A top-level structured input was absent.
    MissingInput(RequiredInput),
    /// A required signed evidence kind was absent.
    MissingSignedEvidence(EvidenceKind),
    /// A signed evidence kind appeared more than once.
    DuplicateSignedEvidence(EvidenceKind),
    /// A required signed producer reported failure or incompleteness.
    UnsatisfiedSignedEvidence(EvidenceKind),
    /// Signed or structured evidence bound another candidate.
    BindingMismatch,
    /// One or more fresh-subject campaigns or cleanups failed.
    CollectionIncomplete,
    /// Signed artifact-inventory bytes did not match the supplied inventory.
    ArtifactInventoryDigestMismatch,
    /// Independent builders did not emit identical artifact paths and bytes.
    ArtifactsNotReproducible,
    /// Signed reproducibility bytes did not match the supplied comparison.
    ReproducibilityDigestMismatch,
    /// A criterion referenced evidence not admitted into this qualification.
    CriterionEvidenceUnavailable(AcReference),
    /// Signed criterion-map bytes did not match the supplied complete map.
    CriterionMapDigestMismatch,
    /// The evidence manifest omitted a required role.
    ManifestIncomplete,
    /// The manifest did not retain an admitted evidence reference.
    ManifestReferenceMissing(EvidenceKind),
    /// The final auditor was also a declared contributor.
    AuditNotIndependent,
    /// The auditor reviewed a different pre-audit evidence set.
    AuditSubjectMismatch,
    /// At least one high or critical audit finding was not actually closed.
    AuditBlockingFindingOpen,
    /// Deterministic release policy rejected the candidate.
    PolicyRejected,
    /// Deterministic release policy could not evaluate the candidate.
    PolicyUnavailable,
}

/// Compact criterion identity retained in a blocker.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AcReference {
    number: u8,
}

impl AcReference {
    pub(super) const fn from_criterion(criterion: AcceptanceCriterion) -> Self {
        Self { number: criterion.number() }
    }

    /// Returns the one-based acceptance criterion number.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.number
    }
}

/// Optional observations supplied to final H4 reduction.
#[derive(Clone, Debug)]
pub struct QualificationInputs {
    pub(super) binding: ReleaseBinding,
    pub(super) evidence: Vec<SignedEvidenceRecord>,
    pub(super) collection_run: Option<CollectionRun>,
    pub(super) artifact_inventory: Option<ArtifactInventory>,
    pub(super) reproducibility: Option<ReproducibilityComparison>,
    pub(super) criterion_map: Option<CriterionEvidenceMap>,
    pub(super) evidence_manifest: Option<EvidenceManifest>,
    pub(super) final_audit: Option<FinalAudit>,
}

impl QualificationInputs {
    /// Starts a fail-closed input set for one candidate.
    #[must_use]
    pub const fn new(binding: ReleaseBinding) -> Self {
        Self {
            binding,
            evidence: Vec::new(),
            collection_run: None,
            artifact_inventory: None,
            reproducibility: None,
            criterion_map: None,
            evidence_manifest: None,
            final_audit: None,
        }
    }

    /// Adds a signature-verified non-campaign evidence record.
    #[must_use]
    pub fn evidence(mut self, record: SignedEvidenceRecord) -> Self {
        self.evidence.push(record);
        self
    }

    /// Attaches the fresh-subject campaign run.
    #[must_use]
    pub fn collection_run(mut self, run: CollectionRun) -> Self {
        self.collection_run = Some(run);
        self
    }

    /// Attaches the canonical artifact inventory.
    #[must_use]
    pub fn artifact_inventory(mut self, inventory: ArtifactInventory) -> Self {
        self.artifact_inventory = Some(inventory);
        self
    }

    /// Attaches the independent-builder comparison.
    #[must_use]
    pub fn reproducibility(mut self, comparison: ReproducibilityComparison) -> Self {
        self.reproducibility = Some(comparison);
        self
    }

    /// Attaches the complete acceptance-criterion evidence map.
    #[must_use]
    pub fn criterion_map(mut self, map: CriterionEvidenceMap) -> Self {
        self.criterion_map = Some(map);
        self
    }

    /// Attaches the content-addressed evidence manifest.
    #[must_use]
    pub fn evidence_manifest(mut self, manifest: EvidenceManifest) -> Self {
        self.evidence_manifest = Some(manifest);
        self
    }

    /// Attaches the signature-verified independent final audit.
    #[must_use]
    pub fn final_audit(mut self, audit: FinalAudit) -> Self {
        self.final_audit = Some(audit);
        self
    }
}

/// Final H4 disposition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum QualificationVerdict {
    /// Every H4 check and the authoritative release policy accepted the exact candidate.
    Ready,
    /// Readiness was withheld; blockers enumerate why.
    NotReady,
}

/// Complete deterministic final H4 report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct QualificationReport {
    schema_version: u32,
    binding: ReleaseBinding,
    artifact_inventory_digest: Option<Sha256Digest>,
    evidence_manifest_digest: Option<Sha256Digest>,
    criterion_map_digest: Option<Sha256Digest>,
    final_audit_digest: Option<Sha256Digest>,
    blockers: Vec<Blocker>,
    policy_decision: Option<PolicyDecision>,
    verdict: QualificationVerdict,
}

impl QualificationReport {
    /// Reduces supplied observations and consults policy only after all H4 checks pass.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] only when deterministic content addressing fails. Missing or
    /// contradictory evidence is retained as a not-ready blocker.
    pub fn evaluate<P: DeterministicReleasePolicy>(
        inputs: &QualificationInputs,
        policy: &P,
    ) -> Result<Self, QualificationError> {
        let mut blockers = Vec::new();
        let records = collect_records(inputs, &mut blockers);
        validate_required_records(inputs, &records, &mut blockers);
        let artifact_inventory_digest =
            validate_artifact_inventory(inputs, &records, &mut blockers)?;
        validate_reproducibility(inputs, &records, &mut blockers)?;
        let available = available_references(inputs, &records);
        let criterion_map_digest = validate_criterion_map(inputs, &available, &mut blockers)?;
        let evidence_manifest_digest = validate_manifest(inputs, &available, &mut blockers)?;
        let final_audit_digest = validate_audit(inputs, &mut blockers)?;
        let policy_decision = evaluate_policy(
            inputs,
            policy,
            PolicyDigests {
                artifact_inventory: artifact_inventory_digest,
                evidence_manifest: evidence_manifest_digest,
                criterion_map: criterion_map_digest,
                final_audit: final_audit_digest,
            },
            &mut blockers,
        );
        let verdict =
            if blockers.is_empty() && matches!(policy_decision, Some(PolicyDecision::Ready)) {
                QualificationVerdict::Ready
            } else {
                QualificationVerdict::NotReady
            };
        Ok(Self {
            schema_version: 1,
            binding: inputs.binding.clone(),
            artifact_inventory_digest,
            evidence_manifest_digest,
            criterion_map_digest,
            final_audit_digest,
            blockers,
            policy_decision,
            verdict,
        })
    }

    /// Returns the final fail-closed verdict.
    #[must_use]
    pub const fn verdict(&self) -> QualificationVerdict {
        self.verdict
    }

    /// Returns blockers in deterministic validation order.
    #[must_use]
    pub fn blockers(&self) -> &[Blocker] {
        &self.blockers
    }

    /// Returns the policy decision when complete inputs permitted evaluation.
    #[must_use]
    pub const fn policy_decision(&self) -> Option<&PolicyDecision> {
        self.policy_decision.as_ref()
    }

    /// Serializes deterministic compact final-report JSON.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] if serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, QualificationError> {
        serde_json::to_vec(self).map_err(|source| {
            QualificationError::serialization("serialize final H4 report", source)
        })
    }

    /// Returns the content identity of canonical final-report JSON.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] if serialization fails.
    pub fn digest(&self) -> Result<Sha256Digest, QualificationError> {
        self.canonical_json().map(|bytes| digest_bytes(&bytes))
    }
}

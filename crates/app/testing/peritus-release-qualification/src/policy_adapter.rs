//! Narrow adapter into the separately owned deterministic release policy.

use serde::Serialize;

use peritus_release_artifacts::{ReleaseBinding, Sha256Digest};

use crate::{AcceptanceCriterion, EvidenceReference, QualificationError, QualificationErrorCode};

/// One criterion and its authenticated evidence presented to release policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PolicyCriterionInput {
    criterion: AcceptanceCriterion,
    evidence: Vec<EvidenceReference>,
}

impl PolicyCriterionInput {
    pub(crate) const fn new(
        criterion: AcceptanceCriterion,
        evidence: Vec<EvidenceReference>,
    ) -> Self {
        Self { criterion, evidence }
    }

    /// Returns the acceptance criterion.
    #[must_use]
    pub const fn criterion(&self) -> AcceptanceCriterion {
        self.criterion
    }

    /// Returns authenticated evidence references.
    #[must_use]
    pub fn evidence(&self) -> &[EvidenceReference] {
        &self.evidence
    }
}

/// Complete lossless H4 input for deterministic release policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReleasePolicyInput {
    binding: ReleaseBinding,
    artifact_inventory_digest: Sha256Digest,
    evidence_manifest_digest: Sha256Digest,
    criterion_map_digest: Sha256Digest,
    final_audit_digest: Sha256Digest,
    criteria: Vec<PolicyCriterionInput>,
}

impl ReleasePolicyInput {
    pub(crate) const fn new(
        binding: ReleaseBinding,
        artifact_inventory_digest: Sha256Digest,
        evidence_manifest_digest: Sha256Digest,
        criterion_map_digest: Sha256Digest,
        final_audit_digest: Sha256Digest,
        criteria: Vec<PolicyCriterionInput>,
    ) -> Self {
        Self {
            binding,
            artifact_inventory_digest,
            evidence_manifest_digest,
            criterion_map_digest,
            final_audit_digest,
            criteria,
        }
    }

    /// Returns the exact candidate binding.
    #[must_use]
    pub const fn binding(&self) -> &ReleaseBinding {
        &self.binding
    }

    /// Returns the canonical artifact inventory digest.
    #[must_use]
    pub const fn artifact_inventory_digest(&self) -> Sha256Digest {
        self.artifact_inventory_digest
    }

    /// Returns the final content-addressed evidence manifest digest.
    #[must_use]
    pub const fn evidence_manifest_digest(&self) -> Sha256Digest {
        self.evidence_manifest_digest
    }

    /// Returns the complete AC-01 through AC-25 map digest.
    #[must_use]
    pub const fn criterion_map_digest(&self) -> Sha256Digest {
        self.criterion_map_digest
    }

    /// Returns the signature-verified independent final-audit digest.
    #[must_use]
    pub const fn final_audit_digest(&self) -> Sha256Digest {
        self.final_audit_digest
    }

    /// Returns exact AC-01 through AC-25 policy inputs.
    #[must_use]
    pub fn criteria(&self) -> &[PolicyCriterionInput] {
        &self.criteria
    }
}

/// Bounded stable reason supplied by a deterministic policy adapter.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PolicyFailure(String);

impl PolicyFailure {
    /// Creates a stable policy reason code.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when it is empty, oversized, or outside the lowercase dotted
    /// identifier grammar.
    pub fn new(value: impl Into<String>) -> Result<Self, QualificationError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'-' | b'_')
            })
        {
            return Err(QualificationError::new(
                QualificationErrorCode::InvalidValue,
                "validate release policy failure code",
                "policy failure violates the 1 through 128 byte lowercase identifier grammar",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the stable reason code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn known(value: &'static str) -> Self {
        Self(value.to_owned())
    }
}

/// Decision returned by the authoritative deterministic release policy adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyDecision {
    /// Policy accepted every exact input.
    Ready,
    /// Policy rejected one or more rules.
    NotReady {
        /// Stable policy reason codes.
        failures: Vec<PolicyFailure>,
    },
    /// The policy could not evaluate the input and no readiness claim exists.
    Unavailable {
        /// Stable reason for inability to evaluate.
        failure: PolicyFailure,
    },
}

/// Adapter implemented by the separately owned deterministic H4 policy crate.
pub trait DeterministicReleasePolicy {
    /// Evaluates a complete release input without performing effects.
    fn evaluate(&self, input: &ReleasePolicyInput) -> PolicyDecision;
}

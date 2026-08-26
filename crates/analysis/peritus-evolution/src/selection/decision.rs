//! Immutable selected/no-eligible decision records.

use crate::{Criterion, SelectionId, VariantId, identity::digest_parts};
use peritus_types::Sha256Digest;

/// Exact deny-wins reasons for one ineligible variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariantRejection {
    variant_id: VariantId,
    failed: Vec<Criterion>,
    unavailable: Vec<Criterion>,
}

impl VariantRejection {
    pub(crate) const fn new(
        variant_id: VariantId,
        failed: Vec<Criterion>,
        unavailable: Vec<Criterion>,
    ) -> Self {
        Self { variant_id, failed, unavailable }
    }
    /// Returns the rejected variant.
    #[must_use]
    pub const fn variant_id(&self) -> VariantId {
        self.variant_id
    }
    /// Borrows mandatory criteria contradicted by available evidence.
    #[must_use]
    pub fn failed(&self) -> &[Criterion] {
        &self.failed
    }
    /// Borrows mandatory criteria lacking usable evidence.
    #[must_use]
    pub fn unavailable(&self) -> &[Criterion] {
        &self.unavailable
    }
}

/// Stable deterministic selection outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionDecision {
    /// One eligible variant won the frozen objective order.
    Selected(VariantId),
    /// No variant passed every mandatory criterion.
    NoEligibleVariant(Vec<VariantRejection>),
}

/// Complete immutable policy-bound selection result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionRecord {
    id: SelectionId,
    policy_digest: Sha256Digest,
    assessment_digests: Vec<Sha256Digest>,
    decision: SelectionDecision,
    digest: Sha256Digest,
}

impl SelectionRecord {
    pub(crate) fn from_exact_parts(
        policy_digest: Sha256Digest,
        assessment_digests: Vec<Sha256Digest>,
        decision: SelectionDecision,
    ) -> Self {
        let digest = selection_digest(policy_digest, &assessment_digests, &decision);
        let id = SelectionId::derive(b"peritus.f0.selection-id.v1\0", digest);
        Self { id, policy_digest, assessment_digests, decision, digest }
    }
    /// Returns the deterministic decision identity.
    #[must_use]
    pub const fn id(&self) -> SelectionId {
        self.id
    }
    /// Returns the frozen policy digest.
    #[must_use]
    pub const fn policy_digest(&self) -> Sha256Digest {
        self.policy_digest
    }
    /// Borrows canonical assessed variant digests.
    #[must_use]
    pub fn assessment_digests(&self) -> &[Sha256Digest] {
        &self.assessment_digests
    }
    /// Borrows the exact selected/no-eligible outcome.
    #[must_use]
    pub const fn decision(&self) -> &SelectionDecision {
        &self.decision
    }
    /// Returns the complete selection digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn selection_digest(
    policy: Sha256Digest,
    assessments: &[Sha256Digest],
    decision: &SelectionDecision,
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(assessments.len() * 32 + 64);
    bytes.push(1);
    push_count(&mut bytes, assessments.len());
    for digest in assessments {
        bytes.extend_from_slice(digest.as_bytes());
    }
    match decision {
        SelectionDecision::Selected(id) => {
            bytes.push(2);
            bytes.extend_from_slice(id.as_bytes());
        }
        SelectionDecision::NoEligibleVariant(rejections) => {
            bytes.push(3);
            push_count(&mut bytes, rejections.len());
            for rejection in rejections {
                bytes.extend_from_slice(rejection.variant_id().as_bytes());
                push_criteria_section(&mut bytes, 1, rejection.failed());
                push_criteria_section(&mut bytes, 2, rejection.unavailable());
            }
        }
    }
    digest_parts(b"peritus.f0.selection.v1\0", &[policy.as_bytes(), &bytes])
}

fn push_criteria_section(bytes: &mut Vec<u8>, tag: u8, criteria: &[Criterion]) {
    bytes.push(tag);
    push_count(bytes, criteria.len());
    bytes.extend(criteria.iter().map(|value| value.tag()));
}

fn push_count(bytes: &mut Vec<u8>, count: usize) {
    bytes.extend_from_slice(&u64::try_from(count).unwrap_or(u64::MAX).to_be_bytes());
}

//! Exact integrated-candidate and revision binding.

use peritus_types::{RevisionTuple, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Exact immutable subject of one H0 qualification campaign.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IntegratedCandidate {
    revision: RevisionTuple,
    source_digest: Sha256Digest,
    release_manifest_digest: Sha256Digest,
    qualification_plan_digest: Sha256Digest,
}

impl IntegratedCandidate {
    /// Creates an exact candidate binding from already validated identities.
    #[must_use]
    pub const fn new(
        revision: RevisionTuple,
        source_digest: Sha256Digest,
        release_manifest_digest: Sha256Digest,
        qualification_plan_digest: Sha256Digest,
    ) -> Self {
        Self { revision, source_digest, release_manifest_digest, qualification_plan_digest }
    }

    /// Returns the complete authority and workspace revision tuple.
    #[must_use]
    pub const fn revision(self) -> (result: RevisionTuple)
        ensures result == self.spec_revision(),
    {
        self.revision
    }

    /// Returns the digest of the qualified source tree.
    #[must_use]
    pub const fn source_digest(self) -> (result: Sha256Digest)
        ensures result == self.spec_source_digest(),
    {
        self.source_digest
    }

    /// Returns the digest of the exact integrated release-artifact manifest.
    #[must_use]
    pub const fn release_manifest_digest(self) -> (result: Sha256Digest)
        ensures result == self.spec_release_manifest_digest(),
    {
        self.release_manifest_digest
    }

    /// Returns the digest of the immutable H0 plan and probe catalog.
    #[must_use]
    pub const fn qualification_plan_digest(self) -> (result: Sha256Digest)
        ensures result == self.spec_qualification_plan_digest(),
    {
        self.qualification_plan_digest
    }

    /// Specification view of the complete revision tuple.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }

    /// Specification view of the source digest.
    pub closed spec fn spec_source_digest(&self) -> Sha256Digest { self.source_digest }

    /// Specification view of the release-manifest digest.
    pub closed spec fn spec_release_manifest_digest(&self) -> Sha256Digest {
        self.release_manifest_digest
    }

    /// Specification view of the qualification-plan digest.
    pub closed spec fn spec_qualification_plan_digest(&self) -> Sha256Digest {
        self.qualification_plan_digest
    }
}

pub open spec fn same_bytes_16_from(left: [u8; 16], right: [u8; 16], index: nat) -> bool
    decreases 16 - index,
{
    if index >= 16 {
        true
    } else {
        left[index as int] == right[index as int]
            && same_bytes_16_from(left, right, index + 1)
    }
}

pub open spec fn same_bytes_16(left: [u8; 16], right: [u8; 16]) -> bool {
    same_bytes_16_from(left, right, 0)
}

pub open spec fn same_bytes_32_from(left: [u8; 32], right: [u8; 32], index: nat) -> bool
    decreases 32 - index,
{
    if index >= 32 {
        true
    } else {
        left[index as int] == right[index as int]
            && same_bytes_32_from(left, right, index + 1)
    }
}

pub open spec fn same_bytes_32(left: [u8; 32], right: [u8; 32]) -> bool {
    same_bytes_32_from(left, right, 0)
}

const fn bytes_16_equal_from(
    left: [u8; 16],
    right: [u8; 16],
    index: usize,
) -> (equal: bool)
    requires index <= 16,
    ensures equal == same_bytes_16_from(left, right, index as nat),
    decreases 16 - index,
{
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes_16_equal_from(left, right, index + 1)
    }
}

const fn bytes_32_equal_from(
    left: [u8; 32],
    right: [u8; 32],
    index: usize,
) -> (equal: bool)
    requires index <= 32,
    ensures equal == same_bytes_32_from(left, right, index as nat),
    decreases 32 - index,
{
    if index == 32 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes_32_equal_from(left, right, index + 1)
    }
}

const fn bytes_16_equal(left: [u8; 16], right: [u8; 16]) -> (equal: bool)
    ensures equal == same_bytes_16(left, right),
{
    bytes_16_equal_from(left, right, 0)
}

const fn bytes_32_equal(left: [u8; 32], right: [u8; 32]) -> (equal: bool)
    ensures equal == same_bytes_32(left, right),
{
    bytes_32_equal_from(left, right, 0)
}

/// Exact mathematical equality of every candidate and revision component.
pub open spec fn candidate_fresh(
    observed: IntegratedCandidate,
    requested: IntegratedCandidate,
) -> bool {
    let left = observed.spec_revision();
    let right = requested.spec_revision();
    same_bytes_16(
        left.spec_acceptance_spec_id().spec_bytes(),
        right.spec_acceptance_spec_id().spec_bytes(),
    )
        && same_bytes_16(
            left.spec_harness_id().spec_bytes(),
            right.spec_harness_id().spec_bytes(),
        )
        && same_bytes_16(
            left.spec_workspace_id().spec_bytes(),
            right.spec_workspace_id().spec_bytes(),
        )
        && left.spec_workspace_generation().spec_value()
            == right.spec_workspace_generation().spec_value()
        && left.spec_workspace_revision().spec_value()
            == right.spec_workspace_revision().spec_value()
        && same_bytes_16(
            left.spec_policy_id().spec_bytes(),
            right.spec_policy_id().spec_bytes(),
        )
        && same_bytes_16(
            left.spec_provider_profile_id().spec_bytes(),
            right.spec_provider_profile_id().spec_bytes(),
        )
        && same_bytes_32(
            observed.spec_source_digest().spec_bytes(),
            requested.spec_source_digest().spec_bytes(),
        )
        && same_bytes_32(
            observed.spec_release_manifest_digest().spec_bytes(),
            requested.spec_release_manifest_digest().spec_bytes(),
        )
        && same_bytes_32(
            observed.spec_qualification_plan_digest().spec_bytes(),
            requested.spec_qualification_plan_digest().spec_bytes(),
        )
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "verified evaluator modules require the executable candidate refinement"
)]
pub(crate) const fn candidate_matches(
    observed: IntegratedCandidate,
    requested: IntegratedCandidate,
) -> (matches: bool)
    ensures matches == candidate_fresh(observed, requested),
{
    let left = observed.revision();
    let right = requested.revision();
    let matches = bytes_16_equal(
        *left.acceptance_spec_id().as_bytes(),
        *right.acceptance_spec_id().as_bytes(),
    )
        && bytes_16_equal(*left.harness_id().as_bytes(), *right.harness_id().as_bytes())
        && bytes_16_equal(*left.workspace_id().as_bytes(), *right.workspace_id().as_bytes())
        && left.workspace_generation().get() == right.workspace_generation().get()
        && left.workspace_revision().get() == right.workspace_revision().get()
        && bytes_16_equal(*left.policy_id().as_bytes(), *right.policy_id().as_bytes())
        && bytes_16_equal(
            *left.provider_profile_id().as_bytes(),
            *right.provider_profile_id().as_bytes(),
        )
        && bytes_32_equal(
            *observed.source_digest().as_bytes(),
            *requested.source_digest().as_bytes(),
        )
        && bytes_32_equal(
            *observed.release_manifest_digest().as_bytes(),
            *requested.release_manifest_digest().as_bytes(),
        )
        && bytes_32_equal(
            *observed.qualification_plan_digest().as_bytes(),
            *requested.qualification_plan_digest().as_bytes(),
        );
    proof {
        reveal_with_fuel(same_bytes_16_from, 17);
        reveal_with_fuel(same_bytes_32_from, 33);
    }
    matches
}

/// Reports whether a digest is not the reserved all-zero missing-evidence value.
#[allow(
    clippy::redundant_pub_crate,
    reason = "verified sibling evaluator modules require the executable digest predicate"
)]
pub(crate) const fn digest_present(digest: Sha256Digest) -> bool {
    let bytes = digest.as_bytes();
    let mut index = 0;
    while index < bytes.len()
        invariant 0 <= index <= bytes.len(),
        decreases bytes.len() - index,
    {
        if bytes[index] != 0 { return true; }
        index += 1;
    }
    false
}

} // verus!

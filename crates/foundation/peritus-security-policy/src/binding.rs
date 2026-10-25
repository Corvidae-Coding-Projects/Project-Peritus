//! Exact integrated-candidate and revision binding.

use peritus_types::{ActorId, RevisionTuple, Sha256Digest};

#[doc = " Exact immutable subject of one H0 qualification campaign."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IntegratedCandidate {
    revision: RevisionTuple,
    source_digest: Sha256Digest,
    release_manifest_digest: Sha256Digest,
    qualification_plan_digest: Sha256Digest,
}
impl IntegratedCandidate {
    #[doc = " Creates an exact candidate binding from already validated identities."]
    #[must_use]
    pub const fn new(
        revision: RevisionTuple,
        source_digest: Sha256Digest,
        release_manifest_digest: Sha256Digest,
        qualification_plan_digest: Sha256Digest,
    ) -> Self {
        Self { revision, source_digest, release_manifest_digest, qualification_plan_digest }
    }
    #[doc = " Returns the complete authority and workspace revision tuple."]
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the digest of the qualified source tree."]
    #[must_use]
    pub const fn source_digest(self) -> Sha256Digest {
        self.source_digest
    }
    #[doc = " Returns the digest of the exact integrated release-artifact manifest."]
    #[must_use]
    pub const fn release_manifest_digest(self) -> Sha256Digest {
        self.release_manifest_digest
    }
    #[doc = " Returns the digest of the immutable H0 plan and probe catalog."]
    #[must_use]
    pub const fn qualification_plan_digest(self) -> Sha256Digest {
        self.qualification_plan_digest
    }
}
const fn bytes_16_equal_from(left: [u8; 16], right: [u8; 16], index: usize) -> bool {
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes_16_equal_from(left, right, index + 1)
    }
}
const fn bytes_32_equal_from(left: [u8; 32], right: [u8; 32], index: usize) -> bool {
    if index == 32 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes_32_equal_from(left, right, index + 1)
    }
}
const fn bytes_16_equal(left: [u8; 16], right: [u8; 16]) -> bool {
    bytes_16_equal_from(left, right, 0)
}
const fn bytes_32_equal(left: [u8; 32], right: [u8; 32]) -> bool {
    bytes_32_equal_from(left, right, 0)
}
#[doc = " Executable equality of actor identities by all stable bytes."]
pub const fn actor_ids_equal(left: ActorId, right: ActorId) -> bool {
    bytes_16_equal(*left.as_bytes(), *right.as_bytes())
}
#[doc = " Executable equality of SHA-256 digests by all bytes."]
pub const fn digests_equal(left: Sha256Digest, right: Sha256Digest) -> bool {
    bytes_32_equal(*left.as_bytes(), *right.as_bytes())
}
#[allow(
    clippy::redundant_pub_crate,
    reason = "verified evaluator modules require the executable candidate refinement"
)]
pub(crate) const fn candidate_matches(
    observed: IntegratedCandidate,
    requested: IntegratedCandidate,
) -> bool {
    let left = observed.revision();
    let right = requested.revision();

    bytes_16_equal(*left.acceptance_spec_id().as_bytes(), *right.acceptance_spec_id().as_bytes())
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
        )
}
const fn digest_bytes_present_exec(bytes: [u8; 32], index: usize) -> bool {
    if index == 32 {
        false
    } else if bytes[index] != 0 {
        true
    } else {
        digest_bytes_present_exec(bytes, index + 1)
    }
}
#[allow(
    clippy::redundant_pub_crate,
    reason = "verified sibling evaluator modules require the executable digest predicate"
)]
pub(crate) const fn digest_present(digest: Sha256Digest) -> bool {
    digest_bytes_present_exec(*digest.as_bytes(), 0)
}

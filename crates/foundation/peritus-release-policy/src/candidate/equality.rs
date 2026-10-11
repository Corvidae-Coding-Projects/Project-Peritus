//! Verified executable correspondence for complete release-candidate identity.

use super::{
    Architecture, GitCommitId, OperatingSystem, PlatformIdentity, PlatformMatrix, ProfileIdentity,
    ReleaseCandidate, ReleaseVersion, SchemaIdentity, ToolchainIdentity,
};

const fn bytes_16_equal_from(left: [u8; 16], right: [u8; 16], index: usize) -> bool {
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes_16_equal_from(left, right, index + 1)
    }
}
const fn bytes_20_equal_from(left: [u8; 20], right: [u8; 20], index: usize) -> bool {
    if index == 20 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes_20_equal_from(left, right, index + 1)
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
#[doc = " Executable equality for exact sixteen-byte identities."]
pub const fn bytes_16_equal(left: [u8; 16], right: [u8; 16]) -> bool {
    bytes_16_equal_from(left, right, 0)
}
const fn bytes_20_equal(left: [u8; 20], right: [u8; 20]) -> bool {
    bytes_20_equal_from(left, right, 0)
}
const fn bytes_32_equal(left: [u8; 32], right: [u8; 32]) -> bool {
    bytes_32_equal_from(left, right, 0)
}
#[doc = " Executable equality for exact SHA-256 digests."]
pub const fn digests_equal(
    left: peritus_types::Sha256Digest,
    right: peritus_types::Sha256Digest,
) -> bool {
    bytes_32_equal(*left.as_bytes(), *right.as_bytes())
}
const fn git_commits_equal(left: GitCommitId, right: GitCommitId) -> bool {
    match (left.sha1_bytes(), right.sha1_bytes()) {
        (Some(left_bytes), Some(right_bytes)) => bytes_20_equal(left_bytes, right_bytes),
        (None, None) => match (left.sha256_bytes(), right.sha256_bytes()) {
            (Some(left_bytes), Some(right_bytes)) => bytes_32_equal(left_bytes, right_bytes),
            _ => false,
        },
        _ => false,
    }
}
const fn operating_systems_equal(left: OperatingSystem, right: OperatingSystem) -> bool {
    matches!(
        (left, right),
        (OperatingSystem::Linux, OperatingSystem::Linux)
            | (OperatingSystem::MacOs, OperatingSystem::MacOs)
            | (OperatingSystem::Windows, OperatingSystem::Windows)
    )
}
const fn architectures_equal(left: Architecture, right: Architecture) -> bool {
    matches!(
        (left, right),
        (Architecture::X86_64, Architecture::X86_64)
            | (Architecture::Aarch64, Architecture::Aarch64)
    )
}
const fn platforms_equal(left: PlatformIdentity, right: PlatformIdentity) -> bool {
    operating_systems_equal(left.operating_system(), right.operating_system())
        && architectures_equal(left.architecture(), right.architecture())
        && bytes_32_equal(*left.profile_digest().as_bytes(), *right.profile_digest().as_bytes())
}
const fn platform_matrices_equal(left: PlatformMatrix, right: PlatformMatrix) -> bool {
    platforms_equal(left.linux(), right.linux())
        && platforms_equal(left.macos(), right.macos())
        && platforms_equal(left.windows(), right.windows())
}
const fn versions_equal(left: ReleaseVersion, right: ReleaseVersion) -> bool {
    left.major() == right.major()
        && left.minor() == right.minor()
        && left.patch() == right.patch()
        && bytes_32_equal(
            *left.descriptor_digest().as_bytes(),
            *right.descriptor_digest().as_bytes(),
        )
}
const fn toolchains_equal(left: ToolchainIdentity, right: ToolchainIdentity) -> bool {
    bytes_32_equal(*left.rust_digest().as_bytes(), *right.rust_digest().as_bytes())
        && bytes_32_equal(*left.verus_digest().as_bytes(), *right.verus_digest().as_bytes())
        && bytes_32_equal(*left.vstd_digest().as_bytes(), *right.vstd_digest().as_bytes())
        && bytes_32_equal(*left.solver_digest().as_bytes(), *right.solver_digest().as_bytes())
}
const fn profiles_equal(left: ProfileIdentity, right: ProfileIdentity) -> bool {
    left.revision() == right.revision()
        && bytes_32_equal(*left.digest().as_bytes(), *right.digest().as_bytes())
}
const fn schemas_equal(left: SchemaIdentity, right: SchemaIdentity) -> bool {
    left.policy() == right.policy()
        && left.evidence() == right.evidence()
        && left.report() == right.report()
        && left.artifact() == right.artifact()
        && bytes_32_equal(*left.catalog_digest().as_bytes(), *right.catalog_digest().as_bytes())
}
#[doc = " Executable refinement of complete release-candidate identity equality."]
pub const fn candidate_matches(left: &ReleaseCandidate, right: &ReleaseCandidate) -> bool {
    bytes_16_equal(*left.id().as_bytes(), *right.id().as_bytes())
        && git_commits_equal(left.commit(), right.commit())
        && versions_equal(left.version(), right.version())
        && platform_matrices_equal(left.platforms(), right.platforms())
        && toolchains_equal(left.toolchain(), right.toolchain())
        && profiles_equal(left.profile(), right.profile())
        && schemas_equal(left.schemas(), right.schemas())
        && left.source_revision() == right.source_revision()
        && bytes_32_equal(*left.manifest_digest().as_bytes(), *right.manifest_digest().as_bytes())
}

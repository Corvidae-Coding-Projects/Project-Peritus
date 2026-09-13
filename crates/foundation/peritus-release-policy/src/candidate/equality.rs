//! Verified executable correspondence for complete release-candidate identity.

use super::{
    Architecture, GitCommitId, OperatingSystem, PlatformIdentity, PlatformMatrix,
    ProfileIdentity, ReleaseCandidate, ReleaseVersion, SchemaIdentity, ToolchainIdentity,
};
use vstd::prelude::*;

verus! {

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

pub open spec fn same_bytes_20_from(left: [u8; 20], right: [u8; 20], index: nat) -> bool
    decreases 20 - index,
{
    if index >= 20 {
        true
    } else {
        left[index as int] == right[index as int]
            && same_bytes_20_from(left, right, index + 1)
    }
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

pub open spec fn same_bytes_16(left: [u8; 16], right: [u8; 16]) -> bool {
    same_bytes_16_from(left, right, 0)
}

pub open spec fn same_bytes_20(left: [u8; 20], right: [u8; 20]) -> bool {
    same_bytes_20_from(left, right, 0)
}

pub open spec fn same_bytes_32(left: [u8; 32], right: [u8; 32]) -> bool {
    same_bytes_32_from(left, right, 0)
}

pub open spec fn digest_matches(left: peritus_types::Sha256Digest, right: peritus_types::Sha256Digest) -> bool {
    same_bytes_32(left.spec_bytes(), right.spec_bytes())
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

const fn bytes_20_equal_from(
    left: [u8; 20],
    right: [u8; 20],
    index: usize,
) -> (equal: bool)
    requires index <= 20,
    ensures equal == same_bytes_20_from(left, right, index as nat),
    decreases 20 - index,
{
    if index == 20 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes_20_equal_from(left, right, index + 1)
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

/// Executable equality for exact sixteen-byte identities.
pub const fn bytes_16_equal(left: [u8; 16], right: [u8; 16]) -> (equal: bool)
    ensures equal == same_bytes_16(left, right),
{
    bytes_16_equal_from(left, right, 0)
}

const fn bytes_20_equal(left: [u8; 20], right: [u8; 20]) -> (equal: bool)
    ensures equal == same_bytes_20(left, right),
{
    bytes_20_equal_from(left, right, 0)
}

const fn bytes_32_equal(left: [u8; 32], right: [u8; 32]) -> (equal: bool)
    ensures equal == same_bytes_32(left, right),
{
    bytes_32_equal_from(left, right, 0)
}

/// Executable equality for exact SHA-256 digests.
pub const fn digests_equal(
    left: peritus_types::Sha256Digest,
    right: peritus_types::Sha256Digest,
) -> (equal: bool)
    ensures equal == digest_matches(left, right),
{
    bytes_32_equal(*left.as_bytes(), *right.as_bytes())
}

pub open spec fn git_commits_match(left: GitCommitId, right: GitCommitId) -> bool {
    match (left.spec_sha1_bytes(), right.spec_sha1_bytes()) {
        (Some(left_bytes), Some(right_bytes)) => same_bytes_20(left_bytes, right_bytes),
        (None, None) => match (left.spec_sha256_bytes(), right.spec_sha256_bytes()) {
            (Some(left_bytes), Some(right_bytes)) => same_bytes_32(left_bytes, right_bytes),
            _ => false,
        },
        _ => false,
    }
}

const fn git_commits_equal(left: GitCommitId, right: GitCommitId) -> (equal: bool)
    ensures equal == git_commits_match(left, right),
{
    match (left.sha1_bytes(), right.sha1_bytes()) {
        (Some(left_bytes), Some(right_bytes)) => bytes_20_equal(left_bytes, right_bytes),
        (None, None) => match (left.sha256_bytes(), right.sha256_bytes()) {
            (Some(left_bytes), Some(right_bytes)) => bytes_32_equal(left_bytes, right_bytes),
            _ => false,
        },
        _ => false,
    }
}

pub open spec fn operating_systems_match(left: OperatingSystem, right: OperatingSystem) -> bool {
    matches!((left, right),
        (OperatingSystem::Linux, OperatingSystem::Linux)
            | (OperatingSystem::MacOs, OperatingSystem::MacOs)
            | (OperatingSystem::Windows, OperatingSystem::Windows))
}

const fn operating_systems_equal(
    left: OperatingSystem,
    right: OperatingSystem,
) -> (equal: bool)
    ensures equal == operating_systems_match(left, right),
{
    matches!((left, right),
        (OperatingSystem::Linux, OperatingSystem::Linux)
            | (OperatingSystem::MacOs, OperatingSystem::MacOs)
            | (OperatingSystem::Windows, OperatingSystem::Windows))
}

pub open spec fn architectures_match(left: Architecture, right: Architecture) -> bool {
    matches!((left, right),
        (Architecture::X86_64, Architecture::X86_64)
            | (Architecture::Aarch64, Architecture::Aarch64))
}

const fn architectures_equal(left: Architecture, right: Architecture) -> (equal: bool)
    ensures equal == architectures_match(left, right),
{
    matches!((left, right),
        (Architecture::X86_64, Architecture::X86_64)
            | (Architecture::Aarch64, Architecture::Aarch64))
}

pub open spec fn platforms_match(left: PlatformIdentity, right: PlatformIdentity) -> bool {
    operating_systems_match(left.spec_operating_system(), right.spec_operating_system())
        && architectures_match(left.spec_architecture(), right.spec_architecture())
        && same_bytes_32(
            left.spec_profile_digest().spec_bytes(),
            right.spec_profile_digest().spec_bytes(),
        )
}

const fn platforms_equal(left: PlatformIdentity, right: PlatformIdentity) -> (equal: bool)
    ensures equal == platforms_match(left, right),
{
    operating_systems_equal(left.operating_system(), right.operating_system())
        && architectures_equal(left.architecture(), right.architecture())
        && bytes_32_equal(
            *left.profile_digest().as_bytes(),
            *right.profile_digest().as_bytes(),
        )
}

pub open spec fn platform_matrices_match(left: PlatformMatrix, right: PlatformMatrix) -> bool {
    platforms_match(left.spec_linux(), right.spec_linux())
        && platforms_match(left.spec_macos(), right.spec_macos())
        && platforms_match(left.spec_windows(), right.spec_windows())
}

const fn platform_matrices_equal(left: PlatformMatrix, right: PlatformMatrix) -> (equal: bool)
    ensures equal == platform_matrices_match(left, right),
{
    platforms_equal(left.linux(), right.linux())
        && platforms_equal(left.macos(), right.macos())
        && platforms_equal(left.windows(), right.windows())
}

pub open spec fn versions_match(left: ReleaseVersion, right: ReleaseVersion) -> bool {
    left.spec_major() == right.spec_major()
        && left.spec_minor() == right.spec_minor()
        && left.spec_patch() == right.spec_patch()
        && same_bytes_32(
            left.spec_descriptor_digest().spec_bytes(),
            right.spec_descriptor_digest().spec_bytes(),
        )
}

const fn versions_equal(left: ReleaseVersion, right: ReleaseVersion) -> (equal: bool)
    ensures equal == versions_match(left, right),
{
    left.major() == right.major()
        && left.minor() == right.minor()
        && left.patch() == right.patch()
        && bytes_32_equal(
            *left.descriptor_digest().as_bytes(),
            *right.descriptor_digest().as_bytes(),
        )
}

pub open spec fn toolchains_match(left: ToolchainIdentity, right: ToolchainIdentity) -> bool {
    same_bytes_32(left.spec_rust_digest().spec_bytes(), right.spec_rust_digest().spec_bytes())
        && same_bytes_32(
            left.spec_verus_digest().spec_bytes(),
            right.spec_verus_digest().spec_bytes(),
        )
        && same_bytes_32(left.spec_vstd_digest().spec_bytes(), right.spec_vstd_digest().spec_bytes())
        && same_bytes_32(
            left.spec_solver_digest().spec_bytes(),
            right.spec_solver_digest().spec_bytes(),
        )
}

const fn toolchains_equal(left: ToolchainIdentity, right: ToolchainIdentity) -> (equal: bool)
    ensures equal == toolchains_match(left, right),
{
    bytes_32_equal(*left.rust_digest().as_bytes(), *right.rust_digest().as_bytes())
        && bytes_32_equal(*left.verus_digest().as_bytes(), *right.verus_digest().as_bytes())
        && bytes_32_equal(*left.vstd_digest().as_bytes(), *right.vstd_digest().as_bytes())
        && bytes_32_equal(*left.solver_digest().as_bytes(), *right.solver_digest().as_bytes())
}

pub open spec fn profiles_match(left: ProfileIdentity, right: ProfileIdentity) -> bool {
    left.spec_revision() == right.spec_revision()
        && same_bytes_32(left.spec_digest().spec_bytes(), right.spec_digest().spec_bytes())
}

const fn profiles_equal(left: ProfileIdentity, right: ProfileIdentity) -> (equal: bool)
    ensures equal == profiles_match(left, right),
{
    left.revision() == right.revision()
        && bytes_32_equal(*left.digest().as_bytes(), *right.digest().as_bytes())
}

pub open spec fn schemas_match(left: SchemaIdentity, right: SchemaIdentity) -> bool {
    left.spec_policy() == right.spec_policy()
        && left.spec_evidence() == right.spec_evidence()
        && left.spec_report() == right.spec_report()
        && left.spec_artifact() == right.spec_artifact()
        && same_bytes_32(
            left.spec_catalog_digest().spec_bytes(),
            right.spec_catalog_digest().spec_bytes(),
        )
}

const fn schemas_equal(left: SchemaIdentity, right: SchemaIdentity) -> (equal: bool)
    ensures equal == schemas_match(left, right),
{
    left.policy() == right.policy()
        && left.evidence() == right.evidence()
        && left.report() == right.report()
        && left.artifact() == right.artifact()
        && bytes_32_equal(
            *left.catalog_digest().as_bytes(),
            *right.catalog_digest().as_bytes(),
        )
}

/// Exact mathematical equality of every release-candidate identity component.
pub open spec fn candidate_matches_exactly(
    left: ReleaseCandidate,
    right: ReleaseCandidate,
) -> bool {
    same_bytes_16(left.spec_id().spec_bytes(), right.spec_id().spec_bytes())
        && git_commits_match(left.spec_commit(), right.spec_commit())
        && versions_match(left.spec_version(), right.spec_version())
        && platform_matrices_match(left.spec_platforms(), right.spec_platforms())
        && toolchains_match(left.spec_toolchain(), right.spec_toolchain())
        && profiles_match(left.spec_profile(), right.spec_profile())
        && schemas_match(left.spec_schemas(), right.spec_schemas())
        && left.spec_source_revision() == right.spec_source_revision()
        && same_bytes_32(
            left.spec_manifest_digest().spec_bytes(),
            right.spec_manifest_digest().spec_bytes(),
        )
}

/// Executable refinement of complete release-candidate identity equality.
pub const fn candidate_matches(
    left: &ReleaseCandidate,
    right: &ReleaseCandidate,
) -> (matches: bool)
    ensures matches == candidate_matches_exactly(*left, *right),
{
    bytes_16_equal(*left.id().as_bytes(), *right.id().as_bytes())
        && git_commits_equal(left.commit(), right.commit())
        && versions_equal(left.version(), right.version())
        && platform_matrices_equal(left.platforms(), right.platforms())
        && toolchains_equal(left.toolchain(), right.toolchain())
        && profiles_equal(left.profile(), right.profile())
        && schemas_equal(left.schemas(), right.schemas())
        && left.source_revision() == right.source_revision()
        && bytes_32_equal(
            *left.manifest_digest().as_bytes(),
            *right.manifest_digest().as_bytes(),
        )
}

} // verus!

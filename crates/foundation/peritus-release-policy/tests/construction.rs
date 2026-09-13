//! Checked release-policy value construction contracts.

mod support;

use peritus_release_policy::{
    Architecture, CandidateId, ConstructionErrorKind, EvidenceBinding, EvidenceObservation,
    FindingId, GitCommitId, GitObjectFormat, OperatingSystem, PlatformIdentity, PlatformMatrix,
    PrincipalId, ProfileIdentity, ReleaseCandidate, ReleaseEvidence, ReleaseVersion, ReviewId,
    SchemaIdentity, ToolchainIdentity,
};
use peritus_types::Sha256Digest;

const fn zero_digest() -> Sha256Digest {
    Sha256Digest::new([0; 32])
}

#[test]
fn zero_nominal_identities_are_rejected() {
    assert_eq!(
        CandidateId::new([0; 16]).expect_err("zero candidate id").kind(),
        ConstructionErrorKind::ZeroIdentity
    );
    assert_eq!(
        PrincipalId::new([0; 16]).expect_err("zero principal id").kind(),
        ConstructionErrorKind::ZeroIdentity
    );
    assert_eq!(
        ReviewId::new([0; 16]).expect_err("zero review id").kind(),
        ConstructionErrorKind::ZeroIdentity
    );
    assert_eq!(
        FindingId::new([0; 16]).expect_err("zero finding id").kind(),
        ConstructionErrorKind::ZeroIdentity
    );
    assert_eq!(
        GitCommitId::sha1([0; 20]).expect_err("zero commit").kind(),
        ConstructionErrorKind::ZeroIdentity
    );
    assert_eq!(
        GitCommitId::sha256([0; 32]).expect_err("zero SHA-256 commit").kind(),
        ConstructionErrorKind::ZeroIdentity
    );
}

#[test]
fn late_nonzero_identity_bytes_are_preserved_exactly() {
    let mut nominal = [0; 16];
    nominal[15] = 91;
    assert_eq!(*CandidateId::new(nominal).expect("candidate id").as_bytes(), nominal);
    assert_eq!(*PrincipalId::new(nominal).expect("principal id").as_bytes(), nominal);
    assert_eq!(*ReviewId::new(nominal).expect("review id").as_bytes(), nominal);
    assert_eq!(*FindingId::new(nominal).expect("finding id").as_bytes(), nominal);

    let mut sha1 = [0; 20];
    sha1[19] = 92;
    let commit = GitCommitId::sha1(sha1).expect("late SHA-1 byte");
    assert_eq!(commit.format(), GitObjectFormat::Sha1);
    assert_eq!(commit.sha1_bytes(), Some(sha1));
    assert_eq!(commit.sha256_bytes(), None);

    let mut sha256 = [0; 32];
    sha256[31] = 93;
    let commit = GitCommitId::sha256(sha256).expect("late SHA-256 byte");
    assert_eq!(commit.format(), GitObjectFormat::Sha256);
    assert_eq!(commit.sha1_bytes(), None);
    assert_eq!(commit.sha256_bytes(), Some(sha256));
}

#[test]
fn digest_bound_candidate_components_preserve_fields_and_reject_zero() {
    let version = ReleaseVersion::new(7, 8, 9, support::digest(1)).expect("version");
    assert_eq!((version.major(), version.minor(), version.patch()), (7, 8, 9));
    assert_eq!(version.descriptor_digest(), support::digest(1));
    assert_eq!(
        ReleaseVersion::new(7, 8, 9, zero_digest()).expect_err("zero descriptor").kind(),
        ConstructionErrorKind::ZeroDigest
    );

    let linux =
        PlatformIdentity::new(OperatingSystem::Linux, Architecture::Aarch64, support::digest(2))
            .expect("platform");
    assert_eq!(linux.operating_system(), OperatingSystem::Linux);
    assert_eq!(linux.architecture(), Architecture::Aarch64);
    assert_eq!(linux.profile_digest(), support::digest(2));
    assert_eq!(
        PlatformIdentity::new(OperatingSystem::Linux, Architecture::Aarch64, zero_digest())
            .expect_err("zero platform profile")
            .kind(),
        ConstructionErrorKind::ZeroDigest
    );

    let toolchain = ToolchainIdentity::new(
        support::digest(3),
        support::digest(4),
        support::digest(5),
        support::digest(6),
    )
    .expect("toolchain");
    assert_eq!(toolchain.rust_digest(), support::digest(3));
    assert_eq!(toolchain.verus_digest(), support::digest(4));
    assert_eq!(toolchain.vstd_digest(), support::digest(5));
    assert_eq!(toolchain.solver_digest(), support::digest(6));
    assert_eq!(
        ToolchainIdentity::new(
            support::digest(3),
            support::digest(4),
            zero_digest(),
            support::digest(6),
        )
        .expect_err("zero toolchain component")
        .kind(),
        ConstructionErrorKind::ZeroDigest
    );
}

#[test]
fn revision_errors_precede_later_digest_errors() {
    assert_eq!(
        ProfileIdentity::new(0, zero_digest()).expect_err("profile revision first").kind(),
        ConstructionErrorKind::ZeroRevision
    );
    assert_eq!(
        ProfileIdentity::new(1, zero_digest()).expect_err("profile digest second").kind(),
        ConstructionErrorKind::ZeroDigest
    );
    assert_eq!(
        SchemaIdentity::new(1, 0, 1, 1, zero_digest()).expect_err("schema revision first").kind(),
        ConstructionErrorKind::ZeroRevision
    );
    assert_eq!(
        SchemaIdentity::new(1, 1, 1, 1, zero_digest()).expect_err("schema digest second").kind(),
        ConstructionErrorKind::ZeroDigest
    );

    let base = support::candidate();
    assert_eq!(
        rebuild_candidate(&base, 0, zero_digest()).expect_err("source revision first").kind(),
        ConstructionErrorKind::ZeroRevision
    );
    assert_eq!(
        rebuild_candidate(&base, 1, zero_digest()).expect_err("manifest digest second").kind(),
        ConstructionErrorKind::ZeroDigest
    );
}

#[test]
fn revision_bound_components_and_platform_matrix_preserve_every_field() {
    let profile = ProfileIdentity::new(17, support::digest(7)).expect("profile");
    assert_eq!(profile.revision(), 17);
    assert_eq!(profile.digest(), support::digest(7));

    let schemas = SchemaIdentity::new(18, 19, 20, 21, support::digest(8)).expect("schemas");
    assert_eq!(schemas.policy(), 18);
    assert_eq!(schemas.evidence(), 19);
    assert_eq!(schemas.report(), 20);
    assert_eq!(schemas.artifact(), 21);
    assert_eq!(schemas.catalog_digest(), support::digest(8));

    let linux =
        PlatformIdentity::new(OperatingSystem::Linux, Architecture::X86_64, support::digest(9))
            .expect("linux");
    let macos =
        PlatformIdentity::new(OperatingSystem::MacOs, Architecture::Aarch64, support::digest(10))
            .expect("macOS");
    let windows =
        PlatformIdentity::new(OperatingSystem::Windows, Architecture::X86_64, support::digest(11))
            .expect("windows");
    let matrix = PlatformMatrix::new(linux, macos, windows).expect("platform matrix");
    assert_eq!(matrix.linux(), linux);
    assert_eq!(matrix.macos(), macos);
    assert_eq!(matrix.windows(), windows);
}

fn rebuild_candidate(
    base: &ReleaseCandidate,
    source_revision: u64,
    manifest_digest: Sha256Digest,
) -> Result<ReleaseCandidate, peritus_release_policy::ConstructionError> {
    ReleaseCandidate::new(
        base.id(),
        base.commit(),
        base.version(),
        base.platforms(),
        base.toolchain(),
        base.profile(),
        base.schemas(),
        source_revision,
        manifest_digest,
    )
}

#[test]
fn invalid_time_and_source_revision_bindings_are_rejected() {
    let candidate = support::candidate();
    assert_eq!(
        EvidenceBinding::new(candidate, 10, 9, 1, candidate.source_revision())
            .expect_err("inverted validity")
            .kind(),
        ConstructionErrorKind::InvalidValidityInterval
    );
    assert_eq!(
        EvidenceBinding::new(candidate, 10, 20, 0, candidate.source_revision())
            .expect_err("zero sequence")
            .kind(),
        ConstructionErrorKind::ZeroRevision
    );
    assert_eq!(
        EvidenceBinding::new(candidate, 10, 9, 0, 0)
            .expect_err("zero revision precedes the inverted interval")
            .kind(),
        ConstructionErrorKind::ZeroRevision
    );
}

#[test]
fn platform_slots_are_nominal_not_positional_guesswork() {
    let linux =
        PlatformIdentity::new(OperatingSystem::Linux, Architecture::X86_64, support::digest(1))
            .expect("linux");
    let windows =
        PlatformIdentity::new(OperatingSystem::Windows, Architecture::X86_64, support::digest(2))
            .expect("windows");
    assert_eq!(
        PlatformMatrix::new(windows, linux, windows).expect_err("mislabeled slots").kind(),
        ConstructionErrorKind::InvalidPlatformMatrix
    );
}

#[test]
fn evidence_collection_bound_is_enforced() {
    let inputs = support::ready_inputs();
    let observation: EvidenceObservation = inputs.observations[0];
    let oversized = vec![observation; ReleaseEvidence::MAX_COLLECTION_LEN + 1];
    assert_eq!(
        ReleaseEvidence::new(oversized, Vec::new(), Vec::new(), Vec::new(), Vec::new(),)
            .expect_err("oversized collection")
            .kind(),
        ConstructionErrorKind::CollectionLimitExceeded
    );
}

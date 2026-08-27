//! Verified-policy adapter seam tests.

use peritus_release_artifacts::{
    CandidateCommit, PlatformTriple, ReleaseBinding, ReleaseVersion as ArtifactVersion,
    Sha256Digest as ArtifactDigest, ToolchainId, digest_bytes,
};
use peritus_release_policy::{
    Architecture, CandidateId, GitCommitId, OperatingSystem, PlatformIdentity, PlatformMatrix,
    ProfileIdentity, ReleaseCandidate, ReleaseEvidence, ReleaseVersion as PolicyVersion,
    SchemaIdentity, ToolchainIdentity,
};
use peritus_types::Sha256Digest as PolicyDigest;

use super::{VerifiedPolicyBinding, VerifiedReleasePolicyAdapter};
use crate::{DeterministicReleasePolicy, PolicyDecision, ReleasePolicyInput};

fn artifact_digest(seed: u8) -> ArtifactDigest {
    ArtifactDigest::from_bytes([seed; 32])
}

fn policy_digest(seed: u8) -> PolicyDigest {
    PolicyDigest::new([seed; 32])
}

fn fixtures() -> (VerifiedPolicyBinding, ReleaseCandidate, ReleaseBinding) {
    let release = ReleaseBinding::new(
        CandidateCommit::new("44".repeat(20)).expect("commit"),
        ArtifactVersion::new("1.2.3").expect("version"),
        ToolchainId::new("rust-1.97.1_verus-0.2026.08.09").expect("toolchain"),
        PlatformTriple::new("tier-one-linux-macos-windows").expect("platform matrix"),
        artifact_digest(1),
    );
    let manifest = release.digest().expect("release binding digest");
    let descriptor = digest_bytes(release.version().as_str().as_bytes());
    let linux =
        PlatformIdentity::new(OperatingSystem::Linux, Architecture::X86_64, policy_digest(2))
            .expect("linux");
    let macos =
        PlatformIdentity::new(OperatingSystem::MacOs, Architecture::Aarch64, policy_digest(3))
            .expect("macos");
    let windows =
        PlatformIdentity::new(OperatingSystem::Windows, Architecture::X86_64, policy_digest(4))
            .expect("windows");
    let candidate = ReleaseCandidate::new(
        CandidateId::new([5; 16]).expect("candidate"),
        GitCommitId::sha1([0x44; 20]).expect("git commit"),
        PolicyVersion::new(1, 2, 3, PolicyDigest::new(*descriptor.as_bytes()))
            .expect("policy version"),
        PlatformMatrix::new(linux, macos, windows).expect("platform matrix"),
        ToolchainIdentity::new(
            policy_digest(6),
            policy_digest(7),
            policy_digest(8),
            policy_digest(9),
        )
        .expect("toolchain"),
        ProfileIdentity::new(1, policy_digest(10)).expect("profile"),
        SchemaIdentity::new(1, 1, 1, 1, policy_digest(11)).expect("schemas"),
        1,
        PolicyDigest::new(*manifest.as_bytes()),
    )
    .expect("release candidate");
    let binding = VerifiedPolicyBinding::new(
        release.clone(),
        artifact_digest(12),
        artifact_digest(13),
        artifact_digest(14),
        artifact_digest(15),
    );
    (binding, candidate, release)
}

fn empty_evidence() -> ReleaseEvidence {
    ReleaseEvidence::new(Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new())
        .expect("empty bounded evidence")
}

#[test]
fn exact_composition_reaches_verified_policy_and_remains_not_ready_without_evidence() {
    let (binding, candidate, release) = fixtures();
    let input = ReleasePolicyInput::new(
        release,
        artifact_digest(12),
        artifact_digest(13),
        artifact_digest(14),
        artifact_digest(15),
        Vec::new(),
    );
    let adapter = VerifiedReleasePolicyAdapter::new(binding, candidate, 50, empty_evidence());
    assert!(matches!(adapter.evaluate(&input), PolicyDecision::NotReady { .. }));
}

#[test]
fn digest_drift_never_reaches_verified_policy() {
    let (binding, candidate, release) = fixtures();
    let input = ReleasePolicyInput::new(
        release,
        artifact_digest(99),
        artifact_digest(13),
        artifact_digest(14),
        artifact_digest(15),
        Vec::new(),
    );
    let adapter = VerifiedReleasePolicyAdapter::new(binding, candidate, 50, empty_evidence());
    let PolicyDecision::Unavailable { failure } = adapter.evaluate(&input) else {
        panic!("binding drift must make verified policy unavailable");
    };
    assert_eq!(failure.as_str(), "release-policy.input-binding-mismatch");
}

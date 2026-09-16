//! Fail-closed release-policy evaluation contracts.

mod support;

use peritus_release_policy::{
    Architecture, CandidateId, Diagnostic, EvidenceObservation, EvidenceRequirement,
    EvidenceSourceKind, GitCommitId, OperatingSystem, PlatformIdentity, PlatformMatrix,
    ProfileIdentity, ReleaseCandidate, ReleaseVerdict, ReleaseVersion, SchemaIdentity,
    ToolchainIdentity,
};
use support::{binding, digest, mismatched_candidate, ready_inputs, stale_binding};

#[test]
fn stale_evidence_is_diagnosed_and_cannot_contribute() {
    let mut inputs = ready_inputs();
    let candidate = inputs.candidate;
    inputs.observations.retain(|value| value.requirement() != EvidenceRequirement::GateA);
    inputs.observations.push(
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            stale_binding(&candidate, 500),
            digest(201),
            digest(202),
            true,
            true,
        )
        .expect("stale observation is structurally valid"),
    );
    let decision = inputs.evaluate();
    assert_eq!(decision.verdict(), ReleaseVerdict::NotReadyForProduction);
    assert!(
        decision.diagnostics().contains(&Diagnostic::StaleEvidence(EvidenceRequirement::GateA, 1,))
    );
    assert!(
        decision.diagnostics().contains(&Diagnostic::MissingEvidence(EvidenceRequirement::GateA,))
    );
}

#[test]
fn mismatched_candidate_evidence_is_diagnosed_and_excluded() {
    let mut inputs = ready_inputs();
    inputs.observations.retain(|value| value.requirement() != EvidenceRequirement::GateA);
    let other = mismatched_candidate();
    inputs.observations.push(
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            binding(&other, 501),
            digest(203),
            digest(204),
            true,
            true,
        )
        .expect("mismatched observation is structurally valid"),
    );
    let decision = inputs.evaluate();
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::MismatchedEvidence(EvidenceRequirement::GateA, 1,))
    );
    assert!(
        decision.diagnostics().contains(&Diagnostic::MissingEvidence(EvidenceRequirement::GateA,))
    );
}

#[test]
fn every_complete_candidate_identity_component_is_checked() {
    let base = support::candidate();
    for other in candidate_mutations(&base) {
        let mut inputs = ready_inputs();
        inputs.observations.retain(|value| value.requirement() != EvidenceRequirement::GateA);
        inputs.observations.push(
            EvidenceObservation::new(
                EvidenceRequirement::GateA,
                EvidenceSourceKind::GateA,
                binding(&other, 520),
                digest(220),
                digest(221),
                true,
                true,
            )
            .expect("mutated candidate observation"),
        );
        let decision = inputs.evaluate();
        assert!(
            decision
                .diagnostics()
                .contains(&Diagnostic::MismatchedEvidence(EvidenceRequirement::GateA, 1,))
        );
    }
}

#[derive(Clone, Copy)]
struct CandidateParts {
    id: CandidateId,
    commit: GitCommitId,
    version: ReleaseVersion,
    platforms: PlatformMatrix,
    toolchain: ToolchainIdentity,
    profile: ProfileIdentity,
    schemas: SchemaIdentity,
    source_revision: u64,
    manifest_digest: peritus_types::Sha256Digest,
}

impl CandidateParts {
    const fn from_candidate(candidate: &ReleaseCandidate) -> Self {
        Self {
            id: candidate.id(),
            commit: candidate.commit(),
            version: candidate.version(),
            platforms: candidate.platforms(),
            toolchain: candidate.toolchain(),
            profile: candidate.profile(),
            schemas: candidate.schemas(),
            source_revision: candidate.source_revision(),
            manifest_digest: candidate.manifest_digest(),
        }
    }

    fn build(self) -> ReleaseCandidate {
        ReleaseCandidate::new(
            self.id,
            self.commit,
            self.version,
            self.platforms,
            self.toolchain,
            self.profile,
            self.schemas,
            self.source_revision,
            self.manifest_digest,
        )
        .expect("mutated release candidate")
    }
}

fn candidate_mutations(base: &ReleaseCandidate) -> [ReleaseCandidate; 9] {
    let original = CandidateParts::from_candidate(base);
    let changed_linux =
        PlatformIdentity::new(OperatingSystem::Linux, Architecture::Aarch64, digest(10))
            .expect("changed Linux target");
    [
        CandidateParts { id: CandidateId::new([2; 16]).expect("changed id"), ..original }.build(),
        CandidateParts { commit: GitCommitId::sha1([3; 20]).expect("changed commit"), ..original }
            .build(),
        CandidateParts {
            version: ReleaseVersion::new(2, 0, 0, digest(3)).expect("changed version"),
            ..original
        }
        .build(),
        CandidateParts {
            platforms: PlatformMatrix::new(
                changed_linux,
                base.platforms().macos(),
                base.platforms().windows(),
            )
            .expect("changed platform matrix"),
            ..original
        }
        .build(),
        CandidateParts {
            toolchain: ToolchainIdentity::new(digest(44), digest(5), digest(6), digest(7))
                .expect("changed toolchain"),
            ..original
        }
        .build(),
        CandidateParts {
            profile: ProfileIdentity::new(2, digest(8)).expect("changed profile"),
            ..original
        }
        .build(),
        CandidateParts {
            schemas: SchemaIdentity::new(2, 1, 1, 1, digest(9)).expect("changed schemas"),
            ..original
        }
        .build(),
        CandidateParts { source_revision: base.source_revision() + 1, ..original }.build(),
        CandidateParts { manifest_digest: digest(99), ..original }.build(),
    ]
}

#[test]
fn wrong_source_unreviewed_unsigned_and_conflict_are_distinct() {
    let mut inputs = ready_inputs();
    let candidate = inputs.candidate;
    inputs.observations.retain(|value| value.requirement() != EvidenceRequirement::GateA);
    inputs.observations.extend([
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::Foundation,
            binding(&candidate, 510),
            digest(210),
            digest(211),
            true,
            true,
        )
        .expect("wrong source"),
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            binding(&candidate, 511),
            digest(212),
            digest(213),
            false,
            true,
        )
        .expect("unreviewed"),
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            binding(&candidate, 512),
            digest(214),
            digest(1),
            true,
            false,
        )
        .expect("unsigned"),
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            binding(&candidate, 513),
            digest(215),
            digest(216),
            true,
            true,
        )
        .expect("first current"),
        EvidenceObservation::new(
            EvidenceRequirement::GateA,
            EvidenceSourceKind::GateA,
            binding(&candidate, 514),
            digest(217),
            digest(218),
            true,
            true,
        )
        .expect("conflicting current"),
    ]);
    let decision = inputs.evaluate();
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::WrongEvidenceSource(EvidenceRequirement::GateA, 1,))
    );
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::UnreviewedEvidence(EvidenceRequirement::GateA, 1,))
    );
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::UnsignedEvidence(EvidenceRequirement::GateA, 1,))
    );
    assert!(
        decision
            .diagnostics()
            .contains(&Diagnostic::ConflictingEvidence(EvidenceRequirement::GateA,))
    );
}

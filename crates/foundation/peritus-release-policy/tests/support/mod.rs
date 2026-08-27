#![allow(dead_code, reason = "integration test binaries use different fixture subsets")]

use peritus_release_policy::{
    Architecture, CandidateId, EvidenceBinding, EvidenceObservation, FindingObservation,
    GitCommitId, OperatingSystem, PlatformIdentity, PlatformMatrix, PrincipalId, ProfileIdentity,
    QualificationObservation, QualificationSlice, QualificationVerdict, REQUIRED_EVIDENCE,
    ReleaseCandidate, ReleaseDecision, ReleaseEvidence, ReleaseVersion, ReviewId,
    ReviewObservation, ReviewOutcome, SchemaIdentity, ToolchainIdentity, WaiverObservation,
    evaluate_release,
};
use peritus_types::Sha256Digest;

pub const EVALUATED_AT: u64 = 50;

pub struct Inputs {
    pub candidate: ReleaseCandidate,
    pub observations: Vec<EvidenceObservation>,
    pub qualifications: Vec<QualificationObservation>,
    pub reviews: Vec<ReviewObservation>,
    pub findings: Vec<FindingObservation>,
    pub waivers: Vec<WaiverObservation>,
}

impl Inputs {
    pub fn evidence(self) -> ReleaseEvidence {
        ReleaseEvidence::new(
            self.observations,
            self.qualifications,
            self.reviews,
            self.findings,
            self.waivers,
        )
        .expect("fixture evidence is bounded")
    }

    pub fn evaluate(self) -> ReleaseDecision {
        let candidate = self.candidate;
        let evidence = self.evidence();
        evaluate_release(candidate, EVALUATED_AT, &evidence)
    }
}

pub const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([if seed == 0 { 1 } else { seed }; 32])
}

pub fn principal(seed: u8) -> PrincipalId {
    PrincipalId::new([seed.max(1); 16]).expect("fixture principal")
}

pub fn candidate() -> ReleaseCandidate {
    let linux = PlatformIdentity::new(OperatingSystem::Linux, Architecture::X86_64, digest(10))
        .expect("linux target");
    let macos = PlatformIdentity::new(OperatingSystem::MacOs, Architecture::Aarch64, digest(11))
        .expect("macOS target");
    let windows = PlatformIdentity::new(OperatingSystem::Windows, Architecture::X86_64, digest(12))
        .expect("windows target");
    ReleaseCandidate::new(
        CandidateId::new([1; 16]).expect("candidate id"),
        GitCommitId::sha1([2; 20]).expect("commit"),
        ReleaseVersion::new(1, 0, 0, digest(3)).expect("version"),
        PlatformMatrix::new(linux, macos, windows).expect("platform matrix"),
        ToolchainIdentity::new(digest(4), digest(5), digest(6), digest(7)).expect("toolchain"),
        ProfileIdentity::new(1, digest(8)).expect("profile"),
        SchemaIdentity::new(1, 1, 1, 1, digest(9)).expect("schemas"),
        7,
        digest(13),
    )
    .expect("release candidate")
}

pub fn binding(candidate: &ReleaseCandidate, sequence: u64) -> EvidenceBinding {
    EvidenceBinding::new(*candidate, 10, 100, sequence, candidate.source_revision())
        .expect("fixture binding")
}

pub fn ready_inputs() -> Inputs {
    let candidate = candidate();
    let observations = REQUIRED_EVIDENCE
        .iter()
        .map(|requirement| {
            let id = requirement.stable_id();
            EvidenceObservation::new(
                *requirement,
                requirement.source_kind(),
                binding(&candidate, u64::from(id)),
                digest(id.wrapping_add(20)),
                digest(id.wrapping_add(80)),
                true,
                true,
            )
            .expect("fixture observation")
        })
        .collect();
    let qualifications = QualificationSlice::ALL
        .iter()
        .map(|slice| {
            let ordinal = slice.ordinal();
            QualificationObservation::new(
                *slice,
                binding(&candidate, 100 + u64::from(ordinal)),
                QualificationVerdict::Ready,
                digest(140 + ordinal),
                digest(150 + ordinal),
                principal(30 + ordinal),
                true,
            )
            .expect("fixture qualification")
        })
        .collect();
    let producer = principal(90);
    let reviews = vec![
        ReviewObservation::new(
            ReviewId::new([1; 16]).expect("review id"),
            binding(&candidate, 201),
            principal(40),
            producer,
            digest(160),
            digest(161),
            ReviewOutcome::Approved,
            true,
        )
        .expect("review one"),
        ReviewObservation::new(
            ReviewId::new([2; 16]).expect("review id"),
            binding(&candidate, 202),
            principal(41),
            producer,
            digest(162),
            digest(163),
            ReviewOutcome::Approved,
            true,
        )
        .expect("review two"),
    ];
    Inputs {
        candidate,
        observations,
        qualifications,
        reviews,
        findings: Vec::new(),
        waivers: Vec::new(),
    }
}

pub fn stale_binding(candidate: &ReleaseCandidate, sequence: u64) -> EvidenceBinding {
    EvidenceBinding::new(*candidate, 1, 2, sequence, candidate.source_revision())
        .expect("stale fixture binding")
}

pub fn mismatched_candidate() -> ReleaseCandidate {
    let base = candidate();
    ReleaseCandidate::new(
        CandidateId::new([99; 16]).expect("other candidate id"),
        base.commit(),
        base.version(),
        base.platforms(),
        base.toolchain(),
        base.profile(),
        base.schemas(),
        base.source_revision(),
        digest(199),
    )
    .expect("mismatched candidate")
}

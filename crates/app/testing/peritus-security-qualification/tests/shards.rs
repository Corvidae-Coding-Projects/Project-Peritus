//! Native platform sharding and exact-candidate aggregation contracts.

use peritus_security_qualification::{
    CancellationToken, CleanupObservation, EvidenceEntry, EvidenceSet, EvidenceValue,
    FreshSubjectFactory, IntegratedCandidate, NativeExecutionReceipt, ProbeObservation,
    ProbeOutcome, ProbeRequest, ProbeSpec, QualificationError, QualificationLimits,
    QualificationPlatform, QualificationRunner, QualificationSubject, ResourceUsage,
    SafeEvidenceCode,
};
use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, Sha256Digest, WorkspaceId,
};

#[test]
fn canonical_three_host_shards_cover_every_probe_exactly_once() {
    let candidate = candidate(1);
    let limits = QualificationLimits::production();
    let cancellation = CancellationToken::new();
    let shards = QualificationPlatform::ALL
        .into_iter()
        .map(|platform| {
            QualificationRunner
                .run_shard(
                    &mut PassingFactory::new(platform),
                    candidate,
                    limits,
                    &cancellation,
                    platform,
                )
                .expect("canonical shard")
        })
        .collect::<Vec<_>>();

    assert_eq!(shards[0].cases().len(), 40);
    assert_eq!(shards[1].cases().len(), 1);
    assert_eq!(shards[2].cases().len(), 1);
    let run = QualificationRunner.aggregate(shards).expect("aggregate shards");
    assert_eq!(run.cases().len(), 42);
    assert!(run.all_passed());
}

#[test]
fn aggregation_rejects_missing_platform_and_candidate_drift() {
    let limits = QualificationLimits::production();
    let cancellation = CancellationToken::new();
    let linux = shard(candidate(2), limits, &cancellation, QualificationPlatform::Linux);
    let macos = shard(candidate(2), limits, &cancellation, QualificationPlatform::Macos);
    assert!(QualificationRunner.aggregate(vec![linux.clone(), macos.clone()]).is_err());

    let windows = shard(candidate(3), limits, &cancellation, QualificationPlatform::Windows);
    assert!(QualificationRunner.aggregate(vec![linux, macos, windows]).is_err());
}

fn shard(
    candidate: IntegratedCandidate,
    limits: QualificationLimits,
    cancellation: &CancellationToken,
    platform: QualificationPlatform,
) -> peritus_security_qualification::QualificationShard {
    QualificationRunner
        .run_shard(&mut PassingFactory::new(platform), candidate, limits, cancellation, platform)
        .expect("canonical shard")
}

struct PassingFactory {
    platform: QualificationPlatform,
    next: u32,
}

impl PassingFactory {
    const fn new(platform: QualificationPlatform) -> Self {
        Self { platform, next: 0 }
    }
}

impl FreshSubjectFactory for PassingFactory {
    fn create(
        &mut self,
        candidate: IntegratedCandidate,
        spec: ProbeSpec,
        _limits: QualificationLimits,
        _cancellation: &CancellationToken,
    ) -> Result<Box<dyn QualificationSubject>, QualificationError> {
        self.next += 1;
        Ok(Box::new(PassingSubject {
            id: format!("{}-{}", self.platform.as_str(), self.next),
            candidate,
            spec,
        }))
    }
}

struct PassingSubject {
    id: String,
    candidate: IntegratedCandidate,
    spec: ProbeSpec,
}

impl QualificationSubject for PassingSubject {
    fn subject_id(&self) -> &str {
        &self.id
    }

    fn execute(
        &mut self,
        _request: ProbeRequest<'_>,
    ) -> Result<ProbeObservation, QualificationError> {
        let mut evidence = EvidenceSet::new();
        evidence.insert(EvidenceEntry::new(
            SafeEvidenceCode::new("assertion.observed")?,
            EvidenceValue::Fact(true),
        ))?;
        let receipt = NativeExecutionReceipt::from_native_observation(
            digest(40),
            digest(41),
            digest(42),
            0,
            true,
            ResourceUsage::new(1, 1, 1, 1, 0),
            evidence,
        )?;
        Ok(ProbeObservation::from_native_execution(
            self.candidate,
            self.spec.id(),
            ProbeOutcome::Passed,
            receipt,
        ))
    }

    fn cleanup(self: Box<Self>) -> Result<CleanupObservation, QualificationError> {
        CleanupObservation::new(self.id, 0, 0, 0, 0, digest(43))
    }
}

fn candidate(seed: u8) -> IntegratedCandidate {
    IntegratedCandidate::new(
        RevisionTuple::new(
            AcceptanceSpecId::new([seed; 16]).expect("acceptance"),
            HarnessId::new([seed.wrapping_add(1); 16]).expect("harness"),
            WorkspaceId::new([seed.wrapping_add(2); 16]).expect("workspace"),
            Generation::first(),
            RevisionNumber::first(),
            PolicyId::new([seed.wrapping_add(3); 16]).expect("policy"),
            ProviderProfileId::new([seed.wrapping_add(4); 16]).expect("provider"),
        ),
        digest(seed),
        digest(seed.wrapping_add(10)),
        digest(seed.wrapping_add(20)),
    )
}

const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([seed; 32])
}

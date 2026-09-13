//! H0 fresh-subject, false-success, manifest, and external-review contracts.

use peritus_security_qualification::{
    CancellationToken, CleanupObservation, EvidenceEntry, EvidenceSet, EvidenceValue,
    FindingLifecycle, FindingObservation, FindingSeverity, FreshSubjectFactory,
    IndependentSecurityReview, IntegratedCandidate, NativeExecutionReceipt, ProbeId,
    ProbeObservation, ProbeOutcome, ProbeRequest, ProbeSpec, QualificationError,
    QualificationLimits, QualificationReport, QualificationRunner, QualificationSubject,
    ReadinessVerdict, ResourceUsage, ReviewCompletion, ReviewScope, ReviewerIdentity,
    SafeEvidenceCode,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, FindingId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

struct Factory {
    next: u32,
    stale_probe: Option<ProbeId>,
    candidate: IntegratedCandidate,
}

impl FreshSubjectFactory for Factory {
    fn create(
        &mut self,
        _candidate: IntegratedCandidate,
        spec: ProbeSpec,
        _limits: QualificationLimits,
        _cancellation: &CancellationToken,
    ) -> Result<Box<dyn QualificationSubject>, QualificationError> {
        self.next += 1;
        Ok(Box::new(Subject {
            id: format!("h0-fresh-{}", self.next),
            spec,
            candidate: self.candidate,
            stale: self.stale_probe == Some(spec.id()),
        }))
    }
}

struct Subject {
    id: String,
    spec: ProbeSpec,
    candidate: IntegratedCandidate,
    stale: bool,
}

impl QualificationSubject for Subject {
    fn subject_id(&self) -> &str {
        &self.id
    }

    fn execute(
        &mut self,
        request: ProbeRequest<'_>,
    ) -> Result<ProbeObservation, QualificationError> {
        assert_eq!(request.spec(), self.spec);
        assert!(!request.cancellation().is_cancelled());
        let mut evidence = EvidenceSet::new();
        evidence.insert(EvidenceEntry::new(
            SafeEvidenceCode::new("assertion.observed").expect("label"),
            EvidenceValue::Fact(true),
        ))?;
        let receipt = NativeExecutionReceipt::from_native_observation(
            digest(71),
            digest(72),
            digest(73),
            0,
            true,
            ResourceUsage::new(10, 2, 1024, 128, 1),
            evidence,
        )?;
        let observed_candidate = if self.stale { candidate(90) } else { self.candidate };
        Ok(ProbeObservation::from_native_execution(
            observed_candidate,
            self.spec.id(),
            ProbeOutcome::Passed,
            receipt,
        ))
    }

    fn cleanup(self: Box<Self>) -> Result<CleanupObservation, QualificationError> {
        CleanupObservation::new(self.id, 0, 0, 0, 0, digest(74))
    }
}

#[test]
fn full_native_campaign_and_independent_review_can_reach_h0_ready() {
    let candidate = candidate(1);
    let run = run(candidate, None);
    let report = QualificationReport::evaluate(run, Some(review(candidate, Vec::new())))
        .expect("qualification report");
    assert!(report.is_ready());
    assert!(matches!(report.verdict(), ReadinessVerdict::Ready(_)));
    assert_eq!(report.run().cases().len(), 42);
}

#[test]
fn absent_external_review_cannot_be_fabricated_into_success() {
    let candidate = candidate(2);
    let report = QualificationReport::evaluate(run(candidate, None), None).expect("report");
    assert!(!report.is_ready());
    assert!(matches!(report.verdict(), ReadinessVerdict::NotReady(reasons) if !reasons.is_empty()));
}

#[test]
fn stale_native_observation_fails_the_exact_candidate() {
    let candidate = candidate(3);
    let run = run(candidate, Some(ProbeId::CandidateMutationInvalidation));
    let report =
        QualificationReport::evaluate(run, Some(review(candidate, Vec::new()))).expect("report");
    assert!(!report.is_ready());
    assert!(matches!(report.verdict(), ReadinessVerdict::NotReady(reasons) if !reasons.is_empty()));
}

#[test]
fn cancelled_campaign_records_every_remaining_case_as_non_success() {
    let candidate = candidate(4);
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let mut factory = Factory { next: 0, stale_probe: None, candidate };
    let run = QualificationRunner
        .run(&mut factory, candidate, QualificationLimits::production(), &cancellation)
        .expect("canonical cancelled run");
    assert_eq!(factory.next, 0);
    assert!(run.cases().iter().all(|case| !matches!(
        case.outcome(),
        peritus_security_qualification::CaseOutcome::Passed
    )));
}

#[test]
fn unresolved_external_high_finding_is_not_ready() {
    let candidate = candidate(5);
    let finding = FindingObservation::new(
        FindingId::new([88; 16]).expect("finding"),
        candidate,
        FindingSeverity::High,
        FindingLifecycle::Open,
    );
    let report =
        QualificationReport::evaluate(run(candidate, None), Some(review(candidate, vec![finding])))
            .expect("report");
    assert!(!report.is_ready());
}

#[test]
fn canonical_manifest_bytes_and_digest_are_reproducible() {
    let candidate = candidate(6);
    let run = run(candidate, None);
    let first = QualificationReport::evaluate(run.clone(), Some(review(candidate, Vec::new())))
        .expect("first report");
    let second = QualificationReport::evaluate(run, Some(review(candidate, Vec::new())))
        .expect("second report");
    assert_eq!(first.manifest().canonical_json(), second.manifest().canonical_json());
    assert_eq!(first.manifest().digest(), second.manifest().digest());
}

fn run(
    candidate: IntegratedCandidate,
    stale_probe: Option<ProbeId>,
) -> peritus_security_qualification::QualificationRun {
    let mut factory = Factory { next: 0, stale_probe, candidate };
    QualificationRunner
        .run(&mut factory, candidate, QualificationLimits::production(), &CancellationToken::new())
        .expect("run")
}

fn review(
    candidate: IntegratedCandidate,
    findings: Vec<FindingObservation>,
) -> IndependentSecurityReview {
    IndependentSecurityReview::new(
        candidate,
        ReviewerIdentity::new(ActorId::new([20; 16]).expect("reviewer"), digest(20), digest(21)),
        ActorId::new([10; 16]).expect("producer"),
        digest(10),
        ReviewCompletion::Completed,
        ReviewScope::ALL.to_vec(),
        digest(22),
        findings,
    )
    .expect("review")
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

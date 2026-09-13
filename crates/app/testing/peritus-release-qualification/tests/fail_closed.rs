//! Fail-closed H4 release qualification contracts.

use std::cell::Cell;

use peritus_release_artifacts::{
    CandidateCommit, PlatformTriple, ReleaseBinding, ReleaseVersion, ToolchainId, digest_bytes,
};
use peritus_release_qualification::{
    DeterministicReleasePolicy, FreshSubjectFactory, FreshSubjectRunner, PolicyDecision,
    PolicyFailure, QualificationError, QualificationInputs, QualificationReport,
    QualificationSubject, QualificationVerdict, SubjectId,
};

fn binding() -> ReleaseBinding {
    ReleaseBinding::new(
        CandidateCommit::new("3".repeat(40)).expect("commit"),
        ReleaseVersion::new("1.0.0").expect("version"),
        ToolchainId::new("rust-1.97.1").expect("toolchain"),
        PlatformTriple::new("aarch64-apple-darwin@macos-15").expect("platform"),
        digest_bytes(b"tree"),
    )
}

struct ObservedPolicy {
    calls: Cell<usize>,
}

impl DeterministicReleasePolicy for ObservedPolicy {
    fn evaluate(
        &self,
        _input: &peritus_release_qualification::ReleasePolicyInput,
    ) -> PolicyDecision {
        self.calls.set(self.calls.get() + 1);
        PolicyDecision::Unavailable {
            failure: PolicyFailure::new("policy.not-wired").expect("policy code"),
        }
    }
}

#[test]
fn missing_inputs_are_not_ready_and_policy_is_not_called() {
    let policy = ObservedPolicy { calls: Cell::new(0) };
    let report = QualificationReport::evaluate(&QualificationInputs::new(binding()), &policy)
        .expect("fail-closed report");
    assert_eq!(report.verdict(), QualificationVerdict::NotReady);
    assert!(!report.blockers().is_empty());
    assert_eq!(policy.calls.get(), 0);
    assert!(report.policy_decision().is_none());
}

struct NeverSubject {
    id: SubjectId,
}

impl QualificationSubject for NeverSubject {
    fn subject_id(&self) -> &SubjectId {
        &self.id
    }

    fn collect(
        &mut self,
        _request: &peritus_release_qualification::CollectionRequest,
    ) -> Result<peritus_release_qualification::SignedEvidenceRecord, QualificationError> {
        Err(invalid_test_identity())
    }

    fn close(
        self,
    ) -> Result<peritus_release_qualification::CleanupObservation, QualificationError> {
        Ok(peritus_release_qualification::CleanupObservation::new(0, 0, 0, 0))
    }
}

struct FailingFactory;

impl FreshSubjectFactory for FailingFactory {
    type Subject = NeverSubject;

    fn create(
        &mut self,
        _request: &peritus_release_qualification::CollectionRequest,
    ) -> Result<Self::Subject, QualificationError> {
        Err(invalid_test_identity())
    }
}

fn invalid_test_identity() -> QualificationError {
    SubjectId::new("INVALID SUBJECT").expect_err("uppercase space-bearing identity is invalid")
}

#[test]
fn provisioning_failure_is_retained_for_every_required_campaign() {
    let run = FreshSubjectRunner::new(binding()).run(&mut FailingFactory);
    assert_eq!(run.cases().len(), 11);
    assert!(!run.is_complete());
    assert!(run.records().next().is_none());
}

//! Per-candidate D1, D2, and D3 quality-cycle binding behavior.

use peritus_collaboration::CollaborationId;
use peritus_orchestrator::{
    CandidateBinding, OrchestratorErrorKind, OrchestratorLimits, QualityCycleBinding,
};
use peritus_scheduler::SchedulerId;
use peritus_types::{
    AcceptanceSpecId, ActorId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, RunId, Sha256Digest, SnapshotId, WorkspaceId,
};

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}

const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}

fn revision(seed: u8) -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(seed)).expect("acceptance id is nonzero"),
        HarnessId::new(bytes(seed.wrapping_add(1))).expect("harness id is nonzero"),
        WorkspaceId::new(bytes(seed.wrapping_add(2))).expect("workspace id is nonzero"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(seed.wrapping_add(3))).expect("policy id is nonzero"),
        ProviderProfileId::new(bytes(seed.wrapping_add(4))).expect("provider id is nonzero"),
    )
}

fn limits() -> OrchestratorLimits {
    OrchestratorLimits::new(4, 4, 4, 4, 4, 16, 16, 32, 8, 16, 65_536, 262_144)
        .expect("fixture limits are valid")
}

fn quality_cycle(
    revision: RevisionTuple,
    gate_plan: Sha256Digest,
    review_binding: Sha256Digest,
) -> Result<QualityCycleBinding, peritus_orchestrator::OrchestratorError> {
    QualityCycleBinding::new(
        revision,
        RunId::new(bytes(20)).expect("gate run is nonzero"),
        RunId::new(bytes(21)).expect("scheduler run is nonzero"),
        RunId::new(bytes(22)).expect("collaboration run is nonzero"),
        gate_plan,
        review_binding,
        SchedulerId::new(bytes(23)).expect("scheduler id is nonzero"),
        digest(24),
        CollaborationId::new(bytes(25)).expect("collaboration id is nonzero"),
        digest(26),
    )
}

fn candidate(revision: RevisionTuple) -> CandidateBinding {
    CandidateBinding::new(
        revision,
        SnapshotId::new(bytes(30)).expect("snapshot id is nonzero"),
        digest(31),
        digest(32),
        digest(33),
        None,
        None,
        vec![ActorId::new(bytes(34)).expect("producer id is nonzero")],
        vec![digest(35)],
        limits(),
    )
    .expect("fixture candidate is valid")
}

#[test]
fn cycle_requires_nonzero_gate_and_review_bindings() {
    for error in [
        quality_cycle(revision(1), digest(0), digest(11))
            .expect_err("zero gate plan digest is invalid"),
        quality_cycle(revision(1), digest(10), digest(0))
            .expect_err("zero review binding digest is invalid"),
    ] {
        assert_eq!(error.kind(), OrchestratorErrorKind::InvalidInput);
    }
}

#[test]
fn cycle_validation_is_typed_to_the_current_candidate_revision() {
    let current_revision = revision(1);
    let cycle =
        quality_cycle(current_revision, digest(10), digest(11)).expect("quality cycle is valid");

    cycle
        .validate_for_candidate(&candidate(current_revision))
        .expect("same-revision candidate is bound");
    let error = cycle
        .validate_for_candidate(&candidate(revision(40)))
        .expect_err("another candidate revision is stale");
    assert_eq!(error.kind(), OrchestratorErrorKind::BindingMismatch);
}

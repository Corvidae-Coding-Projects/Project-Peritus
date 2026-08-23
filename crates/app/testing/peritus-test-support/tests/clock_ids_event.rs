//! Determinism and exhaustion tests for clocks, A1 identifiers, and event fixture contexts.

use peritus_test_support::{
    ClockComponent, ClockError, DeterministicIdSource, EventFixtureBuilder, EventFixtureError,
    FakeClock, IdSourceError,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, ApprovalRequestId, ArtifactId, AttemptId, BudgetId,
    BudgetReservationId, CommandId, EnvironmentId, EvaluationCampaignId, EventId, EventSequence,
    EvidenceId, EvolutionCampaignId, FindingId, GateExecutionId, GateId, HarnessId,
    IdentifierError, PolicyId, ProcessId, ProjectId, ProviderProfileId, ResourceId, ReviewCycleId,
    RunId, SessionId, SnapshotId, TurnId, WorkspaceId,
};
use std::num::NonZeroU64;
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn clock_reads_are_stable_and_advances_are_atomic() {
    let clock = FakeClock::default();
    let initial = clock.reading().expect("clock must be readable");
    assert_eq!(initial.wall_time(), UNIX_EPOCH);
    assert_eq!(initial.monotonic(), Duration::ZERO);
    assert_eq!(clock.reading().expect("repeat read must work"), initial);

    let advanced = clock.advance(Duration::from_millis(17)).expect("exact advance must fit");
    assert_eq!(advanced.wall_time(), UNIX_EPOCH + Duration::from_millis(17));
    assert_eq!(advanced.monotonic(), Duration::from_millis(17));

    let clone = clock.clone();
    clone.advance(Duration::from_millis(2)).expect("shared clone must advance");
    assert_eq!(
        clock.reading().expect("original must observe clone"),
        clone.reading().expect("clone must remain readable")
    );

    let fork = clock.fork().expect("fork must snapshot");
    fork.advance(Duration::from_millis(3)).expect("fork must advance independently");
    assert_ne!(
        fork.reading().expect("fork must be readable"),
        clock.reading().expect("original must be readable")
    );
}

#[test]
fn clock_overflow_does_not_partially_mutate() {
    let clock = FakeClock::with_reading(UNIX_EPOCH, Duration::MAX);
    let before = clock.reading().expect("clock must be readable");
    let error =
        clock.advance(Duration::from_nanos(1)).expect_err("monotonic overflow must be rejected");
    assert_eq!(error, ClockError::Overflow { component: ClockComponent::Monotonic });
    assert_eq!(clock.reading().expect("failed clock must remain readable"), before);
}

#[test]
fn identifier_bytes_are_namespaced_big_endian_and_never_wrap() {
    let namespace = *b"peritus!";
    let mut ids = DeterministicIdSource::new(namespace);
    let first = ids.next_bytes().expect("first ID bytes must exist");
    assert_eq!(&first[..8], &namespace);
    assert_eq!(&first[8..], &1_u64.to_be_bytes());
    assert_eq!(ids.peek_bytes().expect("second bytes must exist")[8..], 2_u64.to_be_bytes());
    assert_eq!(ids.issued(), 1);

    let maximum = NonZeroU64::new(u64::MAX).expect("maximum is nonzero");
    let mut last = DeterministicIdSource::starting_at([0; 8], maximum);
    assert_ne!(last.next_bytes().expect("maximum must be emitted"), [0; 16]);
    assert_eq!(last.next_bytes(), Err(IdSourceError::Exhausted));
    assert_eq!(last.issued(), 1);
}

#[test]
fn every_a1_identifier_constructor_accepts_exact_sequence_bytes() {
    let mut ids = DeterministicIdSource::new(*b"a1-types");
    let values = vec![
        ids.next(ProjectId::new).expect("ProjectId").into_bytes(),
        ids.next(AcceptanceSpecId::new).expect("AcceptanceSpecId").into_bytes(),
        ids.next(HarnessId::new).expect("HarnessId").into_bytes(),
        ids.next(SessionId::new).expect("SessionId").into_bytes(),
        ids.next(RunId::new).expect("RunId").into_bytes(),
        ids.next(AttemptId::new).expect("AttemptId").into_bytes(),
        ids.next(TurnId::new).expect("TurnId").into_bytes(),
        ids.next(ActionId::new).expect("ActionId").into_bytes(),
        ids.next(WorkspaceId::new).expect("WorkspaceId").into_bytes(),
        ids.next(SnapshotId::new).expect("SnapshotId").into_bytes(),
        ids.next(ActorId::new).expect("ActorId").into_bytes(),
        ids.next(EnvironmentId::new).expect("EnvironmentId").into_bytes(),
        ids.next(ResourceId::new).expect("ResourceId").into_bytes(),
        ids.next(PolicyId::new).expect("PolicyId").into_bytes(),
        ids.next(ProviderProfileId::new).expect("ProviderProfileId").into_bytes(),
        ids.next(CommandId::new).expect("CommandId").into_bytes(),
        ids.next(EventId::new).expect("EventId").into_bytes(),
        ids.next(ProcessId::new).expect("ProcessId").into_bytes(),
        ids.next(ArtifactId::new).expect("ArtifactId").into_bytes(),
        ids.next(EvidenceId::new).expect("EvidenceId").into_bytes(),
        ids.next(EvaluationCampaignId::new).expect("EvaluationCampaignId").into_bytes(),
        ids.next(EvolutionCampaignId::new).expect("EvolutionCampaignId").into_bytes(),
        ids.next(GateId::new).expect("GateId").into_bytes(),
        ids.next(GateExecutionId::new).expect("GateExecutionId").into_bytes(),
        ids.next(ReviewCycleId::new).expect("ReviewCycleId").into_bytes(),
        ids.next(FindingId::new).expect("FindingId").into_bytes(),
        ids.next(ApprovalRequestId::new).expect("ApprovalRequestId").into_bytes(),
        ids.next(BudgetId::new).expect("BudgetId").into_bytes(),
        ids.next(BudgetReservationId::new).expect("BudgetReservationId").into_bytes(),
    ];
    assert_eq!(values.len(), 29);
    for (index, value) in values.iter().enumerate() {
        assert_eq!(&value[..8], b"a1-types");
        assert_eq!(&value[8..], &u64::try_from(index + 1).expect("small index").to_be_bytes());
    }
}

#[test]
fn rejected_identifier_bytes_are_reserved() {
    let mut ids = DeterministicIdSource::new(*b"rejected");
    let error = ids
        .next(|_| Err::<(), _>(IdentifierError::Zero))
        .expect_err("caller rejection must surface");
    assert_eq!(error, IdSourceError::IdentifierRejected(IdentifierError::Zero));
    assert_eq!(
        &ids.next_bytes().expect("next value must remain available")[8..],
        &2_u64.to_be_bytes()
    );
}

#[test]
fn event_builder_is_per_aggregate_and_exhausts_without_wrapping() {
    let ids = DeterministicIdSource::new(*b"events!!");
    let mut builder = EventFixtureBuilder::new(ids);
    let first = builder.next_context().expect("first context must exist");
    assert_eq!(first.sequence(), EventSequence::first());
    assert_eq!(&first.event_id().into_bytes()[8..], &1_u64.to_be_bytes());

    let maximum = EventSequence::new(u64::MAX).expect("maximum is valid");
    let ids = DeterministicIdSource::starting_at(
        *b"events!!",
        NonZeroU64::new(u64::MAX).expect("maximum is nonzero"),
    );
    let mut last = EventFixtureBuilder::starting_at(ids, maximum);
    let context = last.next_context().expect("maximum context must be emitted");
    assert_eq!(context.sequence(), maximum);
    assert_eq!(last.next_context(), Err(EventFixtureError::SequenceExhausted));
}

#[test]
fn identifier_exhaustion_does_not_advance_event_sequence() {
    let ids = DeterministicIdSource::starting_at(
        *b"events!!",
        NonZeroU64::new(u64::MAX).expect("maximum is nonzero"),
    );
    let mut builder = EventFixtureBuilder::new(ids);
    builder.next_context().expect("last identifier must be emitted");
    assert_eq!(
        builder.peek_sequence().expect("second sequence remains"),
        EventSequence::new(2).expect("two is valid")
    );
    assert_eq!(
        builder.next_context(),
        Err(EventFixtureError::Identifier(IdSourceError::Exhausted))
    );
    assert_eq!(
        builder.peek_sequence().expect("failed ID must not advance"),
        EventSequence::new(2).expect("two is valid")
    );
}

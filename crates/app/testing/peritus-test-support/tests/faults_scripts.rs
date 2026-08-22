//! Boundary tests for occurrence-addressed faults and exact one-shot scripts.

use peritus_test_support::{
    ExpectedCall, FakeProvider, FakeTool, FaultLabel, FaultNameError, FaultPlan, FaultPlanError,
    FaultPoint, FaultVerificationError, ScriptViolationKind, ScriptedCalls, ScriptedStream,
};
use std::num::NonZeroU64;

const fn nonzero(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).expect("test occurrence must be nonzero")
}

#[test]
fn fault_names_are_strict_and_duplicate_schedules_fail() {
    assert_eq!(FaultPoint::new(""), Err(FaultNameError::Empty));
    assert_eq!(FaultPoint::new("journal..write"), Err(FaultNameError::EmptySegment));
    assert_eq!(FaultLabel::new("DiskFull"), Err(FaultNameError::InvalidSegmentStart));

    let point = FaultPoint::new("journal.before-write").expect("valid point");
    let label = FaultLabel::new("disk-full").expect("valid label");
    let mut plan = FaultPlan::new();
    plan.schedule(point.clone(), nonzero(2), label.clone()).expect("first schedule must work");
    let duplicate =
        plan.schedule(point, nonzero(2), label).expect_err("duplicate must not overwrite");
    assert!(matches!(duplicate, FaultPlanError::Duplicate { .. }));
}

#[test]
fn fault_occurrences_are_per_point_one_shot_and_verified() {
    let write = FaultPoint::new("journal.write").expect("valid point");
    let sync = FaultPoint::new("journal.sync").expect("valid point");
    let mut plan = FaultPlan::new();
    plan.schedule(write.clone(), nonzero(2), FaultLabel::new("disk-full").expect("valid label"))
        .expect("schedule must work");
    plan.schedule(sync.clone(), nonzero(1), FaultLabel::new("interrupted").expect("valid label"))
        .expect("schedule must work");
    let injector = plan.injector();
    let shared = injector.clone();
    assert!(injector.check(&write).expect("check must work").is_none());
    let hit = shared.check(&write).expect("shared check must work").expect("second write must hit");
    assert_eq!(hit.occurrence(), nonzero(2));
    assert!(injector.check(&write).expect("third check must work").is_none());
    assert!(matches!(injector.verify_all_triggered(), Err(FaultVerificationError::Missed { .. })));
    injector.check(&sync).expect("sync check must work");
    injector.verify_all_triggered().expect("all scheduled faults must now be observed");
    let snapshot = injector.snapshot().expect("snapshot must work");
    assert_eq!(snapshot.call_count(&write), 3);
    assert_eq!(snapshot.call_count(&sync), 1);
    assert_eq!(snapshot.triggered().len(), 2);

    let fork = injector.fork().expect("fork must work");
    assert_eq!(fork.snapshot().expect("fork snapshot").call_count(&write), 0);
}

#[test]
fn mismatch_is_recorded_without_consuming_outcome() {
    let mut script = ScriptedCalls::new([
        ExpectedCall::new("expected", 7_u8),
        ExpectedCall::new("second", 9_u8),
    ]);
    let error = script.respond("wrong").expect_err("mismatch must be typed");
    assert_eq!(error.kind(), ScriptViolationKind::RequestMismatch);
    assert_eq!(script.remaining(), 2);
    assert_eq!(script.peek_expected(), Some(&"expected"));
    assert!(!script.observed()[0].matched());

    assert_eq!(script.respond("expected").expect("correction must consume"), 7);
    assert_eq!(script.respond("second").expect("second must consume"), 9);
    script.verify_complete().expect("script must be complete");
    let unexpected = script.respond("late").expect_err("post-script call must fail");
    assert_eq!(unexpected.kind(), ScriptViolationKind::UnexpectedCall);
}

#[test]
fn caller_errors_and_stream_control_steps_remain_protocol_owned() {
    let mut provider =
        FakeProvider::new([ExpectedCall::new("request", Err::<u8, _>("rate-limited"))]);
    let outcome = provider.response_for("request").expect("matching request is not a script error");
    assert_eq!(outcome, Err("rate-limited"));
    provider.verify_complete().expect("provider must be complete");

    let mut tool = FakeTool::new([ExpectedCall::new("call", Ok::<_, &str>("done"))]);
    assert_eq!(tool.outcome_for("call").expect("tool script must match"), Ok("done"));
    tool.verify_complete().expect("tool must be complete");

    let mut stream = ScriptedStream::new([Ok(1_u8), Ok(1_u8), Err(5_u8)]);
    assert_eq!(stream.next_step().expect("step one"), Some(Ok(1)));
    assert_eq!(stream.next_step().expect("step two"), Some(Ok(1)));
    assert_eq!(stream.next_step().expect("error control step"), Some(Err(5)));
    assert_eq!(stream.next_step().expect("end"), None);
    assert_eq!(stream.consumed(), 3);
    stream.verify_complete().expect("stream must be complete");
}

#[test]
fn incomplete_scripts_report_exact_remaining_count() {
    let script = ScriptedCalls::new([ExpectedCall::new(1_u8, 2_u8), ExpectedCall::new(3, 4)]);
    assert_eq!(script.verify_complete().expect_err("unconsumed script must fail").remaining(), 2);
    let stream = ScriptedStream::new([1_u8, 2, 3]);
    assert_eq!(stream.verify_complete().expect_err("unconsumed stream must fail").remaining(), 3);
}

use std::{
    future::{Future, pending},
    time::Duration,
};

use peritus_scheduler::{DispatchId, WorkId, WorkerId};
use peritus_types::Sha256Digest;
use tokio::{runtime::Builder, sync::oneshot};

use super::{
    WorkerAssignment, WorkerCancelDisposition, WorkerCancellationReason, WorkerFailureKind,
    WorkerShutdownDisposition, WorkerSupervisor, WorkerSupervisorErrorKind, WorkerSupervisorLimits,
    WorkerSupervisorPhase, WorkerTaskOutcome,
};

const ASYNC_TEST_BOUND: Duration = Duration::from_secs(1);
const SHUTDOWN_GRACE: Duration = Duration::from_millis(20);

#[test]
fn rejects_duplicate_dispatch_and_active_task_capacity() {
    run_async_test(rejects_duplicate_dispatch_and_active_task_capacity_async());
}

async fn rejects_duplicate_dispatch_and_active_task_capacity_async() {
    let mut supervisor = WorkerSupervisor::new(limits(2, 2, 2));

    supervisor
        .spawn(assignment(1), |_| async { pending::<WorkerTaskOutcome>().await })
        .expect("first task is admitted");

    let duplicate = supervisor
        .spawn(assignment(1), |_| async { completed(91) })
        .expect_err("an owned dispatch cannot be admitted twice");
    assert_eq!(duplicate.kind(), WorkerSupervisorErrorKind::DuplicateDispatch);

    supervisor
        .spawn(assignment(2), |_| async { pending::<WorkerTaskOutcome>().await })
        .expect("second task fills the active-task capacity");
    let capacity = supervisor
        .spawn(assignment(3), |_| async { completed(93) })
        .expect_err("work beyond active-task capacity must be rejected");
    assert_eq!(capacity.kind(), WorkerSupervisorErrorKind::Capacity);

    let snapshot = supervisor.snapshot();
    assert_eq!(snapshot.phase(), WorkerSupervisorPhase::Accepting);
    assert_eq!(snapshot.counts().active_tasks(), 2);
    assert_eq!(snapshot.counts().running_tasks(), 2);
    assert_eq!(snapshot.counts().pending_observations(), 0);
}

#[test]
fn cancellation_retains_and_delivers_the_first_reason() {
    run_async_test(cancellation_retains_and_delivers_the_first_reason_async());
}

async fn cancellation_retains_and_delivers_the_first_reason_async() {
    let mut supervisor = WorkerSupervisor::new(limits(1, 1, 1));
    let target = assignment(10);
    let (reason_sender, reason_receiver) = oneshot::channel();
    let (release_sender, release_receiver) = oneshot::channel();

    supervisor
        .spawn(target, move |mut cancellation| async move {
            let reason = cancellation.cancelled().await;
            reason_sender.send(reason).expect("test receives cancellation reason");
            release_receiver.await.expect("test releases cancelled worker");
            WorkerTaskOutcome::Cancelled(reason)
        })
        .expect("cancellable task is admitted");

    assert_eq!(
        supervisor.cancel(target.dispatch_id(), WorkerCancellationReason::Scheduler),
        Ok(WorkerCancelDisposition::Requested)
    );
    assert_eq!(
        supervisor.cancel(target.dispatch_id(), WorkerCancellationReason::User),
        Ok(WorkerCancelDisposition::AlreadyRequested(WorkerCancellationReason::Scheduler))
    );
    let delivered = tokio::time::timeout(ASYNC_TEST_BOUND, reason_receiver)
        .await
        .expect("worker observes cancellation within the test bound")
        .expect("worker sends its observed cancellation reason");
    assert_eq!(delivered, WorkerCancellationReason::Scheduler);
    assert_eq!(supervisor.snapshot().counts().cancellation_requested(), 1);

    release_sender.send(()).expect("cancelled worker remains live");
    let report = tokio::time::timeout(ASYNC_TEST_BOUND, supervisor.shutdown())
        .await
        .expect("shutdown joins the released worker within the test bound");
    assert_eq!(report.cancellation_requests(), 0);
    assert_eq!(report.abort_requests(), 0);
    assert!(report.remaining().is_empty());
    assert_eq!(report.observations().len(), 1);
    assert_eq!(report.observations()[0].assignment(), target);
    assert_eq!(
        report.observations()[0].outcome(),
        WorkerTaskOutcome::Cancelled(WorkerCancellationReason::Scheduler)
    );
}

#[test]
fn reap_and_result_draining_preserve_bounds_and_order() {
    run_async_test(reap_and_result_draining_preserve_bounds_and_order_async());
}

async fn reap_and_result_draining_preserve_bounds_and_order_async() {
    let mut supervisor = WorkerSupervisor::new(limits(2, 2, 2));
    let first = assignment(20);
    let second = assignment(21);
    let third = assignment(22);

    supervisor.spawn(first, |_| async { completed(20) }).expect("first completed task is admitted");
    wait_for_completed(&supervisor, 1).await;
    let first_reap = supervisor.reap(usize::MAX).await.expect("first reap succeeds");
    assert_eq!(first_reap.reaped(), 1);
    assert_eq!(first_reap.active_remaining(), 0);
    assert!(!first_reap.result_capacity_blocked());

    let pending_duplicate = supervisor
        .spawn(first, |_| async { completed(90) })
        .expect_err("a dispatch awaiting result settlement remains owned");
    assert_eq!(pending_duplicate.kind(), WorkerSupervisorErrorKind::DuplicateDispatch);

    supervisor
        .spawn(second, |_| async { completed(21) })
        .expect("second completed task is admitted");
    supervisor.spawn(third, |_| async { completed(22) }).expect("third completed task is admitted");
    wait_for_completed(&supervisor, 2).await;

    let blocked_reap = supervisor.reap(usize::MAX).await.expect("bounded reap succeeds");
    assert_eq!(blocked_reap.reaped(), 1);
    assert_eq!(blocked_reap.active_remaining(), 1);
    assert!(blocked_reap.result_capacity_blocked());
    assert_eq!(supervisor.snapshot().counts().pending_observations(), 2);

    let drained_first = supervisor.drain_results(1).expect("bounded result drain succeeds");
    assert_eq!(drained_first.len(), 1);
    assert_eq!(drained_first[0].assignment(), first);
    assert_eq!(drained_first[0].outcome(), completed(20));

    let final_reap = supervisor.reap(usize::MAX).await.expect("capacity resumes reaping");
    assert_eq!(final_reap.reaped(), 1);
    assert_eq!(final_reap.active_remaining(), 0);
    assert!(!final_reap.result_capacity_blocked());

    let drained = supervisor.drain_results(usize::MAX).expect("all remaining observations drain");
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].assignment(), second);
    assert_eq!(drained[0].outcome(), completed(21));
    assert_eq!(drained[1].assignment(), third);
    assert_eq!(drained[1].outcome(), completed(22));

    let snapshot = supervisor.snapshot();
    assert_eq!(snapshot.counts().active_tasks(), 0);
    assert_eq!(snapshot.counts().pending_observations(), 0);
    assert!(snapshot.remaining().is_empty());
}

#[test]
fn panic_is_normalized_to_a_redaction_safe_failure() {
    run_async_test(panic_is_normalized_to_a_redaction_safe_failure_async());
}

async fn panic_is_normalized_to_a_redaction_safe_failure_async() {
    let mut supervisor = WorkerSupervisor::new(limits(1, 1, 1));
    let target = assignment(30);

    supervisor.spawn(target, |_| panic_outcome()).expect("panicking task is admitted");
    wait_for_completed(&supervisor, 1).await;
    let reap = supervisor.reap(1).await.expect("panicking task is reaped");
    assert_eq!(reap.reaped(), 1);

    let observations = supervisor.drain_results(1).expect("panic observation drains");
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].assignment(), target);
    assert_eq!(
        observations[0].outcome(),
        WorkerTaskOutcome::Failed {
            kind: WorkerFailureKind::SupervisorPanicked,
            evidence_digest: None,
        }
    );
}

#[test]
fn shutdown_reports_cooperative_cancellation_and_forced_abort_exactly() {
    run_async_test(shutdown_reports_cooperative_cancellation_and_forced_abort_exactly_async());
}

async fn shutdown_reports_cooperative_cancellation_and_forced_abort_exactly_async() {
    let mut supervisor = WorkerSupervisor::new(limits(2, 2, 2));
    let cooperative = assignment(40);
    let stubborn = assignment(41);
    let (cooperative_started_sender, cooperative_started_receiver) = oneshot::channel();
    let (stubborn_started_sender, stubborn_started_receiver) = oneshot::channel();

    supervisor
        .spawn(cooperative, move |mut cancellation| async move {
            let _ = cooperative_started_sender.send(());
            WorkerTaskOutcome::Cancelled(cancellation.cancelled().await)
        })
        .expect("cooperative task is admitted");
    supervisor
        .spawn(stubborn, move |_| async move {
            let _ = stubborn_started_sender.send(());
            pending::<WorkerTaskOutcome>().await
        })
        .expect("stubborn task is admitted");

    tokio::time::timeout(ASYNC_TEST_BOUND, async {
        cooperative_started_receiver.await.expect("cooperative task starts");
        stubborn_started_receiver.await.expect("stubborn task starts");
    })
    .await
    .expect("both workers start within the test bound");

    let report = tokio::time::timeout(ASYNC_TEST_BOUND, supervisor.shutdown())
        .await
        .expect("bounded shutdown returns without waiting for stubborn work");
    assert_eq!(report.disposition(), WorkerShutdownDisposition::Unclean);
    assert_eq!(report.cancellation_requests(), 2);
    assert_eq!(report.abort_requests(), 1);
    assert!(report.remaining().is_empty());
    assert_eq!(report.observations().len(), 2);
    assert_eq!(report.observations()[0].assignment(), cooperative);
    assert_eq!(
        report.observations()[0].outcome(),
        WorkerTaskOutcome::Cancelled(WorkerCancellationReason::Shutdown)
    );
    assert_eq!(report.observations()[1].assignment(), stubborn);
    assert_eq!(
        report.observations()[1].outcome(),
        WorkerTaskOutcome::Failed {
            kind: WorkerFailureKind::SupervisorAborted,
            evidence_digest: None,
        }
    );
    assert_eq!(supervisor.snapshot().phase(), WorkerSupervisorPhase::Stopped);
}

#[test]
fn empty_shutdown_is_clean_and_immediate() {
    run_async_test(empty_shutdown_is_clean_and_immediate_async());
}

async fn empty_shutdown_is_clean_and_immediate_async() {
    let mut supervisor = WorkerSupervisor::new(limits(1, 1, 1));

    let report = tokio::time::timeout(ASYNC_TEST_BOUND, supervisor.shutdown())
        .await
        .expect("empty shutdown completes within the test bound");
    assert_eq!(report.disposition(), WorkerShutdownDisposition::Clean);
    assert_eq!(report.cancellation_requests(), 0);
    assert_eq!(report.abort_requests(), 0);
    assert!(report.observations().is_empty());
    assert!(report.remaining().is_empty());
    assert_eq!(supervisor.snapshot().phase(), WorkerSupervisorPhase::Stopped);
}

fn run_async_test(test: impl Future<Output = ()>) {
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build current-thread test runtime");
    runtime.block_on(test);
}

fn limits(
    maximum_active_tasks: usize,
    maximum_results: usize,
    maximum_reap_per_pass: usize,
) -> WorkerSupervisorLimits {
    WorkerSupervisorLimits::new(
        maximum_active_tasks,
        maximum_results,
        maximum_reap_per_pass,
        SHUTDOWN_GRACE,
        SHUTDOWN_GRACE,
    )
    .expect("test worker limits are valid")
}

fn assignment(seed: u8) -> WorkerAssignment {
    WorkerAssignment::new(
        WorkId::new([seed; WorkId::LENGTH]).expect("test work identity is nonzero"),
        DispatchId::new([seed; DispatchId::LENGTH]).expect("test dispatch identity is nonzero"),
        WorkerId::new([seed; WorkerId::LENGTH]).expect("test worker identity is nonzero"),
    )
}

fn completed(seed: u8) -> WorkerTaskOutcome {
    WorkerTaskOutcome::Completed { result_digest: Sha256Digest::new([seed; 32]) }
}

async fn panic_outcome() -> WorkerTaskOutcome {
    panic!("private worker panic payload")
}

async fn wait_for_completed(supervisor: &WorkerSupervisor, expected: usize) {
    tokio::time::timeout(ASYNC_TEST_BOUND, async {
        loop {
            if supervisor.snapshot().counts().completed_awaiting_reap() == expected {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("workers complete within the test bound");
}

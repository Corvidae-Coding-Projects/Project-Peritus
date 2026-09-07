//! Deterministic virtual-clock regressions for load-window completion.

use std::cell::Cell;
use std::convert::Infallible;
use std::rc::Rc;
use std::time::Duration;

use peritus_benchmarks::{
    AccountingSink, CapacityLimits, ConcurrencyLimits, MeasurementIngestor, MeasurementSink,
    PlanKind, QualificationPlan, QualificationRunner, QualificationSubject, QueueLimits,
    ResourceAccountant, ResourceEnvelope, RunContext, RunnerDescriptor, RunnerTermination,
    ScenarioKind, Sha256Digest, StableId, SubjectDescriptor, Workload, WorkloadParameters,
};

use super::PacedRunner;
use crate::CancellationFlag;

struct Subject {
    descriptor: SubjectDescriptor,
    steps: u64,
    clock: Rc<Cell<Duration>>,
    operation_duration: Duration,
}

impl QualificationSubject for Subject {
    type Authorization = ();
    type Error = Infallible;

    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn execute_step(
        &mut self,
        _authorization: &Self::Authorization,
        _context: &RunContext,
        _step: &peritus_benchmarks::PlanStep,
        _measurements: &mut dyn MeasurementSink,
        _accounting: &mut dyn AccountingSink,
    ) -> Result<(), Self::Error> {
        self.steps += 1;
        self.clock.set(self.clock.get() + self.operation_duration);
        Ok(())
    }
}

#[test]
fn one_step_plan_completes_with_exact_receipt() {
    let mut fixture = fixture();
    let receipt = run_virtual(&mut fixture);
    assert_eq!(receipt.termination(), RunnerTermination::Completed);
    assert_eq!(receipt.executed_steps(), 1);
    assert_eq!(fixture.subject.steps, 1);
}

#[test]
fn pre_requested_cancellation_produces_cancelled_receipt() {
    let mut fixture = fixture();
    fixture.runner.cancellation().cancel();
    let receipt = QualificationRunner::run(
        &mut fixture.runner,
        &mut fixture.subject,
        &(),
        &fixture.context,
        &fixture.plan,
        &mut fixture.measurements,
        &mut fixture.accounting,
    )
    .expect("cancelled receipt");
    assert_eq!(receipt.termination(), RunnerTermination::Cancelled);
    assert_eq!(receipt.executed_steps(), 0);
    assert_eq!(fixture.subject.steps, 0);
}

struct Fixture {
    subject: Subject,
    runner: PacedRunner,
    context: RunContext,
    plan: QualificationPlan,
    measurements: MeasurementIngestor,
    accounting: ResourceAccountant,
    clock: Rc<Cell<Duration>>,
}

fn fixture() -> Fixture {
    fixture_with(1, Duration::ZERO)
}

fn fixture_with(operations_per_second: u32, operation_duration: Duration) -> Fixture {
    let clock = Rc::new(Cell::new(Duration::ZERO));
    let profile = id("profile");
    let workload = Workload::new(
        id("workload"),
        "single operation",
        ScenarioKind::EventAppend,
        WorkloadParameters::load(1, operations_per_second, 1).expect("parameters"),
    )
    .expect("workload");
    let envelope = ResourceEnvelope::new(
        ConcurrencyLimits::new(1, 1, 1).expect("concurrency"),
        CapacityLimits::new(1024, 1024, 1024).expect("capacity"),
        QueueLimits::new(1, 1, 1, 1).expect("queues"),
    );
    let plan =
        QualificationPlan::new(id("plan"), PlanKind::Load, profile.clone(), envelope, workload)
            .expect("plan");
    let run = id("run");
    Fixture {
        subject: Subject {
            descriptor: SubjectDescriptor::new(
                id("subject"),
                "test",
                Sha256Digest::of_bytes(b"subject"),
            )
            .expect("subject descriptor"),
            steps: 0,
            clock: Rc::clone(&clock),
            operation_duration,
        },
        runner: PacedRunner::new(
            RunnerDescriptor::new(id("runner"), "test", Sha256Digest::of_bytes(b"runner"))
                .expect("runner descriptor"),
            CancellationFlag::new(),
        ),
        context: RunContext::new(run.clone(), profile.clone(), id("plan")),
        plan,
        measurements: MeasurementIngestor::new(run, profile, [id("workload")], 1)
            .expect("measurements"),
        accounting: ResourceAccountant::new(envelope),
        clock,
    }
}

#[test]
fn expired_load_window_does_not_drain_backlog_and_claim_completion() {
    let mut fixture = fixture_with(3, Duration::from_secs(1));
    let receipt = run_virtual(&mut fixture);
    assert_eq!(receipt.termination(), RunnerTermination::Failed);
    assert_eq!(receipt.expected_steps(), 3);
    assert_eq!(receipt.executed_steps(), 1);
    assert_eq!(fixture.subject.steps, 1);
    assert!(!receipt.failures().is_empty());
}

#[test]
fn successful_run_observes_the_complete_declared_horizon() {
    let mut fixture = fixture_with(1, Duration::ZERO);
    let receipt = run_virtual(&mut fixture);
    assert!(receipt.completed());
    assert_eq!(fixture.clock.get(), Duration::from_secs(1));
}

#[test]
fn operation_started_inside_window_may_finish_after_its_end() {
    let mut fixture = fixture_with(2, Duration::from_millis(600));
    let receipt = run_virtual(&mut fixture);
    assert!(receipt.completed());
    assert_eq!(receipt.executed_steps(), 2);
    assert_eq!(fixture.clock.get(), Duration::from_millis(1200));
}

#[test]
fn wait_overshooting_the_window_does_not_start_another_operation() {
    let mut fixture = fixture_with(2, Duration::ZERO);
    let receipt = run_virtual_wait(&mut fixture, |duration, clock, _| {
        if duration.is_zero() {
            return true;
        }
        clock.set(clock.get() + duration + Duration::from_secs(1));
        true
    });
    assert_eq!(receipt.termination(), RunnerTermination::Failed);
    assert_eq!(receipt.executed_steps(), 1);
    assert_eq!(fixture.subject.steps, 1);
}

#[test]
fn cancellation_during_final_idle_tail_is_not_a_completed_horizon() {
    let mut fixture = fixture_with(1, Duration::ZERO);
    let receipt = run_virtual_wait(&mut fixture, |duration, clock, cancellation| {
        if duration.is_zero() {
            return true;
        }
        clock.set(Duration::from_millis(250));
        cancellation.cancel();
        false
    });
    assert_eq!(receipt.termination(), RunnerTermination::Cancelled);
    assert_eq!(receipt.executed_steps(), receipt.expected_steps());
    assert!(!receipt.completed());
    assert_eq!(fixture.clock.get(), Duration::from_millis(250));
    assert!(!receipt.failures().is_empty());
}

fn run_virtual(fixture: &mut Fixture) -> peritus_benchmarks::RunnerReceipt {
    run_virtual_wait(fixture, |duration, clock, _| {
        clock.set(clock.get() + duration);
        true
    })
}

fn run_virtual_wait(
    fixture: &mut Fixture,
    mut wait: impl FnMut(Duration, &Cell<Duration>, &CancellationFlag) -> bool,
) -> peritus_benchmarks::RunnerReceipt {
    let clock = Rc::clone(&fixture.clock);
    let mut invocation = super::Invocation {
        subject: &mut fixture.subject,
        authorization: &(),
        context: &fixture.context,
        plan: &fixture.plan,
        measurements: &mut fixture.measurements,
        accounting: &mut fixture.accounting,
    };
    fixture
        .runner
        .execute(
            &mut invocation,
            || clock.get(),
            |duration, cancellation, _| wait(duration, &clock, cancellation),
        )
        .expect("virtual runner receipt")
}

fn id(value: &str) -> StableId {
    StableId::new(value).expect("stable id")
}

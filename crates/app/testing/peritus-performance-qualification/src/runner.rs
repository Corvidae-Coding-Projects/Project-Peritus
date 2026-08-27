//! Monotonic production pacing and complete terminal receipts.

use std::thread;
use std::time::{Duration, Instant};

use peritus_benchmarks::{
    AccountingSink, MeasurementSink, QualificationPlan, QualificationRunner, QualificationSubject,
    RunContext, RunnerDescriptor, RunnerReceipt, RunnerTermination,
};

use crate::{CancellationFlag, RunnerError};

const DEFAULT_CANCELLATION_POLL: Duration = Duration::from_millis(100);

/// Wall-clock qualification runner for one disposable integrated subject.
pub struct PacedRunner {
    descriptor: RunnerDescriptor,
    cancellation: CancellationFlag,
    cancellation_poll: Duration,
}

impl PacedRunner {
    /// Creates a production-paced runner with a shared cancellation flag.
    #[must_use]
    pub const fn new(descriptor: RunnerDescriptor, cancellation: CancellationFlag) -> Self {
        Self { descriptor, cancellation, cancellation_poll: DEFAULT_CANCELLATION_POLL }
    }

    /// Returns the cooperative cancellation flag used by this runner.
    #[must_use]
    pub const fn cancellation(&self) -> &CancellationFlag {
        &self.cancellation
    }

    fn execute<S>(
        &self,
        invocation: &mut Invocation<'_, S>,
        mut wait: impl FnMut(Duration, &CancellationFlag, Duration) -> bool,
    ) -> Result<RunnerReceipt, RunnerError<S::Error>>
    where
        S: QualificationSubject,
    {
        let origin = Instant::now();
        let mut executed = 0_u64;
        for step in invocation.plan {
            let target = Duration::from_micros(step.offset_micros());
            let elapsed = origin.elapsed();
            if let Some(remaining) = target.checked_sub(elapsed)
                && !wait(remaining, &self.cancellation, self.cancellation_poll)
            {
                return receipt(
                    invocation.context,
                    invocation.plan,
                    executed,
                    RunnerTermination::Cancelled,
                )
                .map_err(RunnerError::Receipt);
            }
            if self.cancellation.is_cancelled() {
                return receipt(
                    invocation.context,
                    invocation.plan,
                    executed,
                    RunnerTermination::Cancelled,
                )
                .map_err(RunnerError::Receipt);
            }
            invocation
                .subject
                .execute_step(
                    invocation.authorization,
                    invocation.context,
                    &step,
                    invocation.measurements,
                    invocation.accounting,
                )
                .map_err(|source| RunnerError::Subject { step: step.sequence(), source })?;
            executed = executed
                .checked_add(1)
                .expect("executed steps cannot exceed the validated plan step count");
        }
        receipt(invocation.context, invocation.plan, executed, RunnerTermination::Completed)
            .map_err(RunnerError::Receipt)
    }
}

impl<S> QualificationRunner<S> for PacedRunner
where
    S: QualificationSubject,
{
    type Error = RunnerError<S::Error>;

    fn descriptor(&self) -> &RunnerDescriptor {
        &self.descriptor
    }

    fn run(
        &mut self,
        subject: &mut S,
        authorization: &S::Authorization,
        context: &RunContext,
        plan: &QualificationPlan,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<RunnerReceipt, Self::Error> {
        let mut invocation =
            Invocation { subject, authorization, context, plan, measurements, accounting };
        self.execute(&mut invocation, wait_monotonic)
    }
}

struct Invocation<'a, S>
where
    S: QualificationSubject,
{
    subject: &'a mut S,
    authorization: &'a S::Authorization,
    context: &'a RunContext,
    plan: &'a QualificationPlan,
    measurements: &'a mut dyn MeasurementSink,
    accounting: &'a mut dyn AccountingSink,
}

fn wait_monotonic(duration: Duration, cancellation: &CancellationFlag, poll: Duration) -> bool {
    let deadline = Instant::now() + duration;
    loop {
        if cancellation.is_cancelled() {
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return true;
        }
        thread::sleep((deadline - now).min(poll));
    }
}

fn receipt(
    context: &RunContext,
    plan: &QualificationPlan,
    executed: u64,
    termination: RunnerTermination,
) -> Result<RunnerReceipt, peritus_benchmarks::QualificationError> {
    RunnerReceipt::new(
        context.run_id().clone(),
        context.plan_id().clone(),
        plan.workload().id().clone(),
        plan.step_count(),
        executed,
        termination,
        Vec::new(),
    )
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

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
            Ok(())
        }
    }

    #[test]
    fn one_step_plan_completes_with_exact_receipt() {
        let mut fixture = fixture();
        let mut subject = fixture.subject;
        let runner = fixture.runner;
        let mut invocation = super::Invocation {
            subject: &mut subject,
            authorization: &(),
            context: &fixture.context,
            plan: &fixture.plan,
            measurements: &mut fixture.measurements,
            accounting: &mut fixture.accounting,
        };
        let receipt = runner.execute(&mut invocation, |_, _, _| true).expect("one step plan");
        assert_eq!(receipt.termination(), RunnerTermination::Completed);
        assert_eq!(receipt.executed_steps(), 1);
        assert_eq!(subject.steps, 1);
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
    }

    fn fixture() -> Fixture {
        let profile = id("profile");
        let workload = Workload::new(
            id("workload"),
            "single operation",
            ScenarioKind::EventAppend,
            WorkloadParameters::load(1, 1, 1).expect("parameters"),
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
        }
    }

    fn id(value: &str) -> StableId {
        StableId::new(value).expect("stable id")
    }
}

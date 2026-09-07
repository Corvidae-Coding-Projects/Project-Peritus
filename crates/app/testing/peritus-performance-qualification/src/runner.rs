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
///
/// Operations may begin only within the declared load window. An operation already in progress
/// may finish after it; unstarted backlog produces a failed receipt instead of extending the load
/// window. Successful execution waits until the declared horizon, including a final idle tail.
/// This synchronous adapter does not by itself establish concurrent offered-load fidelity.
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
        mut elapsed: impl FnMut() -> Duration,
        mut wait: impl FnMut(Duration, &CancellationFlag, Duration) -> bool,
    ) -> Result<RunnerReceipt, RunnerError<S::Error>>
    where
        S: QualificationSubject,
    {
        let horizon =
            Duration::from_secs(invocation.plan.workload().parameters().duration_seconds());
        let mut executed = 0_u64;
        for step in invocation.plan {
            let target = Duration::from_micros(step.offset_micros());
            if let Some(remaining) = target.checked_sub(elapsed())
                && !wait(remaining, &self.cancellation, self.cancellation_poll)
            {
                return receipt(
                    invocation.context,
                    invocation.plan,
                    executed,
                    RunnerTermination::Cancelled,
                    None,
                )
                .map_err(RunnerError::Receipt);
            }
            if self.cancellation.is_cancelled() {
                return receipt(
                    invocation.context,
                    invocation.plan,
                    executed,
                    RunnerTermination::Cancelled,
                    None,
                )
                .map_err(RunnerError::Receipt);
            }
            let observed = elapsed();
            if observed >= horizon {
                return receipt(
                    invocation.context,
                    invocation.plan,
                    executed,
                    RunnerTermination::Failed,
                    Some(format!(
                        "load window ended before step {}: elapsed {} us, declared {} us; unstarted backlog was not executed",
                        step.sequence(), observed.as_micros(), horizon.as_micros(),
                    )),
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
        if let Some(remaining) = horizon.checked_sub(elapsed())
            && !wait(remaining, &self.cancellation, self.cancellation_poll)
        {
            return receipt(
                invocation.context,
                invocation.plan,
                executed,
                RunnerTermination::Cancelled,
                Some("cancelled before the complete declared workload horizon".to_owned()),
            )
            .map_err(RunnerError::Receipt);
        }
        receipt(invocation.context, invocation.plan, executed, RunnerTermination::Completed, None)
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
        let origin = Instant::now();
        self.execute(&mut invocation, || origin.elapsed(), wait_monotonic)
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
    failure: Option<String>,
) -> Result<RunnerReceipt, peritus_benchmarks::QualificationError> {
    RunnerReceipt::new(
        context.run_id().clone(),
        context.plan_id().clone(),
        plan.workload().id().clone(),
        plan.step_count(),
        executed,
        termination,
        failure.into_iter().collect(),
    )
}

#[cfg(test)]
mod tests;

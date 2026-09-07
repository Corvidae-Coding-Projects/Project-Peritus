//! Open-loop event arrivals over a bounded owned pool of public A3 clients.

mod engine;
mod evidence;
mod worker;

use peritus_benchmarks::{
    AccountingSink, MeasurementSink, Metric, QualificationPlan, QualificationRunner, RunContext,
    RunnerDescriptor, RunnerReceipt, RunnerTermination, ScenarioKind,
};

use super::{IntegratedSubject, SubjectAuthorization};
use crate::{CancellationFlag, RunnerError, SubjectError, a3::A3Client, identity::IdentitySource};
use evidence::{EvidenceWriter, Observation};
use worker::Worker;

pub use evidence::EventLoadEvidence;

/// Fixed-arrival event load with exact payloads, bounded ownership, and unsampled evidence.
///
/// Uses the workload's rate, concurrency and queue capacity without response-dependent pacing.
/// Latency includes scheduled waiting, client queueing, D2 prerequisites, and the final commit.
/// It never retries rejected work or starts queued work after the declared horizon.
pub struct EventAppendRunner {
    descriptor: RunnerDescriptor,
    cancellation: CancellationFlag,
    evidence: Option<EventLoadEvidence>,
}

impl EventAppendRunner {
    /// Creates a runner without changing any workload or acceptance limit.
    #[must_use]
    pub const fn new(descriptor: RunnerDescriptor, cancellation: CancellationFlag) -> Self {
        Self { descriptor, cancellation, evidence: None }
    }
    /// Takes the unsampled evidence produced by the last completed invocation.
    pub const fn take_evidence(&mut self) -> Option<EventLoadEvidence> {
        self.evidence.take()
    }

    fn execute(
        &mut self,
        subject: &mut IntegratedSubject,
        authorization: &SubjectAuthorization,
        context: &RunContext,
        plan: &QualificationPlan,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<RunnerReceipt, SubjectError> {
        self.evidence = None;
        if authorization.instance != subject.authorization {
            return Err(SubjectError::AuthorizationMismatch);
        }
        if plan.workload().scenario() != ScenarioKind::EventAppend {
            return Err(SubjectError::Configuration(
                "event runner requires an event-append plan".to_owned(),
            ));
        }
        let concurrency = plan.workload().parameters().max_concurrency();
        if concurrency > 32 {
            return Err(SubjectError::Configuration(
                "event concurrency exceeds the 32-worker daemon configuration".to_owned(),
            ));
        }
        let directory = subject.storage().path().parent().ok_or_else(|| {
            SubjectError::Configuration("subject scratch parent is absent".to_owned())
        })?;
        let mut evidence = EvidenceWriter::new(directory)?;
        let workers = (0..concurrency)
            .map(|_| -> Result<Worker, SubjectError> {
                let namespace = subject.identities.next(peritus_types::SessionId::new)?;
                // A separate high-bit domain cannot overlap the subject's PID namespace.
                let ordinal = u64::from_be_bytes(
                    namespace.as_bytes()[8..].try_into().expect("eight identity bytes"),
                );
                let mut identities = IdentitySource::new(ordinal | (1 << 63));
                let client = A3Client::connect(subject.daemon.endpoint(), None, &mut identities)?;
                Ok(Worker { client, identities, revision: subject.revision })
            })
            .collect::<Result<Vec<_>, _>>()?;
        subject.record_startup(context, measurements)?;
        let mut on_completion = |completion: &worker::Completion| {
            if completion.failure.is_none() {
                subject.measure(
                    context,
                    Metric::EventAppendLatency,
                    completion.finished_us - completion.scheduled_us,
                    measurements,
                )?;
            }
            Ok(())
        };
        let summary = engine::run(
            plan,
            &self.cancellation,
            workers,
            engine::Sinks {
                accounting,
                evidence: &mut evidence,
                on_completion: &mut on_completion,
            },
        )?;
        evidence.record(&Observation::Summary {
            workload_id: context.workload_id().as_str(),
            counters: &summary,
        })?;
        self.evidence = Some(evidence.finish()?);
        let termination = if self.cancellation.is_cancelled() {
            RunnerTermination::Cancelled
        } else if summary.committed == summary.expected {
            RunnerTermination::Completed
        } else {
            RunnerTermination::Failed
        };
        let failures = if termination == RunnerTermination::Completed {
            Vec::new()
        } else {
            vec![format!("event load incomplete: {}", serde_json::to_string(&summary)?)]
        };
        Ok(RunnerReceipt::new(
            context.run_id().clone(),
            context.plan_id().clone(),
            plan.workload().id().clone(),
            summary.expected,
            summary.committed,
            termination,
            failures,
        )?)
    }
}

impl QualificationRunner<IntegratedSubject> for EventAppendRunner {
    type Error = RunnerError<SubjectError>;
    fn descriptor(&self) -> &RunnerDescriptor {
        &self.descriptor
    }
    fn run(
        &mut self,
        subject: &mut IntegratedSubject,
        authorization: &SubjectAuthorization,
        context: &RunContext,
        plan: &QualificationPlan,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<RunnerReceipt, Self::Error> {
        self.execute(subject, authorization, context, plan, measurements, accounting)
            .map_err(RunnerError::EventLoad)
    }
}

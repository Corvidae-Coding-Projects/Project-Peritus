//! Planned-operation dispatch grouped away from subject lifecycle and identity.

use std::fs::OpenOptions;
use std::thread;
use std::time::Instant;

use peritus_benchmarks::{
    AccountingSink, MeasurementSink, Metric, PlanStep, PlannedOperation, QueueKind, ResourceEvent,
    RunContext,
};

use super::IntegratedSubject;
use crate::SubjectError;
use crate::a3::A3Client;
use crate::effects::{
    append_artifact, backpressure_metric, deterministic_provider_chunk, micros, queue_metric,
    throughput,
};
use crate::process::OwnedProcess;

impl IntegratedSubject {
    pub(super) fn execute_operation(
        &mut self,
        context: &RunContext,
        step: &PlanStep,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), SubjectError> {
        match step.operation() {
            PlannedOperation::StartRun { run, memory_bytes, disk_bytes, tokens } => {
                self.start_run(*run, context, measurements)?;
                accounting.apply(ResourceEvent::RunStarted {
                    run: *run,
                    memory_bytes: *memory_bytes,
                    disk_bytes: *disk_bytes,
                    tokens: *tokens,
                })?;
            }
            PlannedOperation::FinishRun { run } => {
                self.finish_run(*run)?;
                accounting.apply(ResourceEvent::RunFinished { run: *run })?;
            }
            PlannedOperation::AppendEvent { .. } => {
                self.append_event(context, step.sequence(), measurements)?;
            }
            PlannedOperation::StartProcess { process, memory_bytes } => {
                self.start_process(*process, *memory_bytes, context, measurements, accounting)?;
            }
            PlannedOperation::StreamTerminal { process, bytes } => {
                self.stream_terminal(*process, *bytes, context, measurements)?;
            }
            PlannedOperation::CancelProcess { process } => {
                self.cancel_process(*process, context, measurements)?;
            }
            PlannedOperation::FinishProcess { process } => {
                self.finish_process(*process, context, measurements, accounting)?;
            }
            PlannedOperation::CrashDaemon { .. } => {
                self.client = None;
                self.daemon.crash()?;
            }
            PlannedOperation::RestartDaemon => {
                self.restart_daemon(context, measurements)?;
            }
            PlannedOperation::Enqueue { queue, count } => {
                self.enqueue(*queue, *count, context, measurements, accounting)?;
            }
            PlannedOperation::Dequeue { queue, count } => {
                self.dequeue(*queue, *count, accounting)?;
            }
            PlannedOperation::DrainQueue { queue, count } => {
                self.drain_queue(*queue, *count, accounting)?;
            }
            PlannedOperation::ObserveBackpressure { queue } => {
                self.observe_backpressure(*queue, context, measurements, accounting)?;
            }
            PlannedOperation::StartProviderRequest { request } => {
                self.start_provider(*request, context, measurements, accounting)?;
            }
            PlannedOperation::AccountTokens { request, tokens } => {
                self.account_tokens(*request, *tokens, context, measurements, accounting)?;
            }
            PlannedOperation::FinishProviderRequest { request } => {
                if !self.provider_requests.remove(request) {
                    return Err(SubjectError::UnexpectedResponse(format!(
                        "plan finished unknown provider request {request}"
                    )));
                }
                accounting.apply(ResourceEvent::ProviderRequestFinished { request: *request })?;
            }
            PlannedOperation::WriteArtifact { bytes } => {
                self.write_artifact(*bytes, context, measurements, accounting)?;
            }
            PlannedOperation::CollectArtifacts => {
                self.collect_artifacts(context, measurements, accounting)?;
            }
            PlannedOperation::SampleResources => {
                self.sample_resources(context, measurements)?;
            }
        }
        Ok(())
    }

    fn enqueue(
        &mut self,
        queue: QueueKind,
        count: u32,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), SubjectError> {
        let depth = self.queue_depths.entry(queue).or_default();
        *depth = depth.checked_add(count).ok_or(SubjectError::IdentityExhausted)?;
        let observed_depth = *depth;
        accounting.apply(ResourceEvent::QueuePushed { queue, count })?;
        self.measure(context, queue_metric(queue), u64::from(observed_depth), measurements)
    }

    fn dequeue(
        &mut self,
        queue: QueueKind,
        count: u32,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), SubjectError> {
        let depth = self.queue_depths.entry(queue).or_default();
        *depth = depth
            .checked_sub(count)
            .ok_or_else(|| SubjectError::UnexpectedResponse("queue plan underflowed".to_owned()))?;
        accounting.apply(ResourceEvent::QueuePopped { queue, count })?;
        Ok(())
    }

    fn drain_queue(
        &mut self,
        queue: QueueKind,
        count: u32,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), SubjectError> {
        let depth = self.queue_depths.entry(queue).or_default();
        if *depth != count {
            return Err(SubjectError::UnexpectedResponse(format!(
                "queue drain expected {count} retained items but observed {depth}"
            )));
        }
        if count != 0 {
            accounting.apply(ResourceEvent::QueuePopped { queue, count })?;
        }
        *depth = 0;
        Ok(())
    }

    fn observe_backpressure(
        &mut self,
        queue: QueueKind,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), SubjectError> {
        let started = Instant::now();
        thread::yield_now();
        let waited = micros(started.elapsed());
        accounting.apply(ResourceEvent::BackpressureObserved { queue, wait_micros: waited })?;
        self.measure(context, backpressure_metric(queue), waited, measurements)
    }

    fn start_process(
        &mut self,
        process: u64,
        memory_bytes: u64,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), SubjectError> {
        if self.processes.insert(process, OwnedProcess::start()?).is_some() {
            return Err(SubjectError::UnexpectedResponse(format!(
                "plan started duplicate process {process}"
            )));
        }
        accounting.apply(ResourceEvent::ProcessStarted { process, memory_bytes })?;
        self.measure(
            context,
            Metric::ConcurrentProcesses,
            u64::try_from(self.processes.len()).unwrap_or(u64::MAX),
            measurements,
        )
    }

    fn stream_terminal(
        &mut self,
        process: u64,
        bytes: u32,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
    ) -> Result<(), SubjectError> {
        let started = Instant::now();
        self.processes
            .get_mut(&process)
            .ok_or_else(|| {
                SubjectError::UnexpectedResponse(format!(
                    "terminal stream named unknown process {process}"
                ))
            })?
            .read_exact(usize::try_from(bytes).unwrap_or(usize::MAX))?;
        self.measure(
            context,
            Metric::TerminalThroughput,
            throughput(u64::from(bytes), started.elapsed()),
            measurements,
        )
    }

    fn cancel_process(
        &mut self,
        process: u64,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
    ) -> Result<(), SubjectError> {
        let started = Instant::now();
        self.processes
            .get_mut(&process)
            .ok_or_else(|| {
                SubjectError::UnexpectedResponse(format!(
                    "cancellation named unknown process {process}"
                ))
            })?
            .terminate()?;
        self.measure(
            context,
            Metric::CancellationLatency,
            micros(started.elapsed()),
            measurements,
        )?;
        self.measure(context, Metric::CancellationSuccessRatio, 10_000, measurements)
    }

    fn finish_process(
        &mut self,
        process: u64,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), SubjectError> {
        let started = Instant::now();
        let mut owned = self.processes.remove(&process).ok_or_else(|| {
            SubjectError::UnexpectedResponse(format!(
                "process finish named unknown process {process}"
            ))
        })?;
        owned.terminate()?;
        accounting.apply(ResourceEvent::ProcessFinished { process })?;
        self.measure(
            context,
            Metric::ProcessThroughput,
            throughput(1, started.elapsed()),
            measurements,
        )
    }

    fn restart_daemon(
        &mut self,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
    ) -> Result<(), SubjectError> {
        let latency = self.daemon.restart()?;
        self.client = Some(A3Client::connect(self.daemon.endpoint(), None, &mut self.identities)?);
        self.measure(context, Metric::RecoveryLatency, micros(latency), measurements)?;
        self.measure(context, Metric::RecoverySuccessRatio, 10_000, measurements)
    }

    fn start_provider(
        &mut self,
        request: u64,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), SubjectError> {
        if !self.provider_requests.insert(request) {
            return Err(SubjectError::UnexpectedResponse(format!(
                "plan started duplicate provider request {request}"
            )));
        }
        accounting.apply(ResourceEvent::ProviderRequestStarted { request })?;
        self.measure(
            context,
            Metric::ConcurrentProviderRequests,
            u64::try_from(self.provider_requests.len()).unwrap_or(u64::MAX),
            measurements,
        )
    }

    fn account_tokens(
        &mut self,
        request: u64,
        tokens: u64,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), SubjectError> {
        if !self.provider_requests.contains(&request) {
            return Err(SubjectError::UnexpectedResponse(format!(
                "tokens named unknown provider request {request}"
            )));
        }
        let started = Instant::now();
        std::hint::black_box(deterministic_provider_chunk(request, tokens));
        accounting.apply(ResourceEvent::TokensConsumed { tokens })?;
        self.measure(
            context,
            Metric::TokenThroughput,
            throughput(tokens, started.elapsed()),
            measurements,
        )?;
        self.measure(context, Metric::TokensConsumed, tokens, measurements)
    }

    fn write_artifact(
        &mut self,
        bytes: u32,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), SubjectError> {
        let started = Instant::now();
        append_artifact(self.daemon.artifact_path(), usize::try_from(bytes).unwrap())?;
        self.artifact_bytes = self
            .artifact_bytes
            .checked_add(u64::from(bytes))
            .ok_or(SubjectError::IdentityExhausted)?;
        accounting.apply(ResourceEvent::DiskRetained { bytes: u64::from(bytes) })?;
        self.measure(
            context,
            Metric::DiskThroughput,
            throughput(u64::from(bytes), started.elapsed()),
            measurements,
        )
    }

    fn collect_artifacts(
        &mut self,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), SubjectError> {
        let started = Instant::now();
        if self.daemon.artifact_path().exists() {
            OpenOptions::new().write(true).truncate(true).open(self.daemon.artifact_path())?;
        }
        let released = std::mem::take(&mut self.artifact_bytes);
        if released != 0 {
            accounting.apply(ResourceEvent::DiskReleased { bytes: released })?;
        }
        self.measure(context, Metric::ArtifactGcPause, micros(started.elapsed()), measurements)
    }
}

//! Integrated disposable subject exercised by H3 plans.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

use peritus_benchmarks::{
    AccountingSink, MeasurementRecord, MeasurementSink, Metric, PlanStep, QualificationSubject,
    RunContext, StableId, SubjectDescriptor,
};
use peritus_types::RevisionTuple;

use crate::SubjectError;
use crate::a3::A3Client;
use crate::daemon::DisposableDaemon;
use crate::effects::{micros, resident_bytes};
use crate::identity::IdentitySource;
use crate::process::OwnedProcess;
use crate::scheduler::{SchedulerRun, qualification_revision};

mod operations;

/// Borrowed capability created together with one disposable subject.
pub struct SubjectAuthorization {
    instance: [u8; 16],
}

/// Owned subject and its non-forgeable qualification capability.
pub struct AuthorizedSubject {
    subject: IntegratedSubject,
    authorization: SubjectAuthorization,
}

impl AuthorizedSubject {
    /// Returns disjoint borrows suitable for [`crate::PacedRunner`].
    pub const fn parts(&mut self) -> (&mut IntegratedSubject, &SubjectAuthorization) {
        (&mut self.subject, &self.authorization)
    }
}

/// Real disposable daemon plus local deterministic pressure effects.
pub struct IntegratedSubject {
    descriptor: SubjectDescriptor,
    authorization: [u8; 16],
    daemon: DisposableDaemon,
    client: Option<A3Client>,
    identities: IdentitySource,
    revision: RevisionTuple,
    origin: Instant,
    measurement_sequence: u64,
    startup_latency: Option<u64>,
    runs: BTreeMap<u64, SchedulerRun>,
    event_run: Option<SchedulerRun>,
    processes: BTreeMap<u64, OwnedProcess>,
    provider_requests: BTreeSet<u64>,
    queue_depths: BTreeMap<peritus_benchmarks::QueueKind, u32>,
    artifact_bytes: u64,
}

impl IntegratedSubject {
    /// Launches a fresh daemon and binds the adapter to its exact executable bytes.
    ///
    /// # Errors
    ///
    /// Returns [`SubjectError`] when the daemon cannot start, negotiate A3, or produce a complete
    /// subject identity.
    pub fn launch(
        daemon_executable: &Path,
        implementation_revision: impl Into<String>,
    ) -> Result<AuthorizedSubject, SubjectError> {
        let mut identities = IdentitySource::new(u64::from(std::process::id()));
        let authorization = identities.key()?.try_into().map_err(|_| {
            SubjectError::Configuration("authorization identity had the wrong length".to_owned())
        })?;
        let revision = qualification_revision(&mut identities)?;
        let (daemon, startup) = DisposableDaemon::launch(daemon_executable)?;
        let descriptor = SubjectDescriptor::new(
            StableId::new("peritus-daemon")?,
            implementation_revision,
            daemon.executable_digest()?,
        )?;
        let client = A3Client::connect(daemon.endpoint(), None, &mut identities)?;
        let subject = Self {
            descriptor,
            authorization,
            daemon,
            client: Some(client),
            identities,
            revision,
            origin: Instant::now(),
            measurement_sequence: 0,
            startup_latency: Some(micros(startup)),
            runs: BTreeMap::new(),
            event_run: None,
            processes: BTreeMap::new(),
            provider_requests: BTreeSet::new(),
            queue_depths: BTreeMap::new(),
            artifact_bytes: 0,
        };
        Ok(AuthorizedSubject {
            subject,
            authorization: SubjectAuthorization { instance: authorization },
        })
    }

    fn measure(
        &mut self,
        context: &RunContext,
        metric: Metric,
        value: u64,
        sink: &mut dyn MeasurementSink,
    ) -> Result<(), SubjectError> {
        let record = MeasurementRecord::new(
            context.run_id().clone(),
            context.profile_id().clone(),
            context.workload_id().clone(),
            metric,
            self.measurement_sequence,
            micros(self.origin.elapsed()),
            value,
        )?;
        sink.record(record)?;
        self.measurement_sequence =
            self.measurement_sequence.checked_add(1).ok_or(SubjectError::IdentityExhausted)?;
        Ok(())
    }

    fn record_startup(
        &mut self,
        context: &RunContext,
        sink: &mut dyn MeasurementSink,
    ) -> Result<(), SubjectError> {
        if let Some(latency) = self.startup_latency.take() {
            self.measure(context, Metric::DaemonStartupLatency, latency, sink)?;
        }
        Ok(())
    }

    fn start_run(
        &mut self,
        run: u64,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
    ) -> Result<(), SubjectError> {
        let started = Instant::now();
        let revision = self.revision;
        let mut client = self.client.take().ok_or_else(|| {
            SubjectError::UnexpectedResponse("daemon is crashed and has no A3 session".to_owned())
        })?;
        let scheduler = SchedulerRun::create(&mut client, &mut self.identities, revision)?;
        self.client = Some(client);
        if self.runs.insert(run, scheduler).is_some() {
            return Err(SubjectError::UnexpectedResponse(format!(
                "plan started duplicate run {run}"
            )));
        }
        self.measure(
            context,
            Metric::CommandToFirstEventLatency,
            micros(started.elapsed()),
            measurements,
        )?;
        self.measure(
            context,
            Metric::ConcurrentRuns,
            u64::try_from(self.runs.len()).unwrap_or(u64::MAX),
            measurements,
        )
    }

    fn append_event(
        &mut self,
        context: &RunContext,
        sequence: u64,
        measurements: &mut dyn MeasurementSink,
    ) -> Result<(), SubjectError> {
        let started = Instant::now();
        let mut client = self.client.take().ok_or_else(|| {
            SubjectError::UnexpectedResponse("daemon is crashed and has no A3 session".to_owned())
        })?;
        if self.runs.is_empty() {
            if self.event_run.is_none() {
                self.event_run =
                    Some(SchedulerRun::create(&mut client, &mut self.identities, self.revision)?);
            }
            self.event_run
                .as_mut()
                .expect("event scheduler was initialized")
                .append_event(&mut client, &mut self.identities)?;
        } else {
            let index = usize::try_from(sequence % self.runs.len() as u64).unwrap_or(0);
            let run = *self.runs.keys().nth(index).expect("nonempty run map");
            self.runs
                .get_mut(&run)
                .expect("selected run remains present")
                .append_event(&mut client, &mut self.identities)?;
        }
        self.client = Some(client);
        self.measure(context, Metric::EventAppendLatency, micros(started.elapsed()), measurements)
    }

    fn finish_run(&mut self, run: u64) -> Result<(), SubjectError> {
        let mut scheduler = self.runs.remove(&run).ok_or_else(|| {
            SubjectError::UnexpectedResponse(format!("plan finished unknown run {run}"))
        })?;
        let mut client = self.client.take().ok_or_else(|| {
            SubjectError::UnexpectedResponse("daemon is crashed and has no A3 session".to_owned())
        })?;
        let result = scheduler.finish(&mut client, &mut self.identities);
        self.client = Some(client);
        result
    }

    fn sample_resources(
        &mut self,
        context: &RunContext,
        measurements: &mut dyn MeasurementSink,
    ) -> Result<(), SubjectError> {
        let resident = resident_bytes(self.daemon.pid().ok_or_else(|| {
            SubjectError::UnexpectedResponse("daemon has no live process".to_owned())
        })?)?;
        let workload = context.workload_id().as_str();
        let metric = if workload.contains("terminal-stream") {
            Metric::SteadyMemoryPerProcess
        } else if workload.contains("concurrent-runs") || workload.contains("memory-bounds") {
            Metric::SteadyMemoryPerRun
        } else {
            Metric::DiskUsage
        };
        let divisor = match metric {
            Metric::SteadyMemoryPerRun => self.runs.len().max(1),
            Metric::SteadyMemoryPerProcess => self.processes.len().max(1),
            _ => 1,
        };
        self.measure(
            context,
            metric,
            resident / u64::try_from(divisor).unwrap_or(1),
            measurements,
        )?;
        if workload.contains("memory-bounds") {
            self.measure(context, Metric::PeakResidentMemory, resident, measurements)?;
        }
        if workload.contains("disk-artifacts") {
            self.measure(context, Metric::DiskUsage, self.artifact_bytes, measurements)?;
        }
        Ok(())
    }
}

impl QualificationSubject for IntegratedSubject {
    type Authorization = SubjectAuthorization;
    type Error = SubjectError;

    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn execute_step(
        &mut self,
        authorization: &Self::Authorization,
        context: &RunContext,
        step: &PlanStep,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), Self::Error> {
        if authorization.instance != self.authorization {
            return Err(SubjectError::AuthorizationMismatch);
        }
        self.record_startup(context, measurements)?;
        self.execute_operation(context, step, measurements, accounting)
    }
}

impl Drop for IntegratedSubject {
    fn drop(&mut self) {
        for process in self.processes.values_mut() {
            let _ = process.terminate();
        }
    }
}

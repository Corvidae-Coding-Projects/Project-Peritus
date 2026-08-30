//! Production-catalog campaign coordination for disposable integrated subjects.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use peritus_benchmarks::{
    AccountingSummary, BaselineManifest, MeasurementSet, PlanKind, QualificationDataset,
    QualificationEvaluation, QualificationEvaluator, QualificationPlan, QualificationProfile,
    QualificationRunner, QualificationSubject, RunContext, RunnerDescriptor, RunnerReceipt,
    StableId, SubjectDescriptor, Workload,
};

use crate::sampling::{Sample, SamplingSink, merge_samples};
use crate::shared_accounting::SharedAccounting;
use crate::{CampaignError, CancellationFlag, IntegratedSubject, MachineObservation, PacedRunner};

/// Workload horizon selected for one operator invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignMode {
    /// Runs every sub-hour load workload sequentially.
    Load,
    /// Runs all load workloads, then all long-horizon workloads concurrently.
    Full,
}

/// Complete immutable inputs required before a campaign can execute.
pub struct CampaignRequest {
    dataset: QualificationDataset,
    daemon_executable: PathBuf,
    implementation_revision: String,
    run_id: StableId,
    runner: RunnerDescriptor,
    machine: MachineObservation,
    mode: CampaignMode,
    baseline: Option<BaselineManifest>,
}

impl CampaignRequest {
    /// Constructs a campaign request without silently supplying an accepted baseline.
    #[must_use]
    pub fn new(
        dataset: QualificationDataset,
        daemon_executable: PathBuf,
        implementation_revision: impl Into<String>,
        run_id: StableId,
        runner: RunnerDescriptor,
        machine: MachineObservation,
        mode: CampaignMode,
    ) -> Self {
        Self {
            dataset,
            daemon_executable,
            implementation_revision: implementation_revision.into(),
            run_id,
            runner,
            machine,
            mode,
            baseline: None,
        }
    }

    /// Supplies an independently accepted baseline for regression comparison.
    #[must_use]
    pub fn baseline(mut self, baseline: BaselineManifest) -> Self {
        self.baseline = Some(baseline);
        self
    }
}

/// Executed campaign data ready for external evidence retention.
pub struct CampaignOutcome {
    measurements: MeasurementSet,
    accounting: AccountingSummary,
    receipts: Vec<RunnerReceipt>,
    evaluation: QualificationEvaluation,
    subject: SubjectDescriptor,
    runner: RunnerDescriptor,
    machine: MachineObservation,
    started_unix_micros: u64,
    finished_unix_micros: u64,
}

impl CampaignOutcome {
    /// Returns the bounded, campaign-resequenced measurements.
    #[must_use]
    pub const fn measurements(&self) -> &MeasurementSet {
        &self.measurements
    }

    /// Returns combined high-water and terminal resource accounting.
    #[must_use]
    pub const fn accounting(&self) -> &AccountingSummary {
        &self.accounting
    }

    /// Returns one terminal receipt per selected workload.
    #[must_use]
    pub fn receipts(&self) -> &[RunnerReceipt] {
        &self.receipts
    }

    /// Returns the deterministic SLO and baseline evaluation.
    #[must_use]
    pub const fn evaluation(&self) -> &QualificationEvaluation {
        &self.evaluation
    }

    /// Returns the exact common subject identity.
    #[must_use]
    pub const fn subject(&self) -> &SubjectDescriptor {
        &self.subject
    }

    /// Returns the runner identity used for every workload.
    #[must_use]
    pub const fn runner(&self) -> &RunnerDescriptor {
        &self.runner
    }

    /// Returns retained measured-host facts.
    #[must_use]
    pub const fn machine(&self) -> &MachineObservation {
        &self.machine
    }

    /// Returns the campaign opening time in Unix microseconds.
    #[must_use]
    pub const fn started_unix_micros(&self) -> u64 {
        self.started_unix_micros
    }

    /// Returns the campaign closing time in Unix microseconds.
    #[must_use]
    pub const fn finished_unix_micros(&self) -> u64 {
        self.finished_unix_micros
    }
}

/// Executes stable load plans and concurrent long-horizon plans.
pub struct CampaignCoordinator;

impl CampaignCoordinator {
    /// Executes the selected production horizon and derives one fail-closed evaluation.
    ///
    /// # Errors
    ///
    /// Rejects reference-machine mismatch before workload launch, runner failures, panicked
    /// workers, subject identity drift, sample-bound violations, and evaluator contract failures.
    pub fn run(request: CampaignRequest) -> Result<CampaignOutcome, CampaignError> {
        let profile = request.dataset.profile();
        if !request.machine.assess(profile.reference_machine()).matches() {
            return Err(CampaignError::ReferenceMachineMismatch);
        }
        let (loads, soaks) = classify_workloads(request.dataset.workloads());
        let selected = match request.mode {
            CampaignMode::Load => loads.len(),
            CampaignMode::Full => loads.len().saturating_add(soaks.len()),
        };
        if selected == 0 {
            return Err(CampaignError::NoWorkloads);
        }

        let started_unix_micros = unix_micros(SystemTime::now())?;
        let campaign_origin = Instant::now();
        let cancellation = CancellationFlag::new();
        let accounting = SharedAccounting::new(profile.envelope());
        let mut outcomes = Vec::with_capacity(selected);
        for workload in loads {
            outcomes.push(run_workload(WorkloadInvocation {
                daemon_executable: request.daemon_executable.clone(),
                implementation_revision: request.implementation_revision.clone(),
                run_id: request.run_id.clone(),
                runner: request.runner.clone(),
                profile: profile.clone(),
                workload,
                kind: PlanKind::Load,
                elapsed_offset_micros: micros(campaign_origin.elapsed()),
                cancellation: cancellation.clone(),
                accounting: accounting.clone(),
            })?);
        }
        if request.mode == CampaignMode::Full {
            outcomes.extend(run_soaks(
                soaks,
                &request,
                &campaign_origin,
                &cancellation,
                &accounting,
            )?);
        }
        outcomes.sort_by(|left, right| left.workload_id.cmp(&right.workload_id));

        let subject = outcomes.first().ok_or(CampaignError::NoWorkloads)?.subject.clone();
        if outcomes.iter().any(|outcome| outcome.subject != subject) {
            return Err(CampaignError::SubjectIdentityMismatch);
        }
        let workload_ids =
            outcomes.iter().map(|outcome| outcome.workload_id.clone()).collect::<Vec<_>>();
        let receipts = outcomes.iter().map(|outcome| outcome.receipt.clone()).collect::<Vec<_>>();
        let samples = outcomes.into_iter().flat_map(|outcome| outcome.samples).collect::<Vec<_>>();
        let measurements = merge_samples(
            &request.run_id,
            profile.id(),
            workload_ids,
            profile.max_measurements(),
            samples,
        )?;
        let accounting = accounting.summary()?;
        let evaluation = QualificationEvaluator::evaluate(
            profile,
            request.dataset.workloads(),
            &measurements,
            accounting.clone(),
            &receipts,
            request.baseline.as_ref(),
        )?;
        Ok(CampaignOutcome {
            measurements,
            accounting,
            receipts,
            evaluation,
            subject,
            runner: request.runner,
            machine: request.machine,
            started_unix_micros,
            finished_unix_micros: unix_micros(SystemTime::now())?,
        })
    }
}

struct WorkloadInvocation {
    daemon_executable: PathBuf,
    implementation_revision: String,
    run_id: StableId,
    runner: RunnerDescriptor,
    profile: QualificationProfile,
    workload: Workload,
    kind: PlanKind,
    elapsed_offset_micros: u64,
    cancellation: CancellationFlag,
    accounting: SharedAccounting,
}

struct WorkloadOutcome {
    workload_id: StableId,
    subject: SubjectDescriptor,
    receipt: RunnerReceipt,
    samples: Vec<Sample>,
}

fn run_workload(invocation: WorkloadInvocation) -> Result<WorkloadOutcome, CampaignError> {
    let workload_id = invocation.workload.id().clone();
    let plan_id = StableId::new(format!("plan.{workload_id}"))?;
    let plan = QualificationPlan::new(
        plan_id.clone(),
        invocation.kind,
        invocation.profile.id().clone(),
        invocation.profile.envelope(),
        invocation.workload,
    )?;
    let context = RunContext::for_workload(
        invocation.run_id,
        invocation.profile.id().clone(),
        plan_id,
        workload_id.clone(),
    );
    let mut authorized = IntegratedSubject::launch(
        &invocation.daemon_executable,
        invocation.implementation_revision,
    )?;
    let mut runner = PacedRunner::new(invocation.runner, invocation.cancellation);
    let mut sampling = SamplingSink::new(
        &invocation.profile,
        workload_id.clone(),
        invocation.elapsed_offset_micros,
    );
    let mut accounting = invocation.accounting;
    let (integrated, authorization) = authorized.parts();
    let subject = integrated.descriptor().clone();
    let receipt =
        runner.run(integrated, authorization, &context, &plan, &mut sampling, &mut accounting)?;
    Ok(WorkloadOutcome { workload_id, subject, receipt, samples: sampling.finish() })
}

fn run_soaks(
    soaks: Vec<Workload>,
    request: &CampaignRequest,
    campaign_origin: &Instant,
    cancellation: &CancellationFlag,
    accounting: &SharedAccounting,
) -> Result<Vec<WorkloadOutcome>, CampaignError> {
    let worker_count = soaks.len();
    let (sender, receiver) = mpsc::channel();
    let mut handles = Vec::with_capacity(worker_count);
    for workload in soaks {
        let sender = sender.clone();
        let invocation = WorkloadInvocation {
            daemon_executable: request.daemon_executable.clone(),
            implementation_revision: request.implementation_revision.clone(),
            run_id: request.run_id.clone(),
            runner: request.runner.clone(),
            profile: request.dataset.profile().clone(),
            workload,
            kind: PlanKind::Soak,
            elapsed_offset_micros: micros(campaign_origin.elapsed()),
            cancellation: cancellation.clone(),
            accounting: accounting.clone(),
        };
        let cancellation = cancellation.clone();
        handles.push(thread::spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_workload(invocation)))
                    .unwrap_or(Err(CampaignError::WorkerPanicked));
            if result.is_err() {
                cancellation.cancel();
            }
            let _ = sender.send(result);
        }));
    }
    drop(sender);
    let mut outcomes = Vec::with_capacity(worker_count);
    let mut failure = None;
    for result in receiver {
        match result {
            Ok(outcome) => outcomes.push(outcome),
            Err(error) if failure.is_none() => failure = Some(error),
            Err(_) => {}
        }
    }
    if handles.into_iter().any(|handle| handle.join().is_err()) {
        return Err(CampaignError::WorkerPanicked);
    }
    if let Some(error) = failure {
        return Err(error);
    }
    if outcomes.len() != worker_count {
        return Err(CampaignError::WorkerPanicked);
    }
    Ok(outcomes)
}

pub fn classify_workloads(workloads: &[Workload]) -> (Vec<Workload>, Vec<Workload>) {
    workloads.iter().cloned().partition(|workload| workload.parameters().duration_seconds() < 3_600)
}

fn unix_micros(time: SystemTime) -> Result<u64, CampaignError> {
    Ok(micros(time.duration_since(UNIX_EPOCH)?))
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

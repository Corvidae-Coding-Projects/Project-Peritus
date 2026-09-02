//! Sequential fresh-subject native H0 campaign runner.

use std::{
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind},
};

use peritus_security_policy::IntegratedCandidate;

use crate::{
    CancellationToken, CaseFailure, CaseReport, CleanupObservation, ProbeObservation, ProbeOutcome,
    ProbeSpec, QualificationError, QualificationLimits, QualificationRun,
    observation::checked_subject_id,
    shard::{QualificationPlatform, QualificationShard},
};

/// Exact immutable request supplied to one fresh native subject.
#[derive(Clone, Copy, Debug)]
pub struct ProbeRequest<'signal> {
    candidate: IntegratedCandidate,
    spec: ProbeSpec,
    limits: QualificationLimits,
    cancellation: &'signal CancellationToken,
}

impl<'signal> ProbeRequest<'signal> {
    /// Returns the exact integrated candidate.
    #[must_use]
    pub const fn candidate(self) -> IntegratedCandidate {
        self.candidate
    }

    /// Returns the closed probe contract.
    #[must_use]
    pub const fn spec(self) -> ProbeSpec {
        self.spec
    }

    /// Returns hard per-case resource ceilings.
    #[must_use]
    pub const fn limits(self) -> QualificationLimits {
        self.limits
    }

    /// Borrows the cooperative cancellation signal.
    #[must_use]
    pub const fn cancellation(self) -> &'signal CancellationToken {
        self.cancellation
    }
}

/// One never-before-used native H0 subject.
pub trait QualificationSubject {
    /// Returns the stable unique identity before probe effects begin.
    fn subject_id(&self) -> &str;

    /// Executes exactly one requested native probe.
    ///
    /// Implementations must enforce the supplied native deadline and resource limits, respond to
    /// cancellation, and return only directly observed native evidence.
    ///
    /// # Errors
    ///
    /// Returns a typed adapter or execution failure; failure must never be converted to pass.
    fn execute(
        &mut self,
        request: ProbeRequest<'_>,
    ) -> Result<ProbeObservation, QualificationError>;

    /// Terminates owned processes, unmounts sandboxes, removes temporary paths, and closes network
    /// endpoints, then reports remaining resources.
    ///
    /// # Errors
    ///
    /// Returns a typed cleanup failure when complete teardown cannot be directly observed.
    fn cleanup(self: Box<Self>) -> Result<CleanupObservation, QualificationError>;
}

/// Host adapter that provisions an exclusive fresh subject for one closed probe.
pub trait FreshSubjectFactory {
    /// Creates a subject that has not executed another H0 probe.
    ///
    /// # Errors
    ///
    /// Returns provisioning failure without manufacturing an observation.
    fn create(
        &mut self,
        candidate: IntegratedCandidate,
        spec: ProbeSpec,
        limits: QualificationLimits,
        cancellation: &CancellationToken,
    ) -> Result<Box<dyn QualificationSubject>, QualificationError>;
}

/// Stateless deterministic H0 campaign orchestrator.
#[derive(Clone, Copy, Debug, Default)]
pub struct QualificationRunner;

impl QualificationRunner {
    /// Runs the complete production catalog with one fresh native subject per probe.
    ///
    /// Cleanup is attempted exactly once after every successful provision, including execution
    /// errors and panics. Adapter failure, cancellation, unsupported controls, resource overrun,
    /// reused identities, and incomplete cleanup remain explicit failing case reports.
    ///
    /// # Errors
    ///
    /// Returns only when the runner cannot assemble the canonical closed run shape.
    pub fn run(
        &self,
        factory: &mut dyn FreshSubjectFactory,
        candidate: IntegratedCandidate,
        limits: QualificationLimits,
        cancellation: &CancellationToken,
    ) -> Result<QualificationRun, QualificationError> {
        let cases = run_specs(
            factory,
            candidate,
            limits,
            cancellation,
            ProbeSpec::h0_production().iter().copied(),
        );
        QualificationRun::new(candidate, limits, cases)
    }

    /// Runs only the probes canonically assigned to one native platform.
    ///
    /// Portable tier-one probes run once on Linux. macOS and Windows shards contain only their
    /// native backend probe, so no host can manufacture evidence for another operating system.
    ///
    /// # Errors
    ///
    /// Returns when the resulting shard does not have its exact canonical shape.
    pub fn run_shard(
        &self,
        factory: &mut dyn FreshSubjectFactory,
        candidate: IntegratedCandidate,
        limits: QualificationLimits,
        cancellation: &CancellationToken,
        platform: QualificationPlatform,
    ) -> Result<QualificationShard, QualificationError> {
        let specs =
            ProbeSpec::h0_production().iter().copied().filter(|spec| platform.owns(spec.target()));
        let cases = run_specs(factory, candidate, limits, cancellation, specs);
        QualificationShard::new(candidate, limits, platform, cases)
    }

    pub(crate) fn run_shard_partition(
        factory: &mut dyn FreshSubjectFactory,
        candidate: IntegratedCandidate,
        limits: QualificationLimits,
        cancellation: &CancellationToken,
        platform: QualificationPlatform,
        partition: usize,
        partition_count: usize,
    ) -> Vec<CaseReport> {
        let specs = ProbeSpec::h0_production()
            .iter()
            .copied()
            .filter(|spec| platform.owns(spec.target()))
            .enumerate()
            .filter_map(|(index, spec)| (index % partition_count == partition).then_some(spec));
        run_specs(factory, candidate, limits, cancellation, specs)
    }

    /// Combines exactly one canonical shard from every native platform.
    ///
    /// # Errors
    ///
    /// Rejects missing, duplicate, stale, differently limited, or malformed shards.
    pub fn aggregate(
        &self,
        shards: Vec<QualificationShard>,
    ) -> Result<QualificationRun, QualificationError> {
        crate::shard::aggregate(shards)
    }
}

fn run_specs(
    factory: &mut dyn FreshSubjectFactory,
    candidate: IntegratedCandidate,
    limits: QualificationLimits,
    cancellation: &CancellationToken,
    specs: impl IntoIterator<Item = ProbeSpec>,
) -> Vec<CaseReport> {
    let mut cases = Vec::new();
    let mut subjects = BTreeSet::<String>::new();
    for spec in specs {
        cases.push(run_case(factory, candidate, spec, limits, cancellation, &mut subjects));
    }
    cases
}

fn run_case(
    factory: &mut dyn FreshSubjectFactory,
    candidate: IntegratedCandidate,
    spec: ProbeSpec,
    limits: QualificationLimits,
    cancellation: &CancellationToken,
    subjects: &mut BTreeSet<String>,
) -> CaseReport {
    if cancellation.is_cancelled() {
        return CaseReport::new(spec, None, None, None, vec![CaseFailure::Cancelled]);
    }
    let subject = match provision(factory, candidate, spec, limits, cancellation) {
        Ok(subject) => subject,
        Err(failure) => return CaseReport::new(spec, None, None, None, vec![failure]),
    };
    execute_and_cleanup(subject, candidate, spec, limits, cancellation, subjects)
}

fn provision(
    factory: &mut dyn FreshSubjectFactory,
    candidate: IntegratedCandidate,
    spec: ProbeSpec,
    limits: QualificationLimits,
    cancellation: &CancellationToken,
) -> Result<Box<dyn QualificationSubject>, CaseFailure> {
    match catch_unwind(AssertUnwindSafe(|| factory.create(candidate, spec, limits, cancellation))) {
        Ok(Ok(subject)) => Ok(subject),
        Ok(Err(error)) => Err(CaseFailure::Provision(error)),
        Err(_) => Err(CaseFailure::AdapterPanicked("provision")),
    }
}

fn execute_and_cleanup(
    mut subject: Box<dyn QualificationSubject>,
    candidate: IntegratedCandidate,
    spec: ProbeSpec,
    limits: QualificationLimits,
    cancellation: &CancellationToken,
    subjects: &mut BTreeSet<String>,
) -> CaseReport {
    let subject_id_result =
        catch_unwind(AssertUnwindSafe(|| checked_subject_id(subject.subject_id().to_owned())));
    let mut failures = Vec::new();
    let mut observation = None;
    let subject_id = match subject_id_result {
        Ok(Ok(id)) => Some(id),
        Ok(Err(error)) => {
            failures.push(CaseFailure::NativeExecution(error));
            None
        }
        Err(_) => {
            failures.push(CaseFailure::AdapterPanicked("subject-id"));
            None
        }
    };
    if let Some(id) = &subject_id {
        if subjects.insert(id.clone()) {
            observation = execute_probe(
                subject.as_mut(),
                candidate,
                spec,
                limits,
                cancellation,
                &mut failures,
            );
        } else {
            failures.push(CaseFailure::SubjectReused);
        }
    }
    let cleanup = cleanup_subject(subject, subject_id.as_deref(), &mut failures);
    CaseReport::new(spec, subject_id, observation, cleanup, failures)
}

fn execute_probe(
    subject: &mut dyn QualificationSubject,
    candidate: IntegratedCandidate,
    spec: ProbeSpec,
    limits: QualificationLimits,
    cancellation: &CancellationToken,
    failures: &mut Vec<CaseFailure>,
) -> Option<ProbeObservation> {
    let request = ProbeRequest { candidate, spec, limits, cancellation };
    match catch_unwind(AssertUnwindSafe(|| subject.execute(request))) {
        Ok(Ok(observed)) => {
            validate_observation(candidate, spec, limits, cancellation, &observed, failures);
            Some(observed)
        }
        Ok(Err(error)) => {
            failures.push(CaseFailure::NativeExecution(error));
            None
        }
        Err(_) => {
            failures.push(CaseFailure::AdapterPanicked("execute"));
            None
        }
    }
}

fn cleanup_subject(
    subject: Box<dyn QualificationSubject>,
    expected_id: Option<&str>,
    failures: &mut Vec<CaseFailure>,
) -> Option<CleanupObservation> {
    match catch_unwind(AssertUnwindSafe(|| subject.cleanup())) {
        Ok(Ok(cleanup)) => {
            if expected_id.is_some_and(|id| cleanup.subject_id() != id) {
                failures.push(CaseFailure::CleanupSubjectMismatch);
            }
            if !cleanup.complete() {
                failures.push(CaseFailure::CleanupIncomplete);
            }
            Some(cleanup)
        }
        Ok(Err(error)) => {
            failures.push(CaseFailure::Cleanup(error));
            None
        }
        Err(_) => {
            failures.push(CaseFailure::AdapterPanicked("cleanup"));
            None
        }
    }
}

fn validate_observation(
    candidate: IntegratedCandidate,
    spec: ProbeSpec,
    limits: QualificationLimits,
    cancellation: &CancellationToken,
    observation: &ProbeObservation,
    failures: &mut Vec<CaseFailure>,
) {
    if observation.candidate() != candidate {
        failures.push(CaseFailure::CandidateMismatch);
    }
    if observation.probe() != spec.id() {
        failures.push(CaseFailure::ProbeMismatch);
    }
    let usage = observation.receipt().usage();
    if !usage.within(limits) {
        failures.push(CaseFailure::ResourceLimitExceeded { usage, limits });
    }
    if spec.requires_native_sandbox()
        && observation.outcome() == ProbeOutcome::Passed
        && !observation.receipt().native_sandbox_observed()
    {
        failures.push(CaseFailure::NativeSandboxNotObserved);
    }
    match observation.outcome() {
        ProbeOutcome::Passed => {}
        ProbeOutcome::Failed => failures.push(CaseFailure::AssertionFailed),
        ProbeOutcome::Unsupported => failures.push(CaseFailure::Unsupported),
    }
    if cancellation.is_cancelled() {
        failures.push(CaseFailure::Cancelled);
    }
}

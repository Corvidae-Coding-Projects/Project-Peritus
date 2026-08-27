//! Authority-neutral subject and runner integration contracts.

use std::error::Error;

use serde::Serialize;

use crate::{
    MeasurementIngestor, MeasurementRecord, PlanStep, QualificationError, QualificationPlan,
    ResourceAccountant, ResourceEvent, Sha256Digest, StableId,
};

/// Immutable identity of the G0 or F0 adapter under qualification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SubjectDescriptor {
    component: StableId,
    implementation_revision: String,
    executable_digest: Sha256Digest,
}

impl SubjectDescriptor {
    /// Constructs a subject identity bound to exact executable bytes.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the implementation revision is empty or exceeds its
    /// bounded evidence representation.
    pub fn new(
        component: StableId,
        implementation_revision: impl Into<String>,
        executable_digest: Sha256Digest,
    ) -> Result<Self, QualificationError> {
        let implementation_revision = implementation_revision.into();
        if implementation_revision.trim().is_empty() || implementation_revision.len() > 200 {
            return Err(QualificationError::invalid_value(
                "subject.implementation_revision",
                "must contain 1 through 200 bytes",
            ));
        }
        Ok(Self { component, implementation_revision, executable_digest })
    }

    /// Returns the component key, such as `peritus-daemon` or `peritus-evolution`.
    #[must_use]
    pub const fn component(&self) -> &StableId {
        &self.component
    }

    /// Returns the source or release revision reported by the adapter.
    #[must_use]
    pub fn implementation_revision(&self) -> &str {
        &self.implementation_revision
    }

    /// Returns the digest of the executable subject artifact.
    #[must_use]
    pub const fn executable_digest(&self) -> &Sha256Digest {
        &self.executable_digest
    }
}

/// Immutable identity of the runner implementation and configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunnerDescriptor {
    runner: StableId,
    version: String,
    implementation_digest: Sha256Digest,
}

impl RunnerDescriptor {
    /// Constructs a runner identity bound to exact implementation bytes.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the version is empty or exceeds its bounded evidence
    /// representation.
    pub fn new(
        runner: StableId,
        version: impl Into<String>,
        implementation_digest: Sha256Digest,
    ) -> Result<Self, QualificationError> {
        let version = version.into();
        if version.trim().is_empty() || version.len() > 80 {
            return Err(QualificationError::invalid_value(
                "runner.version",
                "must contain 1 through 80 bytes",
            ));
        }
        Ok(Self { runner, version, implementation_digest })
    }

    /// Returns the runner implementation key.
    #[must_use]
    pub const fn runner(&self) -> &StableId {
        &self.runner
    }

    /// Returns runner version text.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the runner implementation digest.
    #[must_use]
    pub const fn implementation_digest(&self) -> &Sha256Digest {
        &self.implementation_digest
    }
}

/// Stable invocation bindings shared by runner and subject adapters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunContext {
    run: StableId,
    profile: StableId,
    plan: StableId,
}

impl RunContext {
    /// Constructs exact run, profile, and plan bindings.
    #[must_use]
    pub const fn new(run_id: StableId, profile_id: StableId, plan_id: StableId) -> Self {
        Self { run: run_id, profile: profile_id, plan: plan_id }
    }

    /// Returns the unique qualification run identifier.
    #[must_use]
    pub const fn run_id(&self) -> &StableId {
        &self.run
    }

    /// Returns the profile binding.
    #[must_use]
    pub const fn profile_id(&self) -> &StableId {
        &self.profile
    }

    /// Returns the plan binding.
    #[must_use]
    pub const fn plan_id(&self) -> &StableId {
        &self.plan
    }
}

/// Terminal state reported by a qualification runner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerTermination {
    /// Every planned step was executed and observed.
    Completed,
    /// External cancellation stopped the run before completion.
    Cancelled,
    /// The subject or a contract assertion failed.
    Failed,
    /// Runner infrastructure failed independently of the subject assertion.
    InfrastructureFailure,
}

/// Validated terminal receipt from a runner invocation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RunnerReceipt {
    run_id: StableId,
    plan_id: StableId,
    workload_id: StableId,
    expected_steps: u64,
    executed_steps: u64,
    termination: RunnerTermination,
    failures: Vec<String>,
}

impl RunnerReceipt {
    /// Constructs a receipt and prevents incomplete executions from claiming completion.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when step counts are invalid, completion contradicts the
    /// terminal state, or failure explanations exceed their count or size bounds.
    pub fn new(
        run_id: StableId,
        plan_id: StableId,
        workload_id: StableId,
        expected_steps: u64,
        executed_steps: u64,
        termination: RunnerTermination,
        failures: Vec<String>,
    ) -> Result<Self, QualificationError> {
        if expected_steps == 0 || executed_steps > expected_steps {
            return Err(QualificationError::invalid_value(
                "runner_receipt.steps",
                "expected steps must be nonzero and executed steps must not exceed them",
            ));
        }
        let completed = executed_steps == expected_steps && failures.is_empty();
        if (termination == RunnerTermination::Completed) != completed {
            return Err(QualificationError::invalid_value(
                "runner_receipt.termination",
                "completed requires every expected step and no failures",
            ));
        }
        if failures.len() > 64
            || failures.iter().any(|failure| failure.is_empty() || failure.len() > 512)
        {
            return Err(QualificationError::invalid_value(
                "runner_receipt.failures",
                "at most 64 nonempty failures of at most 512 bytes are allowed",
            ));
        }
        Ok(Self {
            run_id,
            plan_id,
            workload_id,
            expected_steps,
            executed_steps,
            termination,
            failures,
        })
    }

    /// Returns the qualification run binding.
    #[must_use]
    pub const fn run_id(&self) -> &StableId {
        &self.run_id
    }

    /// Returns the qualification plan binding.
    #[must_use]
    pub const fn plan_id(&self) -> &StableId {
        &self.plan_id
    }

    /// Returns the workload executed by the plan.
    #[must_use]
    pub const fn workload_id(&self) -> &StableId {
        &self.workload_id
    }

    /// Returns total planned steps.
    #[must_use]
    pub const fn expected_steps(&self) -> u64 {
        self.expected_steps
    }

    /// Returns steps confirmed executed by the runner.
    #[must_use]
    pub const fn executed_steps(&self) -> u64 {
        self.executed_steps
    }

    /// Returns the terminal runner classification.
    #[must_use]
    pub const fn termination(&self) -> RunnerTermination {
        self.termination
    }

    /// Returns bounded failure explanations in observation order.
    #[must_use]
    pub fn failures(&self) -> &[String] {
        &self.failures
    }

    /// Returns whether the receipt supports objective evaluation for readiness.
    #[must_use]
    pub const fn completed(&self) -> bool {
        matches!(self.termination, RunnerTermination::Completed)
    }
}

/// Sink exposed to subject adapters for typed measurements.
pub trait MeasurementSink {
    /// Records one observation or rejects it without partial ingestion.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the observation violates sink bindings or bounds.
    fn record(&mut self, measurement: MeasurementRecord) -> Result<(), QualificationError>;
}

impl MeasurementSink for MeasurementIngestor {
    fn record(&mut self, measurement: MeasurementRecord) -> Result<(), QualificationError> {
        Self::record(self, measurement)
    }
}

/// Sink exposed to subject adapters for exact resource lifecycle observations.
pub trait AccountingSink {
    /// Applies one lifecycle observation or rejects it without a bound bypass.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when the event violates accounting lifecycle or resource
    /// bounds.
    fn apply(&mut self, event: ResourceEvent) -> Result<(), QualificationError>;
}

impl AccountingSink for ResourceAccountant {
    fn apply(&mut self, event: ResourceEvent) -> Result<(), QualificationError> {
        Self::apply(self, event)
    }
}

/// G0/F0 adapter exercised by an external runner.
///
/// `Authorization` belongs to the integrating component. The qualification crate only borrows it;
/// it has no constructor, policy interpretation, or transition method for that type.
pub trait QualificationSubject {
    /// Component-owned authorization or capability proof required for an invocation.
    type Authorization;
    /// Adapter execution failure.
    type Error: Error + Send + Sync + 'static;

    /// Returns immutable subject identity.
    fn descriptor(&self) -> &SubjectDescriptor;

    /// Executes one scheduled operation under component-owned authorization.
    ///
    /// # Errors
    ///
    /// Returns the adapter's typed error when the subject cannot execute or observe the step.
    fn execute_step(
        &mut self,
        authorization: &Self::Authorization,
        context: &RunContext,
        step: &PlanStep,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<(), Self::Error>;
}

/// Runner contract that owns pacing, concurrency, cancellation, and complete observation.
pub trait QualificationRunner<S>
where
    S: QualificationSubject,
{
    /// Runner infrastructure failure.
    type Error: Error + Send + Sync + 'static;

    /// Returns immutable runner identity.
    fn descriptor(&self) -> &RunnerDescriptor;

    /// Executes a plan without manufacturing or widening the subject's authorization.
    ///
    /// # Errors
    ///
    /// Returns the runner's typed error when pacing, execution, observation, or infrastructure
    /// prevents production of a terminal receipt.
    fn run(
        &mut self,
        subject: &mut S,
        authorization: &S::Authorization,
        context: &RunContext,
        plan: &QualificationPlan,
        measurements: &mut dyn MeasurementSink,
        accounting: &mut dyn AccountingSink,
    ) -> Result<RunnerReceipt, Self::Error>;
}

//! Effect boundary for one-new-subject-per-scenario qualification.

use crate::{
    CleanupObservation, PackageManifest, QualificationError, QualificationRun, QualificationTarget,
    ScenarioId, ScenarioObservation, ScenarioSpec,
};

/// Exact immutable input supplied to a fresh scenario subject.
#[derive(Clone, Copy, Debug)]
pub struct ScenarioRequest<'manifest> {
    target: QualificationTarget,
    manifest: &'manifest PackageManifest,
    scenario: ScenarioSpec,
}

impl<'manifest> ScenarioRequest<'manifest> {
    /// Returns the target host contract.
    #[must_use]
    pub const fn target(self) -> QualificationTarget {
        self.target
    }

    /// Borrows the exact package manifest under qualification.
    #[must_use]
    pub const fn manifest(self) -> &'manifest PackageManifest {
        self.manifest
    }

    /// Returns the current closed scenario.
    #[must_use]
    pub const fn scenario(self) -> ScenarioSpec {
        self.scenario
    }
}

/// One newly provisioned, exclusive H2 subject.
pub trait QualificationSubject {
    /// Returns the stable unique identity before any scenario effect.
    fn subject_id(&self) -> &str;

    /// Installs and observes exactly the supplied scenario.
    ///
    /// # Errors
    ///
    /// Returns typed adapter or observation failures without manufacturing a passing result.
    fn execute(
        &mut self,
        request: ScenarioRequest<'_>,
    ) -> Result<ScenarioObservation, QualificationError>;

    /// Terminates processes and removes the fresh subject, returning complete cleanup evidence.
    ///
    /// # Errors
    ///
    /// Returns a typed failure when cleanup cannot be proven complete.
    fn close(self: Box<Self>) -> Result<CleanupObservation, QualificationError>;
}

/// Adapter that provisions a never-before-used subject for a single scenario.
pub trait FreshSubjectFactory {
    /// Creates a subject bound to the target and scenario before package effects occur.
    ///
    /// # Errors
    ///
    /// Returns a typed provisioning failure; an existing or recycled subject is invalid.
    fn create(
        &mut self,
        target: QualificationTarget,
        scenario: ScenarioId,
    ) -> Result<Box<dyn QualificationSubject>, QualificationError>;
}

/// Deterministic H2 orchestrator that never reuses a subject between scenarios.
#[derive(Clone, Copy, Debug, Default)]
pub struct FreshSubjectRunner;

impl FreshSubjectRunner {
    /// Runs the complete closed catalog with one newly provisioned subject per scenario.
    ///
    /// Cleanup is attempted after both successful and failed scenario execution. An execution
    /// error is returned only after the subject has been closed; cleanup failure takes precedence
    /// because it leaves external state unresolved.
    ///
    /// # Errors
    ///
    /// Returns target/manifest drift, provisioning, execution, cleanup, or run-shape failures.
    pub fn run(
        &self,
        factory: &mut dyn FreshSubjectFactory,
        target: QualificationTarget,
        manifest: &PackageManifest,
    ) -> Result<QualificationRun, QualificationError> {
        let contract = crate::PlatformContract::production(target.platform());
        contract.validate_target(target)?;
        if manifest.platform() != target.platform()
            || manifest.architecture() != target.architecture()
        {
            return Err(QualificationError::new(
                crate::QualificationErrorCode::InvalidInput,
                crate::QualificationRecovery::RebuildRelease,
                "start packaged-host qualification",
                "manifest platform or architecture differs from the fresh-subject target",
            ));
        }
        let mut observations = Vec::with_capacity(ScenarioId::all().len());
        for scenario in ScenarioId::all() {
            let mut subject = factory.create(target, scenario.id())?;
            let expected_subject_id = subject.subject_id().to_owned();
            let request = ScenarioRequest { target, manifest, scenario: *scenario };
            let result = subject.execute(request);
            let cleanup = subject.close();
            let cleanup = cleanup?;
            let mut observation = result?;
            if observation.scenario() != scenario.id()
                || observation.subject_id() != expected_subject_id
            {
                return Err(QualificationError::new(
                    crate::QualificationErrorCode::SubjectProtocol,
                    crate::QualificationRecovery::ReplaceSubject,
                    "run packaged-host qualification",
                    "adapter returned an observation for a different scenario or subject",
                ));
            }
            observation.attach_cleanup(cleanup)?;
            observations.push(observation);
        }
        QualificationRun::new(target, manifest.digest(), observations)
    }
}

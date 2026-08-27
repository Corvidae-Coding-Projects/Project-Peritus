//! Deterministic sequential execution with one fresh subject per scenario.

use crate::cancellation::CancellationOwner;
use crate::invariant::{evaluate, evaluate_cleanup};
use crate::unwind::{GuardedFuture, callback};
use crate::{
    DisruptionObservation, FailurePhase, PreparationObservation, QualificationConfig,
    QualificationReport, RecoveryObservation, ResilienceSubject, ResilienceSubjectFactory,
    ScenarioCatalog, ScenarioFailure, ScenarioReport, SuiteFailure,
};

/// Stateless H1 resilience qualification runner.
pub struct QualificationRunner;

impl QualificationRunner {
    /// Runs a catalog sequentially in stable ID order against fresh isolated subjects.
    ///
    /// The future is runtime-neutral. Dropping it cancels the current runner-owned token and drops
    /// the in-flight future and subject in place. Subjects must therefore combine cooperative
    /// cancellation with synchronous RAII ownership. A run polled to completion calls factory
    /// cleanup exactly once for every successfully created subject.
    pub async fn run<S, Factory>(
        config: QualificationConfig,
        catalog: &ScenarioCatalog,
        factory: &Factory,
    ) -> QualificationReport
    where
        S: ResilienceSubject,
        Factory: ResilienceSubjectFactory<S> + ?Sized,
    {
        let descriptor = match callback(FailurePhase::Definition, || factory.descriptor().clone()) {
            Ok(descriptor) => descriptor,
            Err(panic) => {
                return QualificationReport::invalid(
                    config,
                    catalog.profile(),
                    None,
                    SuiteFailure::SubjectDescriptorPanic(panic),
                );
            }
        };
        if catalog.scenarios().len() > usize::from(config.max_scenarios()) {
            return QualificationReport::invalid(
                config,
                catalog.profile(),
                Some(descriptor),
                SuiteFailure::CatalogExceedsConfiguration {
                    actual: catalog.scenarios().len(),
                    maximum: config.max_scenarios(),
                },
            );
        }
        let mut reports = Vec::with_capacity(catalog.scenarios().len());
        for scenario in catalog.scenarios() {
            reports.push(run_scenario(config, scenario, factory).await);
        }
        QualificationReport::complete(config, catalog.profile(), descriptor, reports)
    }
}

async fn run_scenario<S, Factory>(
    config: QualificationConfig,
    scenario: &crate::ScenarioSpec,
    factory: &Factory,
) -> ScenarioReport
where
    S: ResilienceSubject,
    Factory: ResilienceSubjectFactory<S> + ?Sized,
{
    let mut cancellation = CancellationOwner::new();
    let mut subject = match create_subject(scenario, factory, &cancellation).await {
        Ok(subject) => subject,
        Err(failure) => {
            return ScenarioReport::new(
                scenario.clone(),
                false,
                None,
                None,
                None,
                None,
                vec![failure],
            );
        }
    };

    let mut failures = Vec::new();
    let mut preparation = None;
    let mut disruption = None;
    let mut recovery = None;
    match exercise(&mut subject, scenario).await {
        Ok((prepared, disrupted, recovered)) => {
            for violation in evaluate(config, scenario, &prepared, &disrupted, &recovered) {
                failures.push(ScenarioFailure::Contract(violation));
            }
            preparation = Some(prepared);
            disruption = Some(disrupted);
            recovery = Some(recovered);
        }
        Err(failure) => failures.push(failure),
    }

    let mut cleanup = None;
    let cleanup_future =
        match callback(FailurePhase::Cleanup, || factory.cleanup(scenario, subject)) {
            Ok(future) => Some(future),
            Err(panic) => {
                failures.push(ScenarioFailure::Panic(panic));
                None
            }
        };
    if let Some(future) = cleanup_future {
        match GuardedFuture::new(future, FailurePhase::Cleanup).await {
            Ok(Ok(observation)) => {
                let cleanup_violations = evaluate_cleanup(config, observation);
                if cleanup_violations.is_empty() {
                    cancellation.completed();
                }
                for violation in cleanup_violations {
                    failures.push(ScenarioFailure::Contract(violation));
                }
                cleanup = Some(observation);
            }
            Ok(Err(error)) => {
                failures.push(ScenarioFailure::Subject { phase: FailurePhase::Cleanup, error });
            }
            Err(panic) => {
                failures.push(ScenarioFailure::Panic(panic));
            }
        }
    }

    ScenarioReport::new(
        scenario.clone(),
        true,
        preparation,
        disruption,
        recovery,
        cleanup,
        failures,
    )
}

async fn create_subject<S, Factory>(
    scenario: &crate::ScenarioSpec,
    factory: &Factory,
    cancellation: &CancellationOwner,
) -> Result<S, ScenarioFailure>
where
    S: ResilienceSubject,
    Factory: ResilienceSubjectFactory<S> + ?Sized,
{
    let future = callback(FailurePhase::Setup, || factory.create(scenario, cancellation.token()))
        .map_err(ScenarioFailure::Panic)?;
    match GuardedFuture::new(future, FailurePhase::Setup).await {
        Ok(Ok(subject)) => Ok(subject),
        Ok(Err(error)) => Err(ScenarioFailure::Subject { phase: FailurePhase::Setup, error }),
        Err(panic) => Err(ScenarioFailure::Panic(panic)),
    }
}

async fn exercise<S: ResilienceSubject>(
    subject: &mut S,
    scenario: &crate::ScenarioSpec,
) -> Result<(PreparationObservation, DisruptionObservation, RecoveryObservation), ScenarioFailure> {
    let prepare_future = callback(FailurePhase::Preparation, || subject.prepare(scenario))
        .map_err(ScenarioFailure::Panic)?;
    let preparation = match GuardedFuture::new(prepare_future, FailurePhase::Preparation).await {
        Ok(Ok(observation)) => observation,
        Ok(Err(error)) => {
            return Err(ScenarioFailure::Subject { phase: FailurePhase::Preparation, error });
        }
        Err(panic) => return Err(ScenarioFailure::Panic(panic)),
    };

    let inject_future = callback(FailurePhase::Injection, || subject.inject(scenario))
        .map_err(ScenarioFailure::Panic)?;
    let disruption = match GuardedFuture::new(inject_future, FailurePhase::Injection).await {
        Ok(Ok(observation)) => observation,
        Ok(Err(error)) => {
            return Err(ScenarioFailure::Subject { phase: FailurePhase::Injection, error });
        }
        Err(panic) => return Err(ScenarioFailure::Panic(panic)),
    };

    let recover_future = callback(FailurePhase::Recovery, || subject.recover(scenario))
        .map_err(ScenarioFailure::Panic)?;
    let recovery = match GuardedFuture::new(recover_future, FailurePhase::Recovery).await {
        Ok(Ok(observation)) => observation,
        Ok(Err(error)) => {
            return Err(ScenarioFailure::Subject { phase: FailurePhase::Recovery, error });
        }
        Err(panic) => return Err(ScenarioFailure::Panic(panic)),
    };
    Ok((preparation, disruption, recovery))
}

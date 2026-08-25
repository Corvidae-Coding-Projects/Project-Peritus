//! Executable D3 scheduler conformance cases.

use super::{
    SchedulerConformanceFixture, SchedulerConformanceSubject, SchedulerScenario, SchedulerTerminal,
};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct SchedulerCase {
    descriptor: CaseDescriptor,
    scenario: SchedulerScenario,
}

impl<S: SchedulerConformanceSubject> ConformanceCase<S> for SchedulerCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral D3 scheduler conformance suite.
#[must_use]
pub fn scheduler_suite<S: SchedulerConformanceSubject + 'static>() -> StaticSuite<S> {
    use SchedulerScenario as S;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.scheduler"),
            ReportText::literal(
                "D3 fairness, resources, ownership, cancellation, replay, and terminal contract",
            ),
        ),
        vec![
            boxed(
                "backpressure",
                "Queue and attempt bounds remain explicit",
                S::BoundedBackpressure,
            ),
            boxed(
                "cancellation-tree",
                "Cancellation reaches every descendant",
                S::CancellationTree,
            ),
            boxed(
                "dependencies",
                "Dependencies must succeed before dispatch",
                S::DependencyReadiness,
            ),
            boxed(
                "fairness",
                "Feasible selection is deterministic and bounded-fair",
                S::DeterministicFairness,
            ),
            boxed("pause-drain", "Pause and drain preserve active ownership", S::PauseAndDrain),
            boxed(
                "resources",
                "Reservations conserve global and worker capacity",
                S::ResourceConservation,
            ),
            boxed("restart", "Restart and exact retry are idempotent", S::Restart),
            boxed(
                "terminal-truth",
                "Only complete success can complete the scheduler",
                S::TerminalTruth,
            ),
            boxed("worker-loss", "Worker loss preserves retry and ambiguity truth", S::WorkerLoss),
            boxed("worker-ownership", "Live dispatch ownership remains unique", S::WorkerOwnership),
        ],
    )
}

fn boxed<S: SchedulerConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: SchedulerScenario,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(SchedulerCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(format!("peritus.scheduler.{suffix}"))
                .expect("static scheduler case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: SchedulerConformanceSubject>(
    subject: &mut S,
    scenario: SchedulerScenario,
) -> CaseResult {
    let fixture = SchedulerConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, 0, 0, 0, false);
    };
    let bounded = observed.work <= fixture.maximum_work()
        && observed.peak_attempt <= fixture.maximum_attempts()
        && observed.peak_bypass <= fixture.maximum_bypass();
    let common = bounded
        && observed.resources_conserved
        && observed.ownership_unique
        && observed.no_implicit_success;
    let completed = observed.terminal == SchedulerTerminal::Completed;
    let exact = common
        && match scenario {
            SchedulerScenario::DeterministicFairness => observed.selection_deterministic,
            SchedulerScenario::ResourceConservation => observed.resources_conserved,
            SchedulerScenario::DependencyReadiness => observed.dependencies_satisfied,
            SchedulerScenario::WorkerOwnership => observed.ownership_unique,
            SchedulerScenario::WorkerLoss => !completed && observed.loss_truthful,
            SchedulerScenario::BoundedBackpressure => !completed && observed.backpressure_bounded,
            SchedulerScenario::PauseAndDrain => observed.pause_respected,
            SchedulerScenario::CancellationTree => !completed && observed.cancellation_complete,
            SchedulerScenario::Restart => {
                observed.replay_equivalent && observed.idempotent_recovery
            }
            SchedulerScenario::TerminalTruth => completed && observed.no_implicit_success,
        };
    if exact {
        CaseResult::passed(observations(
            observed.work,
            observed.peak_attempt,
            observed.peak_bypass,
            true,
        ))
    } else {
        failed(scenario, observed.work, observed.peak_attempt, observed.peak_bypass, bounded)
    }
}

fn failed(
    scenario: SchedulerScenario,
    work: u16,
    attempts: u16,
    bypass: u16,
    exact: bool,
) -> CaseResult {
    CaseResult::failed(observations(work, attempts, bypass, exact), assertion(scenario))
}

fn observations(work: u16, attempts: u16, bypass: u16, exact: bool) -> Vec<Observation> {
    vec![
        Observation::new(
            ObservationId::catalog("work"),
            ObservationValue::Unsigned(u64::from(work)),
        ),
        Observation::new(
            ObservationId::catalog("peak-attempt"),
            ObservationValue::Unsigned(u64::from(attempts)),
        ),
        Observation::new(
            ObservationId::catalog("peak-bypass"),
            ObservationValue::Unsigned(u64::from(bypass)),
        ),
        Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact)),
    ]
}

fn assertion(scenario: SchedulerScenario) -> AssertionFailure {
    let number = match scenario {
        SchedulerScenario::DeterministicFairness => "001",
        SchedulerScenario::ResourceConservation => "002",
        SchedulerScenario::DependencyReadiness => "003",
        SchedulerScenario::WorkerOwnership => "004",
        SchedulerScenario::WorkerLoss => "005",
        SchedulerScenario::BoundedBackpressure => "006",
        SchedulerScenario::PauseAndDrain => "007",
        SchedulerScenario::CancellationTree => "008",
        SchedulerScenario::Restart => "009",
        SchedulerScenario::TerminalTruth => "010",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-SCHEDULER-CONFORMANCE-{number}"))
            .expect("static scheduler failure code"),
        ReportText::literal("D3 direct observations violated the selected scheduler contract"),
        None,
        None,
    )
}

//! Executable E3 evaluation conformance cases.

use super::{
    EvaluationConformanceFixture, EvaluationConformanceSubject, EvaluationScenario,
    EvaluationTerminal,
};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct EvaluationCase {
    descriptor: CaseDescriptor,
    scenario: EvaluationScenario,
}

impl<S: EvaluationConformanceSubject> ConformanceCase<S> for EvaluationCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral E3 conformance suite.
#[must_use]
pub fn evaluation_suite<S: EvaluationConformanceSubject + 'static>() -> StaticSuite<S> {
    use EvaluationScenario as E;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.evaluation"),
            ReportText::literal(
                "E3 frozen-input, isolated rollout, accounting, statistics, replay, and publication contract",
            ),
        ),
        vec![
            boxed("accounting", "Every rollout settles exactly once", E::CompleteAccounting),
            boxed("cancellation", "Cancellation is durable and terminal", E::Cancellation),
            boxed(
                "determinism",
                "Campaign plans and reports are reproducible",
                E::DeterministicCampaign,
            ),
            boxed("frozen-inputs", "Evaluation inputs remain digest-pinned", E::FrozenInputs),
            boxed(
                "infrastructure",
                "Infrastructure failures remain distinct from task failures",
                E::InfrastructureClassification,
            ),
            boxed("isolation", "Candidate and evaluator work remain isolated", E::RolloutIsolation),
            boxed("malformed", "Malformed evaluation frames stay inert", E::MalformedInput),
            boxed("panic", "Subject panic is contained as failure", E::PanicContainment),
            boxed(
                "publication",
                "Artifact finalization precedes report publication",
                E::PublicationOrdering,
            ),
            boxed("redaction", "Sensitive canaries stay off default surfaces", E::Redaction),
            boxed("replay", "Replay and exact retry avoid duplicate effects", E::DurableReplay),
            boxed(
                "statistics",
                "Statistical preconditions and bounds are enforced",
                E::StatisticalValidity,
            ),
            boxed("teardown", "Teardown failure remains explicit", E::TeardownIsolation),
        ],
    )
}

fn boxed<S: EvaluationConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: EvaluationScenario,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(EvaluationCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(format!("peritus.evaluation.{suffix}"))
                .expect("static evaluation case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: EvaluationConformanceSubject>(
    subject: &mut S,
    scenario: EvaluationScenario,
) -> CaseResult {
    let fixture = EvaluationConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, [0; 3], false);
    };
    let counts = [
        u64::from(observed.planned_rollouts),
        u64::from(observed.maximum_attempts),
        u64::from(observed.report_metrics),
    ];
    let bounded = observed.planned_rollouts <= fixture.maximum_rollouts()
        && observed.maximum_attempts <= fixture.maximum_attempts_per_rollout()
        && observed.report_metrics <= fixture.maximum_report_metrics()
        && observed.bounds_enforced;
    let completed = observed.terminal == EvaluationTerminal::Completed;
    let rejected = observed.terminal == EvaluationTerminal::Rejected;
    let cancelled = observed.terminal == EvaluationTerminal::Cancelled;
    let common = bounded && observed.redaction_safe && observed.non_authoritative;
    let exact = common
        && match scenario {
            EvaluationScenario::FrozenInputs => completed && observed.frozen_inputs_exact,
            EvaluationScenario::RolloutIsolation => completed && observed.isolation_exact,
            EvaluationScenario::DeterministicCampaign => completed && observed.deterministic,
            EvaluationScenario::CompleteAccounting => completed && observed.accounting_complete,
            EvaluationScenario::StatisticalValidity => completed && observed.statistics_valid,
            EvaluationScenario::InfrastructureClassification => {
                completed && observed.infrastructure_distinct
            }
            EvaluationScenario::Cancellation => cancelled && observed.cancellation_durable,
            EvaluationScenario::DurableReplay => completed && observed.replay_equivalent,
            EvaluationScenario::MalformedInput => rejected && observed.malformed_rejected,
            EvaluationScenario::PublicationOrdering => completed && observed.publication_ordered,
            EvaluationScenario::Redaction => observed.redaction_safe,
            EvaluationScenario::PanicContainment => observed.panic_contained,
            EvaluationScenario::TeardownIsolation => observed.teardown_explicit,
        };
    if exact {
        CaseResult::passed(observations(counts, true))
    } else {
        failed(scenario, counts, false)
    }
}

fn failed(scenario: EvaluationScenario, counts: [u64; 3], exact: bool) -> CaseResult {
    CaseResult::failed(observations(counts, exact), assertion(scenario))
}

fn observations(counts: [u64; 3], exact: bool) -> Vec<Observation> {
    ["planned-rollouts", "maximum-attempts", "report-metrics"]
        .into_iter()
        .zip(counts)
        .map(|(name, value)| {
            Observation::new(ObservationId::catalog(name), ObservationValue::Unsigned(value))
        })
        .chain([Observation::new(
            ObservationId::catalog("exact"),
            ObservationValue::Boolean(exact),
        )])
        .collect()
}

fn assertion(scenario: EvaluationScenario) -> AssertionFailure {
    let number = match scenario {
        EvaluationScenario::FrozenInputs => "001",
        EvaluationScenario::RolloutIsolation => "002",
        EvaluationScenario::DeterministicCampaign => "003",
        EvaluationScenario::CompleteAccounting => "004",
        EvaluationScenario::StatisticalValidity => "005",
        EvaluationScenario::InfrastructureClassification => "006",
        EvaluationScenario::Cancellation => "007",
        EvaluationScenario::DurableReplay => "008",
        EvaluationScenario::MalformedInput => "009",
        EvaluationScenario::PublicationOrdering => "010",
        EvaluationScenario::Redaction => "011",
        EvaluationScenario::PanicContainment => "012",
        EvaluationScenario::TeardownIsolation => "013",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-EVALUATION-CONFORMANCE-{number}"))
            .expect("static evaluation failure code"),
        ReportText::literal("E3 direct observations violated the selected evaluation contract"),
        None,
        None,
    )
}

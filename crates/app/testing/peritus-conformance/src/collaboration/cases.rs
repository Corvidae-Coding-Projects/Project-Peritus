//! Executable D3 collaboration conformance cases.

use super::{
    CollaborationConformanceFixture, CollaborationConformanceSubject, CollaborationScenario,
    CollaborationTerminal,
};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct CollaborationCase {
    descriptor: CaseDescriptor,
    scenario: CollaborationScenario,
}

impl<S: CollaborationConformanceSubject> ConformanceCase<S> for CollaborationCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral D3 collaboration conformance suite.
#[must_use]
pub fn collaboration_suite<S: CollaborationConformanceSubject + 'static>() -> StaticSuite<S> {
    use CollaborationScenario as C;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.collaboration"),
            ReportText::literal(
                "D3 causal delegation, messaging, joins, handoffs, cancellation, and replay",
            ),
        ),
        vec![
            boxed(
                "all-required",
                "All-required joins wait for every required child",
                C::AllRequiredJoin,
            ),
            boxed(
                "any-required",
                "Any-required joins use a declared successful child",
                C::AnyRequiredJoin,
            ),
            boxed(
                "artifact-handoff",
                "Handoffs retain exact artifact and revision evidence",
                C::ArtifactHandoff,
            ),
            boxed("bounded-graph", "Task and message graph bounds are enforced", C::BoundedGraph),
            boxed(
                "cancellation-tree",
                "Cancellation reaches every descendant",
                C::CancellationTree,
            ),
            boxed("causal-messages", "Messages retain task-local causal order", C::CausalMessages),
            boxed(
                "causal-parentage",
                "Every task reaches one root through acyclic parents",
                C::CausalParentage,
            ),
            boxed("delegation", "Delegation lifecycle and ownership are exact", C::Delegation),
            boxed("restart", "Restart and exact retry are idempotent", C::Restart),
            boxed("terminal-truth", "Only satisfied required joins can complete", C::TerminalTruth),
        ],
    )
}

fn boxed<S: CollaborationConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: CollaborationScenario,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(CollaborationCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(format!("peritus.collaboration.{suffix}"))
                .expect("static collaboration case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: CollaborationConformanceSubject>(
    subject: &mut S,
    scenario: CollaborationScenario,
) -> CaseResult {
    let fixture = CollaborationConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, [0; 4], false);
    };
    let counts = [observed.tasks, observed.peak_depth, observed.peak_fanout, observed.messages];
    let bounded = observed.tasks <= fixture.maximum_tasks()
        && observed.peak_depth <= fixture.maximum_depth()
        && observed.peak_fanout <= fixture.maximum_fanout()
        && observed.messages <= fixture.maximum_messages();
    let common = bounded && observed.parentage_valid && observed.no_implicit_success;
    let completed = observed.terminal == CollaborationTerminal::Completed;
    let exact = common
        && match scenario {
            CollaborationScenario::CausalParentage => observed.parentage_valid,
            CollaborationScenario::Delegation => observed.delegation_exact,
            CollaborationScenario::BoundedGraph => !completed && observed.bounds_enforced,
            CollaborationScenario::CausalMessages => observed.messages_causal,
            CollaborationScenario::AllRequiredJoin => observed.all_join_truthful,
            CollaborationScenario::AnyRequiredJoin => observed.any_join_truthful,
            CollaborationScenario::ArtifactHandoff => observed.handoff_exact,
            CollaborationScenario::CancellationTree => !completed && observed.cancellation_complete,
            CollaborationScenario::Restart => {
                observed.replay_equivalent && observed.idempotent_recovery
            }
            CollaborationScenario::TerminalTruth => completed && observed.no_implicit_success,
        };
    if exact {
        CaseResult::passed(observations(counts, true))
    } else {
        failed(scenario, counts, bounded)
    }
}

fn failed(scenario: CollaborationScenario, counts: [u16; 4], exact: bool) -> CaseResult {
    CaseResult::failed(observations(counts, exact), assertion(scenario))
}

fn observations(counts: [u16; 4], exact: bool) -> Vec<Observation> {
    let [tasks, depth, fanout, messages] = counts;
    vec![
        Observation::new(
            ObservationId::catalog("tasks"),
            ObservationValue::Unsigned(u64::from(tasks)),
        ),
        Observation::new(
            ObservationId::catalog("peak-depth"),
            ObservationValue::Unsigned(u64::from(depth)),
        ),
        Observation::new(
            ObservationId::catalog("peak-fanout"),
            ObservationValue::Unsigned(u64::from(fanout)),
        ),
        Observation::new(
            ObservationId::catalog("messages"),
            ObservationValue::Unsigned(u64::from(messages)),
        ),
        Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact)),
    ]
}

fn assertion(scenario: CollaborationScenario) -> AssertionFailure {
    let number = match scenario {
        CollaborationScenario::CausalParentage => "001",
        CollaborationScenario::Delegation => "002",
        CollaborationScenario::BoundedGraph => "003",
        CollaborationScenario::CausalMessages => "004",
        CollaborationScenario::AllRequiredJoin => "005",
        CollaborationScenario::AnyRequiredJoin => "006",
        CollaborationScenario::ArtifactHandoff => "007",
        CollaborationScenario::CancellationTree => "008",
        CollaborationScenario::Restart => "009",
        CollaborationScenario::TerminalTruth => "010",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-COLLABORATION-CONFORMANCE-{number}"))
            .expect("static collaboration failure code"),
        ReportText::literal("D3 direct observations violated the selected collaboration contract"),
        None,
        None,
    )
}

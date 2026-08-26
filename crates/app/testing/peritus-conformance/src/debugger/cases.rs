//! Executable E2 debugger conformance cases.

use super::{
    DebuggerConformanceFixture, DebuggerConformanceSubject, DebuggerScenario, DebuggerTerminal,
};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct DebuggerCase {
    descriptor: CaseDescriptor,
    scenario: DebuggerScenario,
}

impl<S: DebuggerConformanceSubject> ConformanceCase<S> for DebuggerCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral E2 conformance suite.
#[must_use]
pub fn debugger_suite<S: DebuggerConformanceSubject + 'static>() -> StaticSuite<S> {
    use DebuggerScenario as D;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.debugger"),
            ReportText::literal(
                "E2 bounded selection, analysis, citation, replay, model, and non-authority contract",
            ),
        ),
        vec![
            boxed("bounds", "Independent debugger limits reject overflow", D::BoundedResources),
            boxed("cancellation", "Cancellation is durable and terminal", D::Cancellation),
            boxed("citations", "Claims cite only selected evidence", D::CitationContainment),
            boxed("clustering", "Cross-run patterns are deterministic", D::DeterministicClustering),
            boxed("malformed", "Malformed debugger frames stay inert", D::MalformedInput),
            boxed(
                "model-rejection",
                "Invalid model output cannot enter a report",
                D::ModelOutputRejection,
            ),
            boxed("panic", "Subject panic is contained as failure", D::PanicContainment),
            boxed("redaction", "Sensitive canaries stay off default surfaces", D::Redaction),
            boxed("replay", "Replay and exact retry avoid duplicate effects", D::DurableReplay),
            boxed("selection", "Evidence selection is exact and immutable", D::EvidenceSelection),
            boxed("taxonomy", "Failure taxonomy is complete and closed", D::TaxonomyCompleteness),
            boxed("teardown", "Teardown failure remains explicit", D::TeardownIsolation),
            boxed(
                "timeline",
                "Causal timelines are canonical and bounded",
                D::TimelineConstruction,
            ),
        ],
    )
}

fn boxed<S: DebuggerConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: DebuggerScenario,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(DebuggerCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(format!("peritus.debugger.{suffix}"))
                .expect("static debugger case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: DebuggerConformanceSubject>(
    subject: &mut S,
    scenario: DebuggerScenario,
) -> CaseResult {
    let fixture = DebuggerConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, [0; 4], false);
    };
    let counts = [
        u64::from(observed.selected_events),
        u64::from(observed.timeline_entries),
        u64::from(observed.causes),
        u64::from(observed.patterns),
    ];
    let bounded = observed.selected_events <= fixture.maximum_selected_events()
        && observed.timeline_entries <= fixture.maximum_timeline_entries()
        && observed.causes <= fixture.maximum_causes()
        && observed.patterns <= fixture.maximum_patterns()
        && observed.bounds_enforced;
    let completed = observed.terminal == DebuggerTerminal::Completed;
    let rejected = observed.terminal == DebuggerTerminal::Rejected;
    let cancelled = observed.terminal == DebuggerTerminal::Cancelled;
    let common = bounded && observed.redaction_safe && observed.non_authoritative;
    let exact = common
        && match scenario {
            DebuggerScenario::EvidenceSelection => completed && observed.selection_exact,
            DebuggerScenario::TimelineConstruction => completed && observed.timeline_exact,
            DebuggerScenario::TaxonomyCompleteness => completed && observed.taxonomy_complete,
            DebuggerScenario::CitationContainment => completed && observed.citations_contained,
            DebuggerScenario::ModelOutputRejection => rejected && observed.model_rejection_exact,
            DebuggerScenario::DeterministicClustering => {
                completed && observed.clustering_deterministic
            }
            DebuggerScenario::DurableReplay => completed && observed.replay_equivalent,
            DebuggerScenario::Cancellation => cancelled && observed.cancellation_durable,
            DebuggerScenario::MalformedInput => rejected && observed.malformed_rejected,
            DebuggerScenario::Redaction => observed.redaction_safe,
            DebuggerScenario::BoundedResources => rejected && bounded,
            DebuggerScenario::PanicContainment => observed.panic_contained,
            DebuggerScenario::TeardownIsolation => observed.teardown_explicit,
        };
    if exact {
        CaseResult::passed(observations(counts, true))
    } else {
        failed(scenario, counts, false)
    }
}

fn failed(scenario: DebuggerScenario, counts: [u64; 4], exact: bool) -> CaseResult {
    CaseResult::failed(observations(counts, exact), assertion(scenario))
}

fn observations(counts: [u64; 4], exact: bool) -> Vec<Observation> {
    ["selected-events", "timeline-entries", "causes", "patterns"]
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

fn assertion(scenario: DebuggerScenario) -> AssertionFailure {
    let number = match scenario {
        DebuggerScenario::EvidenceSelection => "001",
        DebuggerScenario::TimelineConstruction => "002",
        DebuggerScenario::TaxonomyCompleteness => "003",
        DebuggerScenario::CitationContainment => "004",
        DebuggerScenario::ModelOutputRejection => "005",
        DebuggerScenario::DeterministicClustering => "006",
        DebuggerScenario::DurableReplay => "007",
        DebuggerScenario::Cancellation => "008",
        DebuggerScenario::MalformedInput => "009",
        DebuggerScenario::Redaction => "010",
        DebuggerScenario::BoundedResources => "011",
        DebuggerScenario::PanicContainment => "012",
        DebuggerScenario::TeardownIsolation => "013",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-DEBUGGER-CONFORMANCE-{number}"))
            .expect("static debugger failure code"),
        ReportText::literal("E2 direct observations violated the selected debugger contract"),
        None,
        None,
    )
}

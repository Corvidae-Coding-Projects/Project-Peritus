//! Executable D0 agent-loop conformance cases.

use super::{AgentConformanceFixture, AgentConformanceSubject, AgentScenario, AgentTerminal};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct AgentCase {
    descriptor: CaseDescriptor,
    scenario: AgentScenario,
}

impl<S: AgentConformanceSubject> ConformanceCase<S> for AgentCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral D0 conformance suite.
#[must_use]
pub fn agent_suite<S: AgentConformanceSubject + 'static>() -> StaticSuite<S> {
    use AgentScenario as A;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.agent"),
            ReportText::literal(
                "D0 durable context, provider, tool, control, replay, budget, and completion contract",
            ),
        ),
        vec![
            boxed(
                "active-tool-control",
                "Active tools remain owned and controllable",
                A::ActiveToolControl,
            ),
            boxed(
                "budget-exhaustion",
                "Budget exhaustion is explicit non-success",
                A::BudgetExhaustion,
            ),
            boxed(
                "cancellation",
                "Cancellation settles owned work without success",
                A::Cancellation,
            ),
            boxed(
                "completion-eligibility",
                "Completion requires current settled evidence",
                A::CompletionEligibility,
            ),
            boxed(
                "context-composition",
                "Role, memory, compaction, and rendering remain exact",
                A::ContextComposition,
            ),
            boxed(
                "crash-no-redispatch",
                "Restart never repeats an uncertain effect",
                A::CrashNoRedispatch,
            ),
            boxed(
                "inspect-edit-run-test",
                "The complete coding loop reaches one proposal",
                A::InspectEditRunTest,
            ),
            boxed(
                "parallel-ordering",
                "Parallel results retain proposal order",
                A::ParallelOrdering,
            ),
            boxed("pause-resume", "Pause and resume preserve exact continuation", A::PauseResume),
            boxed(
                "prefix-replay",
                "Every durable prefix reproduces state and next effect",
                A::PrefixReplay,
            ),
            boxed(
                "provider-reduction",
                "Provider events reduce deterministically and truthfully",
                A::ProviderReduction,
            ),
            boxed("retry-safety", "Retry and resume require exact protection", A::RetrySafety),
            boxed(
                "tool-authorization",
                "No tool effect precedes independent authority",
                A::ToolAuthorization,
            ),
        ],
    )
}

fn boxed<S: AgentConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: AgentScenario,
) -> Box<dyn ConformanceCase<S>> {
    let id = match suffix {
        "active-tool-control" => "peritus.agent.active-tool-control",
        "budget-exhaustion" => "peritus.agent.budget-exhaustion",
        "cancellation" => "peritus.agent.cancellation",
        "completion-eligibility" => "peritus.agent.completion-eligibility",
        "context-composition" => "peritus.agent.context-composition",
        "crash-no-redispatch" => "peritus.agent.crash-no-redispatch",
        "inspect-edit-run-test" => "peritus.agent.inspect-edit-run-test",
        "parallel-ordering" => "peritus.agent.parallel-ordering",
        "pause-resume" => "peritus.agent.pause-resume",
        "prefix-replay" => "peritus.agent.prefix-replay",
        "provider-reduction" => "peritus.agent.provider-reduction",
        "retry-safety" => "peritus.agent.retry-safety",
        _ => "peritus.agent.tool-authorization",
    };
    Box::new(AgentCase {
        descriptor: CaseDescriptor::new(CaseId::catalog(id), ReportText::literal(summary)),
        scenario,
    })
}

fn result<S: AgentConformanceSubject>(subject: &mut S, scenario: AgentScenario) -> CaseResult {
    let fixture = AgentConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, false);
    };
    let bounded = observed.transitions <= fixture.max_transitions()
        && observed.model_attempts <= fixture.max_model_attempts()
        && observed.tool_calls <= fixture.max_tool_calls()
        && observed.peak_parallel <= fixture.parallel_limit();
    let common = bounded
        && observed.no_implicit_success
        && observed.authority_before_effect
        && observed.ownership_accounted
        && observed.revision_exact;
    let exact = common
        && match scenario {
            AgentScenario::InspectEditRunTest => {
                observed.terminal == AgentTerminal::Completed && observed.completion_eligible
            }
            AgentScenario::PauseResume => {
                observed.terminal == AgentTerminal::Active && observed.replay_equivalent
            }
            AgentScenario::Cancellation => observed.terminal == AgentTerminal::Cancelled,
            AgentScenario::PrefixReplay => observed.replay_equivalent,
            AgentScenario::ContextComposition => observed.revision_exact,
            AgentScenario::ProviderReduction | AgentScenario::ParallelOrdering => {
                observed.stable_ordering
            }
            AgentScenario::RetrySafety | AgentScenario::CrashNoRedispatch => observed.no_redispatch,
            AgentScenario::ToolAuthorization => observed.authority_before_effect,
            AgentScenario::ActiveToolControl => observed.ownership_accounted,
            AgentScenario::BudgetExhaustion => observed.terminal == AgentTerminal::Failed,
            AgentScenario::CompletionEligibility => observed.completion_eligible,
        };
    if exact {
        CaseResult::passed(observations(observed.transitions, true))
    } else {
        failed(scenario, bounded)
    }
}

fn failed(scenario: AgentScenario, bounded: bool) -> CaseResult {
    CaseResult::failed(observations(0, bounded), assertion(scenario))
}

fn observations(transitions: u64, exact: bool) -> Vec<Observation> {
    vec![
        Observation::new(
            ObservationId::catalog("transitions"),
            ObservationValue::Unsigned(transitions),
        ),
        Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact)),
    ]
}

fn assertion(scenario: AgentScenario) -> AssertionFailure {
    let number = match scenario {
        AgentScenario::InspectEditRunTest => "001",
        AgentScenario::PauseResume => "002",
        AgentScenario::Cancellation => "003",
        AgentScenario::PrefixReplay => "004",
        AgentScenario::ContextComposition => "005",
        AgentScenario::ProviderReduction => "006",
        AgentScenario::RetrySafety => "007",
        AgentScenario::ToolAuthorization => "008",
        AgentScenario::ActiveToolControl => "009",
        AgentScenario::ParallelOrdering => "010",
        AgentScenario::BudgetExhaustion => "011",
        AgentScenario::CompletionEligibility => "012",
        AgentScenario::CrashNoRedispatch => "013",
    };
    let code = format!("PERITUS-AGENT-CONFORMANCE-{number}");
    AssertionFailure::new(
        FailureCode::new(code).expect("static conformance code"),
        ReportText::literal("D0 direct observations violated the selected agent-loop contract"),
        None,
        None,
    )
}

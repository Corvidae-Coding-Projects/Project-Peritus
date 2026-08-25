use std::sync::{Arc, Mutex};

use peritus_conformance::{
    AgentConformanceError, AgentConformanceFixture, AgentConformanceObservation,
    AgentConformanceSubject, AgentScenario, AgentTerminal, CaseDescriptor, CaseStatus,
    ConformanceFuture, ConformanceRunner, SubjectDescriptor, SubjectFactory, SubjectFailure,
    SuiteStatus, agent_suite,
};

use super::harness::{block_on, text};

struct ReferenceAgent {
    repeat_uncertain_effect: bool,
}

impl AgentConformanceSubject for ReferenceAgent {
    fn exercise(
        &mut self,
        fixture: &AgentConformanceFixture,
    ) -> Result<AgentConformanceObservation, AgentConformanceError> {
        let terminal = match fixture.scenario() {
            AgentScenario::InspectEditRunTest => AgentTerminal::Completed,
            AgentScenario::Cancellation => AgentTerminal::Cancelled,
            AgentScenario::BudgetExhaustion => AgentTerminal::Failed,
            _ => AgentTerminal::Active,
        };
        Ok(AgentConformanceObservation {
            terminal,
            transitions: 24,
            model_attempts: 3,
            tool_calls: 4,
            peak_parallel: 2,
            replay_equivalent: true,
            no_implicit_success: true,
            authority_before_effect: true,
            ownership_accounted: true,
            stable_ordering: true,
            revision_exact: true,
            completion_eligible: true,
            no_redispatch: !self.repeat_uncertain_effect,
        })
    }
}

#[derive(Clone, Copy, Default)]
struct Counts {
    created: usize,
    torn_down: usize,
}

struct Factory {
    descriptor: SubjectDescriptor,
    counts: Arc<Mutex<Counts>>,
    repeat_uncertain_effect: bool,
}

impl Factory {
    fn new(repeat_uncertain_effect: bool) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("agent-reference"), text("A2 D0 oracle")),
            counts: Arc::new(Mutex::new(Counts::default())),
            repeat_uncertain_effect,
        }
    }
}

impl SubjectFactory<ReferenceAgent> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceAgent, SubjectFailure>> {
        self.counts.lock().expect("counts lock").created += 1;
        let repeat_uncertain_effect = self.repeat_uncertain_effect;
        Box::pin(async move { Ok(ReferenceAgent { repeat_uncertain_effect }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceAgent,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        self.counts.lock().expect("counts lock").torn_down += 1;
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn agent_catalog_runs_thirteen_cases_with_fresh_subjects() {
    let factory = Factory::new(false);
    let report = block_on(ConformanceRunner::run(&agent_suite::<ReferenceAgent>(), &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 13);
    let counts = *factory.counts.lock().expect("counts lock");
    assert_eq!((counts.created, counts.torn_down), (13, 13));
}

#[test]
fn agent_catalog_detects_uncertain_effect_redispatch() {
    let factory = Factory::new(true);
    let report = block_on(ConformanceRunner::run(&agent_suite::<ReferenceAgent>(), &factory));
    let failures = report
        .cases()
        .iter()
        .filter(|case| case.status() == CaseStatus::Failed)
        .map(|case| case.descriptor().id().as_str())
        .collect::<Vec<_>>();
    assert_eq!(failures, ["peritus.agent.crash-no-redispatch", "peritus.agent.retry-safety"]);
}

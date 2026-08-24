use std::sync::{Arc, Mutex};

use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, SubjectDescriptor,
    SubjectFactory, SubjectFailure, SuiteStatus, ToolConformanceError, ToolConformanceFixture,
    ToolConformanceObservation, ToolConformanceSubject, ToolDescriptorObservation, ToolDisposition,
    ToolEffectObservation, ToolReplayMode, ToolReplayObservation, ToolResultObservation,
    ToolScenario, tool_suite,
};

use super::harness::{block_on, text};

#[derive(Clone, Copy, Default)]
enum ToolOracleBehavior {
    #[default]
    Honest,
    DispatchOnRejectedAuthority,
    ReplaySecondEffect,
}

struct ReferenceToolSubject {
    behavior: ToolOracleBehavior,
}

impl ToolConformanceSubject for ReferenceToolSubject {
    fn exercise(
        &mut self,
        fixture: &ToolConformanceFixture,
    ) -> Result<ToolConformanceObservation, ToolConformanceError> {
        let scenario = fixture.scenario();
        let rejected_authority = matches!(scenario, ToolScenario::Authorization(_));
        let replay_mode = match scenario {
            ToolScenario::Replay(mode) => Some(mode),
            _ => None,
        };
        let disposition = disposition(scenario);
        let effectful = matches!(
            scenario,
            ToolScenario::Dispatch
                | ToolScenario::ResultTruth
                | ToolScenario::Cancellation
                | ToolScenario::Deadline
        );
        let faulty_rejection = rejected_authority
            && matches!(self.behavior, ToolOracleBehavior::DispatchOnRejectedAuthority);
        let effects = if effectful || faulty_rejection {
            ToolEffectObservation::new(1, 1, 1, 1)
        } else {
            ToolEffectObservation::default()
        };
        let failed = matches!(scenario, ToolScenario::ResultTruth);
        let terminal = ToolResultObservation::new(
            matches!(disposition, ToolDisposition::Succeeded | ToolDisposition::Replayed),
            failed,
            32,
            32,
            u64::from(failed),
            false,
            true,
            failed,
        );
        let replay = replay_observation(replay_mode, self.behavior);
        Ok(ToolConformanceObservation::new(
            disposition,
            matches!(scenario, ToolScenario::DescriptorSchema).then(|| {
                ToolDescriptorObservation::new(
                    fixture.tool_name().to_owned(),
                    [7; 32],
                    [7; 32],
                    true,
                    true,
                    true,
                )
            }),
            !matches!(scenario, ToolScenario::SchemaRejection),
            matches!(scenario, ToolScenario::Exposure | ToolScenario::Dispatch),
            true,
            effects,
            terminal,
            if matches!(scenario, ToolScenario::Cancellation | ToolScenario::Deadline) {
                vec![1, 2, 3]
            } else {
                Vec::new()
            },
            matches!(scenario, ToolScenario::Cancellation | ToolScenario::Deadline),
            matches!(scenario, ToolScenario::Cancellation | ToolScenario::Deadline),
            replay,
        ))
    }
}

const fn disposition(scenario: ToolScenario) -> ToolDisposition {
    match scenario {
        ToolScenario::SchemaRejection | ToolScenario::Authorization(_) => ToolDisposition::Rejected,
        ToolScenario::ResultTruth => ToolDisposition::Failed,
        ToolScenario::Cancellation => ToolDisposition::Cancelled,
        ToolScenario::Deadline => ToolDisposition::TimedOut,
        ToolScenario::Replay(ToolReplayMode::ExactIdempotent) => ToolDisposition::Replayed,
        ToolScenario::Replay(ToolReplayMode::Conflicting | ToolReplayMode::NonIdempotent) => {
            ToolDisposition::Rejected
        }
        ToolScenario::Replay(ToolReplayMode::Indeterminate) => ToolDisposition::Indeterminate,
        _ => ToolDisposition::Succeeded,
    }
}

const fn replay_observation(
    mode: Option<ToolReplayMode>,
    behavior: ToolOracleBehavior,
) -> ToolReplayObservation {
    let second_effect =
        mode.is_some() && matches!(behavior, ToolOracleBehavior::ReplaySecondEffect);
    match mode {
        Some(ToolReplayMode::ExactIdempotent) => {
            ToolReplayObservation::new(true, second_effect, false, false)
        }
        Some(ToolReplayMode::Conflicting | ToolReplayMode::NonIdempotent) => {
            ToolReplayObservation::new(false, second_effect, true, false)
        }
        Some(ToolReplayMode::Indeterminate) => {
            ToolReplayObservation::new(false, second_effect, false, true)
        }
        None => ToolReplayObservation::new(false, false, false, false),
    }
}

#[derive(Clone, Copy, Default)]
struct FactoryCounts {
    created: usize,
    torn_down: usize,
}

struct Factory {
    descriptor: SubjectDescriptor,
    counts: Arc<Mutex<FactoryCounts>>,
    behavior: ToolOracleBehavior,
}

impl Factory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("tool-reference"), text("A2 tool oracle")),
            counts: Arc::new(Mutex::new(FactoryCounts::default())),
            behavior: ToolOracleBehavior::Honest,
        }
    }

    fn with_behavior(behavior: ToolOracleBehavior) -> Self {
        Self { behavior, ..Self::new() }
    }
}

impl SubjectFactory<ReferenceToolSubject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceToolSubject, SubjectFailure>> {
        self.counts.lock().expect("counts lock").created += 1;
        let behavior = self.behavior;
        Box::pin(async move { Ok(ReferenceToolSubject { behavior }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceToolSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        self.counts.lock().expect("counts lock").torn_down += 1;
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn tool_catalog_executes_complete_contract_with_a_fresh_subject_per_case() {
    let factory = Factory::new();
    let report = block_on(ConformanceRunner::run(&tool_suite::<ReferenceToolSubject>(), &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 8);
    let counts = *factory.counts.lock().expect("counts lock");
    assert_eq!(counts.created, 8);
    assert_eq!(counts.torn_down, 8);
}

#[test]
fn tool_catalog_detects_rejected_dispatch_and_duplicate_replay_effects() {
    for (behavior, expected) in [
        (ToolOracleBehavior::DispatchOnRejectedAuthority, "peritus.tool.authorization-no-effect"),
        (ToolOracleBehavior::ReplaySecondEffect, "peritus.tool.replay"),
    ] {
        let factory = Factory::with_behavior(behavior);
        let report =
            block_on(ConformanceRunner::run(&tool_suite::<ReferenceToolSubject>(), &factory));
        let failures = report
            .cases()
            .iter()
            .filter(|case| case.status() == CaseStatus::Failed)
            .map(|case| case.descriptor().id().as_str())
            .collect::<Vec<_>>();
        assert_eq!(failures, [expected]);
    }
}

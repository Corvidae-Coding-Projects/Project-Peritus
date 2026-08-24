use std::sync::{Arc, Mutex};

use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, ProcessConformanceError,
    ProcessConformanceFixture, ProcessConformanceObservation, ProcessConformanceSubject,
    ProcessDisposition, ProcessEffectObservation, ProcessInvocationObservation,
    ProcessOutputObservation, ProcessOutputStream, ProcessOwnershipObservation,
    ProcessRecoveryDisposition, ProcessRecoveryProbe, ProcessScenario,
    ProcessStreamOffsetObservation, ProcessTrigger, SubjectDescriptor, SubjectFactory,
    SubjectFailure, SuiteStatus, process_suite,
};

use super::harness::{block_on, text};

#[derive(Clone, Copy, Default)]
enum ProcessOracleBehavior {
    #[default]
    Honest,
    MissingOutputOffsets,
    RepeatedStdoutOffset,
    DeadlineTasksRemainLive,
}

#[derive(Default)]
struct ReferenceProcessSubject {
    behavior: ProcessOracleBehavior,
}

impl ProcessConformanceSubject for ReferenceProcessSubject {
    fn exercise(
        &mut self,
        fixture: &ProcessConformanceFixture,
    ) -> Result<ProcessConformanceObservation, ProcessConformanceError> {
        let scenario = fixture.scenario();
        let disposition = match scenario {
            ProcessScenario::Cancellation => ProcessDisposition::Cancelled,
            ProcessScenario::Deadline => ProcessDisposition::TimedOut,
            ProcessScenario::OutputBound => ProcessDisposition::OutputLimit,
            ProcessScenario::Restart(_) => ProcessDisposition::Recovered,
            ProcessScenario::Authorization(_) => ProcessDisposition::Unauthorized,
            _ => ProcessDisposition::Exited,
        };
        let trigger = match scenario {
            ProcessScenario::Cancellation => Some(ProcessTrigger::User),
            ProcessScenario::Deadline => Some(ProcessTrigger::Deadline),
            ProcessScenario::OutputBound => Some(ProcessTrigger::OutputLimit),
            _ => None,
        };
        let command = std::iter::once(fixture.executable().to_owned())
            .chain(fixture.arguments().iter().map(|value| (*value).to_owned()))
            .collect();
        let environment = fixture
            .environment()
            .iter()
            .map(|binding| (binding.name().to_owned(), binding.value().to_owned()))
            .collect();
        let invocation = ProcessInvocationObservation::new(
            command,
            fixture.working_directory().to_owned(),
            environment,
            false,
        );
        let output = output_observation(scenario, self.behavior);
        let rejected = matches!(scenario, ProcessScenario::Authorization(_));
        let ownership = ownership_observation(fixture, self.behavior, rejected);
        let effects = if rejected {
            ProcessEffectObservation::default()
        } else {
            ProcessEffectObservation::new(1, 1, 1, true)
        };
        let recovery = match scenario {
            ProcessScenario::Restart(ProcessRecoveryProbe::Terminal) => {
                Some(ProcessRecoveryDisposition::Terminal)
            }
            ProcessScenario::Restart(ProcessRecoveryProbe::ExactLive) => {
                Some(ProcessRecoveryDisposition::LiveOwned)
            }
            ProcessScenario::Restart(ProcessRecoveryProbe::Absent) => {
                Some(ProcessRecoveryDisposition::AbsentUnobserved)
            }
            ProcessScenario::Restart(
                ProcessRecoveryProbe::Mismatched | ProcessRecoveryProbe::Unverifiable,
            ) => Some(ProcessRecoveryDisposition::Indeterminate),
            _ => None,
        };
        Ok(ProcessConformanceObservation::new(
            disposition,
            trigger,
            invocation,
            output,
            ownership,
            effects,
            recovery,
            false,
            matches!(scenario, ProcessScenario::Restart(ProcessRecoveryProbe::ExactLive)),
        ))
    }
}

fn output_observation(
    scenario: ProcessScenario,
    behavior: ProcessOracleBehavior,
) -> ProcessOutputObservation {
    let (stdout, stderr, terminal, observed, retained, dropped, complete, resize) = match scenario {
        ProcessScenario::PipeStreaming => {
            (b"pipe-out".to_vec(), b"pipe-err".to_vec(), Vec::new(), 16, 16, 0, true, false)
        }
        ProcessScenario::PtyStreaming => {
            (Vec::new(), Vec::new(), b"pty-input".to_vec(), 9, 9, 0, true, true)
        }
        ProcessScenario::OutputBound => {
            (b"abcd".to_vec(), Vec::new(), Vec::new(), 8, 4, 4, false, false)
        }
        _ => (Vec::new(), Vec::new(), Vec::new(), 0, 0, 0, true, false),
    };
    let stream_offsets = match (behavior, scenario) {
        (
            ProcessOracleBehavior::MissingOutputOffsets,
            ProcessScenario::PipeStreaming
            | ProcessScenario::PtyStreaming
            | ProcessScenario::OutputBound,
        ) => Vec::new(),
        (ProcessOracleBehavior::RepeatedStdoutOffset, ProcessScenario::PipeStreaming) => vec![
            offset(ProcessOutputStream::Stdout, 0),
            offset(ProcessOutputStream::Stderr, 0),
            offset(ProcessOutputStream::Stdout, 0),
            offset(ProcessOutputStream::Stderr, 4),
        ],
        (_, ProcessScenario::PipeStreaming) => vec![
            offset(ProcessOutputStream::Stdout, 0),
            offset(ProcessOutputStream::Stderr, 0),
            offset(ProcessOutputStream::Stdout, 4),
            offset(ProcessOutputStream::Stderr, 4),
        ],
        (_, ProcessScenario::PtyStreaming) => vec![
            offset(ProcessOutputStream::Terminal, 0),
            offset(ProcessOutputStream::Terminal, 4),
            offset(ProcessOutputStream::Terminal, 8),
        ],
        (_, ProcessScenario::OutputBound) => vec![offset(ProcessOutputStream::Stdout, 0)],
        _ => Vec::new(),
    };
    ProcessOutputObservation::new(
        stdout,
        stderr,
        terminal,
        vec![1, 2, 3],
        stream_offsets,
        observed,
        retained,
        dropped,
        complete,
        matches!(scenario, ProcessScenario::PipeStreaming | ProcessScenario::PtyStreaming),
        resize,
    )
}

const fn offset(stream: ProcessOutputStream, value: u64) -> ProcessStreamOffsetObservation {
    ProcessStreamOffsetObservation::new(stream, value)
}

fn ownership_observation(
    fixture: &ProcessConformanceFixture,
    behavior: ProcessOracleBehavior,
    rejected: bool,
) -> ProcessOwnershipObservation {
    let scenario = fixture.scenario();
    let support_tasks_joined = !matches!(
        (behavior, scenario),
        (ProcessOracleBehavior::DeadlineTasksRemainLive, ProcessScenario::Deadline)
    );
    ProcessOwnershipObservation::new(
        fixture.descendant_depth(),
        !rejected,
        !rejected && support_tasks_joined,
        u64::from(!rejected && !matches!(scenario, ProcessScenario::Restart(_))),
        matches!(scenario, ProcessScenario::Cancellation | ProcessScenario::Deadline),
        matches!(scenario, ProcessScenario::Deadline),
    )
}

#[derive(Clone, Copy, Default)]
struct FactoryCounts {
    created: usize,
    torn_down: usize,
}

struct Factory {
    descriptor: SubjectDescriptor,
    counts: Arc<Mutex<FactoryCounts>>,
    behavior: ProcessOracleBehavior,
}

impl Factory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                text("process-reference"),
                text("A2 process oracle"),
            ),
            counts: Arc::new(Mutex::new(FactoryCounts::default())),
            behavior: ProcessOracleBehavior::Honest,
        }
    }

    fn with_behavior(behavior: ProcessOracleBehavior) -> Self {
        Self { behavior, ..Self::new() }
    }
}

impl SubjectFactory<ReferenceProcessSubject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceProcessSubject, SubjectFailure>> {
        self.counts.lock().expect("counts lock").created += 1;
        let behavior = self.behavior;
        Box::pin(async move { Ok(ReferenceProcessSubject { behavior }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceProcessSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        self.counts.lock().expect("counts lock").torn_down += 1;
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn process_catalog_executes_complete_contract_with_a_fresh_subject_per_case() {
    let factory = Factory::new();
    let report =
        block_on(ConformanceRunner::run(&process_suite::<ReferenceProcessSubject>(), &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 10);
    let counts = *factory.counts.lock().expect("counts lock");
    assert_eq!(counts.created, 10);
    assert_eq!(counts.torn_down, 10);
}

#[test]
fn process_catalog_rejects_invalid_offsets_and_incomplete_deadline_cleanup() {
    for (behavior, expected_failures) in [
        (
            ProcessOracleBehavior::MissingOutputOffsets,
            &[
                "peritus.process.output-bounds",
                "peritus.process.pipe-streaming",
                "peritus.process.pty-streaming",
            ] as &[_],
        ),
        (ProcessOracleBehavior::RepeatedStdoutOffset, &["peritus.process.pipe-streaming"] as &[_]),
        (ProcessOracleBehavior::DeadlineTasksRemainLive, &["peritus.process.deadline"] as &[_]),
    ] {
        let factory = Factory::with_behavior(behavior);
        let report =
            block_on(ConformanceRunner::run(&process_suite::<ReferenceProcessSubject>(), &factory));
        let failures = report
            .cases()
            .iter()
            .filter(|case| case.status() == CaseStatus::Failed)
            .map(|case| case.descriptor().id().as_str())
            .collect::<Vec<_>>();
        assert_eq!(failures, expected_failures);
    }
}

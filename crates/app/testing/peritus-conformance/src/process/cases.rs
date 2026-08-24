//! Executable C2 process-owner conformance cases.

use super::fixtures::{AUTHORITY_DRIFTS, fixture};
use super::{
    ProcessConformanceSubject, ProcessDisposition, ProcessOutputObservation, ProcessOutputStream,
    ProcessRecoveryDisposition, ProcessRecoveryProbe, ProcessScenario,
    ProcessStreamOffsetObservation, ProcessTrigger,
};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct ProcessCase {
    descriptor: CaseDescriptor,
    kind: ProcessCaseKind,
}

#[derive(Clone, Copy)]
enum ProcessCaseKind {
    LiteralInvocation,
    PipeStreaming,
    PtyStreaming,
    OutputBounds,
    Cancellation,
    Deadline,
    TreeCleanup,
    TerminalUniqueness,
    RestartClassification,
    AuthorizationNoEffect,
}

impl<S: ProcessConformanceSubject> ConformanceCase<S> for ProcessCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move {
            let exact = match self.kind {
                ProcessCaseKind::LiteralInvocation => literal_invocation(subject),
                ProcessCaseKind::PipeStreaming => pipe_streaming(subject),
                ProcessCaseKind::PtyStreaming => pty_streaming(subject),
                ProcessCaseKind::OutputBounds => output_bounds(subject),
                ProcessCaseKind::Cancellation => cancellation(subject),
                ProcessCaseKind::Deadline => deadline(subject),
                ProcessCaseKind::TreeCleanup => tree_cleanup(subject),
                ProcessCaseKind::TerminalUniqueness => terminal_uniqueness(subject),
                ProcessCaseKind::RestartClassification => restart_classification(subject),
                ProcessCaseKind::AuthorizationNoEffect => authorization_no_effect(subject),
            };
            result(exact, self.kind)
        })
    }
}

/// Returns the complete runtime-neutral C2 process conformance suite.
#[must_use]
pub fn process_suite<S: ProcessConformanceSubject + 'static>() -> StaticSuite<S> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.process"),
            ReportText::literal(
                "C2 structured process ownership, authority, and recovery contract",
            ),
        ),
        vec![
            boxed(
                "authorization-no-effect",
                "Authority drift has no target effect",
                ProcessCaseKind::AuthorizationNoEffect,
            ),
            boxed(
                "cancellation",
                "Cancellation owns and joins the complete tree",
                ProcessCaseKind::Cancellation,
            ),
            boxed(
                "deadline",
                "Deadline classification survives forced escalation",
                ProcessCaseKind::Deadline,
            ),
            boxed(
                "literal-argv-cwd-env",
                "Structured invocation remains literal and exact",
                ProcessCaseKind::LiteralInvocation,
            ),
            boxed(
                "output-bounds",
                "Output ceilings retain exact accounting",
                ProcessCaseKind::OutputBounds,
            ),
            boxed(
                "pipe-streaming",
                "Pipe streams remain separate and ordered",
                ProcessCaseKind::PipeStreaming,
            ),
            boxed(
                "pty-streaming",
                "PTY input, output, close, and resize are observed",
                ProcessCaseKind::PtyStreaming,
            ),
            boxed(
                "restart-classification",
                "Restart never guesses process identity or success",
                ProcessCaseKind::RestartClassification,
            ),
            boxed(
                "terminal-uniqueness",
                "Only one terminal record is accepted",
                ProcessCaseKind::TerminalUniqueness,
            ),
            boxed(
                "tree-cleanup",
                "Root and descendants are quiescent before publication",
                ProcessCaseKind::TreeCleanup,
            ),
        ],
    )
}

fn boxed<S: ProcessConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    kind: ProcessCaseKind,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(ProcessCase {
        descriptor: CaseDescriptor::new(
            CaseId::catalog(match suffix {
                "authorization-no-effect" => "peritus.process.authorization-no-effect",
                "cancellation" => "peritus.process.cancellation",
                "deadline" => "peritus.process.deadline",
                "literal-argv-cwd-env" => "peritus.process.literal-argv-cwd-env",
                "output-bounds" => "peritus.process.output-bounds",
                "pipe-streaming" => "peritus.process.pipe-streaming",
                "pty-streaming" => "peritus.process.pty-streaming",
                "restart-classification" => "peritus.process.restart-classification",
                "terminal-uniqueness" => "peritus.process.terminal-uniqueness",
                _ => "peritus.process.tree-cleanup",
            }),
            ReportText::literal(summary),
        ),
        kind,
    })
}

fn literal_invocation<S: ProcessConformanceSubject>(subject: &mut S) -> bool {
    let fixture = fixture(ProcessScenario::LiteralInvocation);
    let Ok(observed) = subject.exercise(&fixture) else { return false };
    let invocation = observed.invocation();
    let command = invocation.command();
    command.len() == fixture.arguments().len() + 1
        && command.first().is_some_and(|value| value == fixture.executable())
        && command[1..].iter().map(String::as_str).eq(fixture.arguments().iter().copied())
        && invocation.working_directory() == fixture.working_directory()
        && invocation.environment().len() == fixture.environment().len()
        && invocation
            .environment()
            .iter()
            .zip(fixture.environment())
            .all(|(actual, expected)| actual.0 == expected.name() && actual.1 == expected.value())
        && !invocation.shell_interpreted()
        && observed.disposition() == ProcessDisposition::Exited
}

fn pipe_streaming<S: ProcessConformanceSubject>(subject: &mut S) -> bool {
    let Ok(observed) = subject.exercise(&fixture(ProcessScenario::PipeStreaming)) else {
        return false;
    };
    let output = observed.output();
    observed.disposition() == ProcessDisposition::Exited
        && output.stdout() == b"pipe-out"
        && output.stderr() == b"pipe-err"
        && output.terminal().is_empty()
        && output.input_closed()
        && output.complete()
        && monotonic_nonzero(output.event_sequences())
        && exact_stream_offset_evidence(output)
}

fn pty_streaming<S: ProcessConformanceSubject>(subject: &mut S) -> bool {
    let Ok(observed) = subject.exercise(&fixture(ProcessScenario::PtyStreaming)) else {
        return false;
    };
    let output = observed.output();
    observed.disposition() == ProcessDisposition::Exited
        && output.stdout().is_empty()
        && output.stderr().is_empty()
        && output.terminal() == b"pty-input"
        && output.input_closed()
        && output.resize_observed()
        && output.complete()
        && monotonic_nonzero(output.event_sequences())
        && exact_stream_offset_evidence(output)
}

fn output_bounds<S: ProcessConformanceSubject>(subject: &mut S) -> bool {
    let Ok(observed) = subject.exercise(&fixture(ProcessScenario::OutputBound)) else {
        return false;
    };
    let output = observed.output();
    observed.disposition() == ProcessDisposition::OutputLimit
        && observed.trigger() == Some(ProcessTrigger::OutputLimit)
        && output.observed_bytes() == 8
        && output.retained_bytes() == 4
        && output.dropped_bytes() == 4
        && !output.complete()
        && output.retained_bytes() + output.dropped_bytes() == output.observed_bytes()
        && exact_stream_offset_evidence(output)
}

fn cancellation<S: ProcessConformanceSubject>(subject: &mut S) -> bool {
    let Ok(observed) = subject.exercise(&fixture(ProcessScenario::Cancellation)) else {
        return false;
    };
    let ownership = observed.ownership();
    observed.disposition() == ProcessDisposition::Cancelled
        && observed.trigger() == Some(ProcessTrigger::User)
        && ownership.graceful_stop_observed()
        && ownership.tree_quiescent()
        && ownership.support_tasks_joined()
        && ownership.terminal_records() == 1
}

fn deadline<S: ProcessConformanceSubject>(subject: &mut S) -> bool {
    let Ok(observed) = subject.exercise(&fixture(ProcessScenario::Deadline)) else { return false };
    let ownership = observed.ownership();
    observed.disposition() == ProcessDisposition::TimedOut
        && observed.trigger() == Some(ProcessTrigger::Deadline)
        && ownership.graceful_stop_observed()
        && ownership.forced_stop_observed()
        && ownership.tree_quiescent()
        && ownership.support_tasks_joined()
        && ownership.terminal_records() == 1
}

fn tree_cleanup<S: ProcessConformanceSubject>(subject: &mut S) -> bool {
    let fixture = fixture(ProcessScenario::TreeCleanup);
    let Ok(observed) = subject.exercise(&fixture) else { return false };
    let ownership = observed.ownership();
    ownership.descendants_observed() == fixture.descendant_depth()
        && ownership.tree_quiescent()
        && ownership.support_tasks_joined()
        && ownership.terminal_records() == 1
}

fn terminal_uniqueness<S: ProcessConformanceSubject>(subject: &mut S) -> bool {
    subject.exercise(&fixture(ProcessScenario::TerminalUniqueness)).is_ok_and(|value| {
        value.ownership().terminal_records() == 1
            && value.ownership().tree_quiescent()
            && value.ownership().support_tasks_joined()
    })
}

fn restart_classification<S: ProcessConformanceSubject>(subject: &mut S) -> bool {
    let probes = [
        (ProcessRecoveryProbe::Terminal, ProcessRecoveryDisposition::Terminal, false),
        (ProcessRecoveryProbe::ExactLive, ProcessRecoveryDisposition::LiveOwned, true),
        (ProcessRecoveryProbe::Absent, ProcessRecoveryDisposition::AbsentUnobserved, false),
        (ProcessRecoveryProbe::Mismatched, ProcessRecoveryDisposition::Indeterminate, false),
        (ProcessRecoveryProbe::Unverifiable, ProcessRecoveryDisposition::Indeterminate, false),
    ];
    probes.into_iter().all(|(probe, expected, signal)| {
        subject.exercise(&fixture(ProcessScenario::Restart(probe))).is_ok_and(|value| {
            value.disposition() == ProcessDisposition::Recovered
                && value.recovery() == Some(expected)
                && value.recovery_signal_sent() == signal
                && !value.success_inferred_without_terminal()
        })
    })
}

fn authorization_no_effect<S: ProcessConformanceSubject>(subject: &mut S) -> bool {
    AUTHORITY_DRIFTS.into_iter().all(|drift| {
        subject.exercise(&fixture(ProcessScenario::Authorization(drift))).is_ok_and(|value| {
            value.disposition() == ProcessDisposition::Unauthorized
                && !value.effects().any_effect()
                && !value.effects().authorization_consumed()
                && value.ownership().terminal_records() == 0
        })
    })
}

fn result(exact: bool, kind: ProcessCaseKind) -> CaseResult {
    let observations =
        vec![Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact))];
    if exact {
        CaseResult::passed(observations)
    } else {
        CaseResult::failed(observations, assertion(kind))
    }
}

fn assertion(kind: ProcessCaseKind) -> AssertionFailure {
    let (code, summary) = match kind {
        ProcessCaseKind::LiteralInvocation => {
            ("001", "structured invocation was not literal and exact")
        }
        ProcessCaseKind::PipeStreaming => {
            ("002", "pipe streams were not separate, bounded, and ordered")
        }
        ProcessCaseKind::PtyStreaming => {
            ("003", "PTY input, resize, close, or combined output was incomplete")
        }
        ProcessCaseKind::OutputBounds => ("004", "output ceiling accounting was not exact"),
        ProcessCaseKind::Cancellation => {
            ("005", "cancelled process tree was not completely joined")
        }
        ProcessCaseKind::Deadline => {
            ("006", "deadline or forced escalation classification was lost")
        }
        ProcessCaseKind::TreeCleanup => ("007", "owned descendants or support tasks remained live"),
        ProcessCaseKind::TerminalUniqueness => ("008", "terminal result was missing or duplicated"),
        ProcessCaseKind::RestartClassification => {
            ("009", "restart guessed identity, success, or signalling authority")
        }
        ProcessCaseKind::AuthorizationNoEffect => {
            ("010", "authority drift produced a target effect")
        }
    };
    AssertionFailure::new(
        FailureCode::catalog(match code {
            "001" => "PERITUS-PROCESS-CONFORMANCE-001",
            "002" => "PERITUS-PROCESS-CONFORMANCE-002",
            "003" => "PERITUS-PROCESS-CONFORMANCE-003",
            "004" => "PERITUS-PROCESS-CONFORMANCE-004",
            "005" => "PERITUS-PROCESS-CONFORMANCE-005",
            "006" => "PERITUS-PROCESS-CONFORMANCE-006",
            "007" => "PERITUS-PROCESS-CONFORMANCE-007",
            "008" => "PERITUS-PROCESS-CONFORMANCE-008",
            "009" => "PERITUS-PROCESS-CONFORMANCE-009",
            _ => "PERITUS-PROCESS-CONFORMANCE-010",
        }),
        ReportText::literal(summary),
        None,
        None,
    )
}

fn monotonic_nonzero(values: &[u64]) -> bool {
    values.first().is_some_and(|value| *value > 0)
        && values.windows(2).all(|pair| pair[0] < pair[1])
}

fn exact_stream_offset_evidence(output: &ProcessOutputObservation) -> bool {
    let offsets = output.stream_offsets();
    let evidence_matches_bytes = [
        (ProcessOutputStream::Stdout, !output.stdout().is_empty()),
        (ProcessOutputStream::Stderr, !output.stderr().is_empty()),
        (ProcessOutputStream::Terminal, !output.terminal().is_empty()),
    ]
    .into_iter()
    .all(|(stream, emitted)| {
        offsets.iter().any(|observation| observation.stream() == stream) == emitted
    });
    evidence_matches_bytes && stream_offsets_increase(offsets)
}

fn stream_offsets_increase(values: &[ProcessStreamOffsetObservation]) -> bool {
    let mut stdout = None;
    let mut stderr = None;
    let mut terminal = None;
    for observation in values {
        let previous = match observation.stream() {
            ProcessOutputStream::Stdout => &mut stdout,
            ProcessOutputStream::Stderr => &mut stderr,
            ProcessOutputStream::Terminal => &mut terminal,
        };
        if previous.is_some_and(|offset| offset >= observation.offset()) {
            return false;
        }
        *previous = Some(observation.offset());
    }
    true
}

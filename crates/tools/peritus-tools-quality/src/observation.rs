//! Structured execution facts and candidate B2 gate observation inputs.

use peritus_process::{OsExitObservation, OutputCompleteness, TerminalDisposition, TerminalResult};
use peritus_quality_policy::{GateAttemptOrdinal, GateFailure, GateObservation, GateOutcome};
use peritus_types::{GateExecutionId, GateId, ProcessId, RevisionTuple, Sha256Digest};
use sha2::{Digest, Sha256};

use crate::{CheckDefinition, ExpectedSuccess};

/// Exact C2 facts used to classify one quality process without claiming acceptance.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent evidence-completeness facts remain explicit for fail-closed classification"
)]
pub struct QualityExecutionObservation {
    process_id: ProcessId,
    plan_digest: Sha256Digest,
    disposition: TerminalDisposition,
    os_exit: OsExitObservation,
    output_complete: bool,
    artifact_publication_complete: bool,
    cleanup_complete: bool,
    parser_complete: bool,
    predicate_satisfied: bool,
    expected_exit_satisfied: bool,
}

impl QualityExecutionObservation {
    /// Returns the C2 process identity.
    #[must_use]
    pub const fn process_id(&self) -> ProcessId {
        self.process_id
    }
    /// Returns the exact C2 execution plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns the top-level C2 terminal classification.
    #[must_use]
    pub const fn disposition(&self) -> TerminalDisposition {
        self.disposition
    }
    /// Returns the independent operating-system exit observation.
    #[must_use]
    pub const fn os_exit(&self) -> &OsExitObservation {
        &self.os_exit
    }
    /// Returns whether every retained output stream is complete.
    #[must_use]
    pub const fn output_complete(&self) -> bool {
        self.output_complete
    }
    /// Returns whether C2 published every terminal artifact.
    #[must_use]
    pub const fn artifact_publication_complete(&self) -> bool {
        self.artifact_publication_complete
    }
    /// Returns whether process-tree cleanup and support-task joining completed.
    #[must_use]
    pub const fn cleanup_complete(&self) -> bool {
        self.cleanup_complete
    }
    /// Returns whether the configured parser produced a complete result.
    #[must_use]
    pub const fn parser_complete(&self) -> bool {
        self.parser_complete
    }
    /// Returns whether the parsed success predicate was satisfied.
    #[must_use]
    pub const fn predicate_satisfied(&self) -> bool {
        self.predicate_satisfied
    }
    /// Returns whether the independent operating-system exit matched the frozen definition.
    #[must_use]
    pub const fn expected_exit_satisfied(&self) -> bool {
        self.expected_exit_satisfied
    }
    /// Returns whether output, artifact, and cleanup evidence is complete independently of parsing.
    #[must_use]
    pub const fn execution_complete(&self) -> bool {
        self.output_complete && self.artifact_publication_complete && self.cleanup_complete
    }
    /// Returns whether all streams, artifacts, cleanup, and parser facts are complete.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.execution_complete() && self.parser_complete
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QualityTerminalClass {
    Passed,
    PredicateFailed,
    UnsuccessfulExit,
    InvalidResult,
    Infrastructure,
    Cancelled,
    TimedOut,
    Indeterminate,
}

impl QualityTerminalClass {
    pub(crate) const fn outcome(self) -> GateOutcome {
        match self {
            Self::Passed => GateOutcome::Passed,
            Self::PredicateFailed => GateOutcome::Failed(GateFailure::PredicateFailed),
            Self::UnsuccessfulExit => GateOutcome::Failed(GateFailure::UnsuccessfulExit),
            Self::InvalidResult => GateOutcome::Failed(GateFailure::InvalidResult),
            Self::Infrastructure | Self::Cancelled | Self::TimedOut | Self::Indeterminate => {
                GateOutcome::Failed(GateFailure::Infrastructure)
            }
        }
    }
}

pub(crate) struct ClassifiedQualityExecution {
    observation: QualityExecutionObservation,
    candidate: CandidateGateObservation,
    terminal_class: QualityTerminalClass,
}

impl ClassifiedQualityExecution {
    pub(crate) const fn observation(&self) -> &QualityExecutionObservation {
        &self.observation
    }

    pub(crate) const fn candidate(&self) -> CandidateGateObservation {
        self.candidate
    }

    pub(crate) const fn terminal_class(&self) -> QualityTerminalClass {
        self.terminal_class
    }
}

/// Gate identity, normalized outcome, and result digest awaiting D1 freshness binding.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CandidateGateObservation {
    gate_id: GateId,
    outcome: GateOutcome,
    result_digest: Sha256Digest,
}

impl CandidateGateObservation {
    /// Returns the gate identity declared by the selected definition.
    #[must_use]
    pub const fn gate_id(self) -> GateId {
        self.gate_id
    }
    /// Returns the candidate normalized outcome.
    #[must_use]
    pub const fn outcome(self) -> GateOutcome {
        self.outcome
    }
    /// Returns the digest of the exact execution classification inputs.
    #[must_use]
    pub const fn result_digest(self) -> Sha256Digest {
        self.result_digest
    }

    /// Binds caller-owned execution, attempt, and exact revision identity into a B2 observation.
    ///
    /// This does not assert freshness, DAG completion, or final acceptance.
    #[must_use]
    pub const fn bind(
        self,
        execution_id: GateExecutionId,
        attempt: GateAttemptOrdinal,
        revision: RevisionTuple,
    ) -> GateObservation {
        GateObservation::new(
            execution_id,
            self.gate_id,
            attempt,
            revision,
            self.outcome,
            self.result_digest,
        )
    }
}

pub(crate) fn classify(
    definition: &CheckDefinition,
    terminal: &TerminalResult,
    parser_complete: bool,
    predicate_satisfied: bool,
) -> ClassifiedQualityExecution {
    let observation = execution_observation(
        definition,
        terminal,
        parser_complete,
        predicate_satisfied,
    );
    let terminal_class = match terminal.disposition() {
        TerminalDisposition::TimedOut => QualityTerminalClass::TimedOut,
        TerminalDisposition::Cancelled => QualityTerminalClass::Cancelled,
        TerminalDisposition::RecoveryIndeterminate => QualityTerminalClass::Indeterminate,
        _ if !observation.execution_complete()
            || terminal.disposition() != TerminalDisposition::Exited =>
        {
            QualityTerminalClass::Infrastructure
        }
        _ if !parser_complete => QualityTerminalClass::InvalidResult,
        _ if !predicate_satisfied => QualityTerminalClass::PredicateFailed,
        _ if observation.expected_exit_satisfied() => QualityTerminalClass::Passed,
        _ => QualityTerminalClass::UnsuccessfulExit,
    };
    classified(definition, terminal, observation, terminal_class.outcome(), terminal_class)
}

pub(crate) fn classify_legacy(
    definition: &CheckDefinition,
    terminal: &TerminalResult,
    parser_complete: bool,
    predicate_satisfied: bool,
) -> ClassifiedQualityExecution {
    let observation = execution_observation(
        definition,
        terminal,
        parser_complete,
        predicate_satisfied,
    );
    let outcome = if !parser_complete {
        GateOutcome::Failed(GateFailure::InvalidResult)
    } else if !observation.complete() || terminal.disposition() != TerminalDisposition::Exited {
        GateOutcome::Failed(GateFailure::Infrastructure)
    } else if !predicate_satisfied {
        GateOutcome::Failed(GateFailure::PredicateFailed)
    } else if observation.expected_exit_satisfied() {
        GateOutcome::Passed
    } else {
        GateOutcome::Failed(GateFailure::UnsuccessfulExit)
    };
    let terminal_class = match terminal.disposition() {
        TerminalDisposition::TimedOut => QualityTerminalClass::TimedOut,
        TerminalDisposition::Cancelled => QualityTerminalClass::Cancelled,
        TerminalDisposition::RecoveryIndeterminate => QualityTerminalClass::Indeterminate,
        _ => match outcome {
            GateOutcome::Passed => QualityTerminalClass::Passed,
            GateOutcome::Failed(GateFailure::PredicateFailed) => {
                QualityTerminalClass::PredicateFailed
            }
            GateOutcome::Failed(GateFailure::UnsuccessfulExit) => {
                QualityTerminalClass::UnsuccessfulExit
            }
            GateOutcome::Failed(GateFailure::InvalidResult) => QualityTerminalClass::InvalidResult,
            GateOutcome::Failed(GateFailure::Infrastructure) => {
                QualityTerminalClass::Infrastructure
            }
        },
    };
    classified(definition, terminal, observation, outcome, terminal_class)
}

fn execution_observation(
    definition: &CheckDefinition,
    terminal: &TerminalResult,
    parser_complete: bool,
    predicate_satisfied: bool,
) -> QualityExecutionObservation {
    let output_complete = terminal.output().is_complete();
    let artifact_complete = terminal.artifact_publication_complete();
    let cleanup_complete = terminal.tree_cleanup_complete() && terminal.support_tasks_joined();
    QualityExecutionObservation {
        process_id: terminal.process_id(),
        plan_digest: terminal.plan_digest(),
        disposition: terminal.disposition(),
        os_exit: terminal.os_exit().clone(),
        output_complete,
        artifact_publication_complete: artifact_complete,
        cleanup_complete,
        parser_complete,
        predicate_satisfied,
        expected_exit_satisfied: expected_exit(definition.expected_success(), terminal.os_exit()),
    }
}

fn classified(
    definition: &CheckDefinition,
    terminal: &TerminalResult,
    observation: QualityExecutionObservation,
    outcome: GateOutcome,
    terminal_class: QualityTerminalClass,
) -> ClassifiedQualityExecution {
    let result_digest =
        result_digest(definition, terminal, observation.parser_complete, observation.predicate_satisfied, outcome);
    let candidate =
        CandidateGateObservation { gate_id: definition.gate_id(), outcome, result_digest };
    ClassifiedQualityExecution { observation, candidate, terminal_class }
}

const fn expected_exit(expected: ExpectedSuccess, observed: &OsExitObservation) -> bool {
    match (expected, observed) {
        (ExpectedSuccess::ExitCode(expected), OsExitObservation::Code(observed)) => {
            expected == *observed
        }
        _ => false,
    }
}

fn result_digest(
    definition: &CheckDefinition,
    terminal: &TerminalResult,
    parser_complete: bool,
    predicate_satisfied: bool,
    outcome: GateOutcome,
) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(b"peritus-c4-quality-result-v2");
    hash.update(definition.gate_id().as_bytes());
    hash.update(terminal.plan_digest().as_bytes());
    hash.update([disposition_tag(terminal.disposition())]);
    hash.update([u8::from(parser_complete)]);
    hash.update([u8::from(predicate_satisfied)]);
    hash.update([outcome_tag(outcome)]);
    for stream in terminal.output().streams() {
        hash.update([stream_tag(stream.stream())]);
        hash.update(stream.observed().to_le_bytes());
        hash.update(stream.retained().to_le_bytes());
        hash.update(stream.dropped().to_le_bytes());
        hash.update([completeness_tag(stream.completeness())]);
    }
    let mut artifacts = terminal.artifacts().to_vec();
    artifacts.sort_by_key(|artifact| stream_tag(artifact.stream()));
    for artifact in artifacts {
        hash.update([stream_tag(artifact.stream())]);
        hash.update(artifact.digest().as_bytes());
        hash.update(artifact.size().to_le_bytes());
        hash.update(artifact.start_offset().to_le_bytes());
        hash.update(artifact.end_offset().to_le_bytes());
        hash.update([completeness_tag(artifact.completeness())]);
    }
    Sha256Digest::new(hash.finalize().into())
}

const fn disposition_tag(value: TerminalDisposition) -> u8 {
    value as u8
}

const fn outcome_tag(value: GateOutcome) -> u8 {
    match value {
        GateOutcome::Passed => 1,
        GateOutcome::Failed(GateFailure::PredicateFailed) => 2,
        GateOutcome::Failed(GateFailure::UnsuccessfulExit) => 3,
        GateOutcome::Failed(GateFailure::InvalidResult) => 4,
        GateOutcome::Failed(GateFailure::Infrastructure) => 5,
    }
}

const fn stream_tag(value: peritus_process::OutputStream) -> u8 {
    match value {
        peritus_process::OutputStream::Stdout => 1,
        peritus_process::OutputStream::Stderr => 2,
        peritus_process::OutputStream::Terminal => 3,
    }
}

const fn completeness_tag(value: OutputCompleteness) -> u8 {
    match value {
        OutputCompleteness::Complete => 1,
        OutputCompleteness::Truncated => 2,
        OutputCompleteness::Incomplete => 3,
    }
}

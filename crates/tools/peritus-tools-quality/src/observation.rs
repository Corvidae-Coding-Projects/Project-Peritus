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
    /// Returns whether all streams, artifacts, cleanup, and parser facts are complete.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.output_complete
            && self.artifact_publication_complete
            && self.cleanup_complete
            && self.parser_complete
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

pub fn classify(
    definition: &CheckDefinition,
    terminal: &TerminalResult,
    parser_complete: bool,
    predicate_satisfied: bool,
) -> (QualityExecutionObservation, CandidateGateObservation) {
    let output_complete = terminal.output().is_complete();
    let artifact_complete = terminal.artifact_publication_complete();
    let cleanup_complete = terminal.tree_cleanup_complete() && terminal.support_tasks_joined();
    let observation = QualityExecutionObservation {
        process_id: terminal.process_id(),
        plan_digest: terminal.plan_digest(),
        disposition: terminal.disposition(),
        os_exit: terminal.os_exit().clone(),
        output_complete,
        artifact_publication_complete: artifact_complete,
        cleanup_complete,
        parser_complete,
    };
    let infrastructure_complete = observation.complete();
    let outcome = if !parser_complete {
        GateOutcome::Failed(GateFailure::InvalidResult)
    } else if !infrastructure_complete || terminal.disposition() != TerminalDisposition::Exited {
        GateOutcome::Failed(GateFailure::Infrastructure)
    } else if !predicate_satisfied {
        GateOutcome::Failed(GateFailure::PredicateFailed)
    } else if expected_exit(definition.expected_success(), terminal.os_exit()) {
        GateOutcome::Passed
    } else {
        GateOutcome::Failed(GateFailure::UnsuccessfulExit)
    };
    let result_digest =
        result_digest(definition, terminal, parser_complete, predicate_satisfied, outcome);
    let candidate =
        CandidateGateObservation { gate_id: definition.gate_id(), outcome, result_digest };
    (observation, candidate)
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
    hash.update(b"peritus-c4-quality-result-v1");
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

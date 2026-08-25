//! Strict typed decoding of `quality.run` terminal envelopes.

mod contract;
mod structured;

use peritus_tool_protocol::{
    ArtifactCompleteness, RecoveryRoute, ReplayIdentity, ResultStatus, Retryability, ToolResult,
    Truncation,
};
use peritus_types::{ActionId, GateId, ProcessId, Sha256Digest};

use crate::{QualityError, QualityErrorKind, run_descriptor};
use structured::{DecodedOutcome, DecodedStructured, decode_structured};

/// Exact invocation identities expected from one authorized `quality.run` dispatch.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct QualityResultBinding {
    action_id: ActionId,
    prepared_digest: Sha256Digest,
    replay_identity: ReplayIdentity,
    gate_id: GateId,
}

impl QualityResultBinding {
    /// Creates the exact terminal-envelope binding expected by D1.
    #[must_use]
    pub const fn new(
        action_id: ActionId,
        prepared_digest: Sha256Digest,
        replay_identity: ReplayIdentity,
        gate_id: GateId,
    ) -> Self {
        Self { action_id, prepared_digest, replay_identity, gate_id }
    }

    /// Returns the expected action identity.
    #[must_use]
    pub const fn action_id(self) -> ActionId {
        self.action_id
    }

    /// Returns the expected prepared-call digest.
    #[must_use]
    pub const fn prepared_digest(self) -> Sha256Digest {
        self.prepared_digest
    }

    /// Returns the expected replay identity.
    #[must_use]
    pub const fn replay_identity(self) -> ReplayIdentity {
        self.replay_identity
    }

    /// Returns the expected B2 gate identity.
    #[must_use]
    pub const fn gate_id(self) -> GateId {
        self.gate_id
    }
}

/// Closed D1-relevant interpretation of a quality terminal.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QualityTerminalKind {
    /// The frozen quality predicate passed with complete trustworthy evidence.
    Passed,
    /// The check completed authoritatively and its candidate predicate did not pass.
    CandidateFailure,
    /// C2/C3/C4 infrastructure prevented a trustworthy candidate result.
    InfrastructureFailure,
    /// Cancellation completed without success.
    Cancelled,
    /// The immutable quality deadline elapsed without success.
    TimedOut,
    /// The quality-owned structured terminal was missing, invalid, or self-contradictory.
    MalformedOutput,
    /// The candidate result was present but output, progress, cleanup, or artifacts were partial.
    IncompleteEvidence,
}

/// One complete content-addressed quality artifact projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityArtifact {
    digest: Sha256Digest,
    size: u64,
    media_type: String,
    label: String,
}

impl QualityArtifact {
    /// Returns the exact artifact digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns the exact artifact byte size.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Borrows the validated media type.
    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// Borrows the stable stream label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Strict normalized observation derived from one exact `quality.run` result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityTerminal {
    gate_id: GateId,
    kind: QualityTerminalKind,
    tool_result_digest: Sha256Digest,
    candidate_result_digest: Option<Sha256Digest>,
    execution_plan_digest: Option<Sha256Digest>,
    process_id: Option<ProcessId>,
    artifacts: Vec<QualityArtifact>,
    retryability: Retryability,
    recovery: RecoveryRoute,
}

impl QualityTerminal {
    /// Returns the exact gate identity.
    #[must_use]
    pub const fn gate_id(&self) -> GateId {
        self.gate_id
    }

    /// Returns the fail-closed terminal classification.
    #[must_use]
    pub const fn kind(&self) -> QualityTerminalKind {
        self.kind
    }

    /// Returns the digest of the complete canonical C4 terminal envelope.
    #[must_use]
    pub const fn tool_result_digest(&self) -> Sha256Digest {
        self.tool_result_digest
    }

    /// Returns the C4 candidate classification digest when it decoded exactly.
    #[must_use]
    pub const fn candidate_result_digest(&self) -> Option<Sha256Digest> {
        self.candidate_result_digest
    }

    /// Returns the exact C2 plan digest when it decoded exactly.
    #[must_use]
    pub const fn execution_plan_digest(&self) -> Option<Sha256Digest> {
        self.execution_plan_digest
    }

    /// Returns the exact C2 process identity when it decoded exactly.
    #[must_use]
    pub const fn process_id(&self) -> Option<ProcessId> {
        self.process_id
    }

    /// Borrows complete content-addressed artifacts in C4 canonical order.
    #[must_use]
    pub fn artifacts(&self) -> &[QualityArtifact] {
        &self.artifacts
    }

    /// Returns whether a fresh authorized action may retry this terminal.
    #[must_use]
    pub const fn retryability(&self) -> Retryability {
        self.retryability
    }

    /// Returns the required recovery route before a retry.
    #[must_use]
    pub const fn recovery(&self) -> RecoveryRoute {
        self.recovery
    }
}

/// Strictly decodes a `quality.run` terminal against one exact prepared invocation.
///
/// Invocation identity mismatches are returned as errors because the supplied terminal does not
/// belong to this attempt. Quality-owned malformed or incomplete output is instead represented as
/// a typed non-success observation suitable for durable D1 recording.
///
/// # Errors
/// Returns a typed mismatch when action, descriptor, prepared-call, replay, gate, or artifact
/// provenance identities differ from `binding`.
pub fn decode_quality_result(
    result: &ToolResult,
    binding: QualityResultBinding,
) -> Result<QualityTerminal, QualityError> {
    let descriptor = run_descriptor()?;
    if result.action_id() != binding.action_id
        || result.descriptor_digest() != descriptor.descriptor_digest()
        || result.prepared_digest() != binding.prepared_digest
        || result.replay_identity() != binding.replay_identity
    {
        return Err(QualityError::new(
            QualityErrorKind::InvocationMismatch,
            "quality result does not belong to the expected prepared invocation",
        ));
    }
    if result.artifacts().iter().any(|artifact| {
        artifact.provenance().action_id() != binding.action_id
            || artifact.provenance().prepared_digest() != binding.prepared_digest
    }) {
        return Err(QualityError::new(
            QualityErrorKind::InvocationMismatch,
            "quality artifact provenance differs from the expected prepared invocation",
        ));
    }

    let tool_result_digest = peritus_codec::sha256(&result.canonical_bytes());
    let decoded = result.structured().and_then(decode_structured);
    if decoded.as_ref().is_some_and(|decoded| decoded.gate_id != binding.gate_id) {
        return Err(QualityError::new(
            QualityErrorKind::InvocationMismatch,
            "quality structured result names another gate",
        ));
    }
    let artifacts = result
        .artifacts()
        .iter()
        .map(|artifact| QualityArtifact {
            digest: artifact.digest(),
            size: artifact.size(),
            media_type: artifact.media_type().as_str().to_owned(),
            label: artifact.label().as_str().to_owned(),
        })
        .collect::<Vec<_>>();
    let incomplete_artifact = result
        .artifacts()
        .iter()
        .any(|artifact| artifact.completeness() != ArtifactCompleteness::Complete);
    let incomplete_envelope = result.truncation().output != Truncation::Complete;
    let failure = result.failure_value();
    let (retryability, recovery) = failure
        .map_or((Retryability::Never, RecoveryRoute::None), |failure| {
            (failure.retryability(), failure.recovery())
        });

    let contract_consistent = contract::terminal_contract_consistent(
        result.status(),
        failure,
        decoded.as_ref().map(|value| value.outcome),
    );
    let incomplete = decoded.as_ref().is_some_and(|decoded| {
        !decoded.execution_complete
            || decoded.progress_truncated
            || incomplete_artifact
            || incomplete_envelope
    });
    let kind =
        classify_terminal(result.status(), decoded.as_ref(), contract_consistent, incomplete);
    let (retryability, recovery) = if contract_consistent {
        normalize_retry(kind, retryability, recovery)
    } else {
        (Retryability::Never, RecoveryRoute::None)
    };

    Ok(QualityTerminal {
        gate_id: binding.gate_id,
        kind,
        tool_result_digest,
        candidate_result_digest: decoded.as_ref().map(|value| value.result_digest),
        execution_plan_digest: decoded.as_ref().map(|value| value.plan_digest),
        process_id: decoded.as_ref().map(|value| value.process_id),
        artifacts,
        retryability,
        recovery,
    })
}

fn classify_terminal(
    status: ResultStatus,
    decoded: Option<&DecodedStructured>,
    contract_consistent: bool,
    incomplete: bool,
) -> QualityTerminalKind {
    if !contract_consistent {
        return QualityTerminalKind::MalformedOutput;
    }
    match status {
        ResultStatus::Cancelled => QualityTerminalKind::Cancelled,
        ResultStatus::TimedOut => QualityTerminalKind::TimedOut,
        ResultStatus::Indeterminate => QualityTerminalKind::InfrastructureFailure,
        ResultStatus::Succeeded | ResultStatus::Failed if incomplete => {
            QualityTerminalKind::IncompleteEvidence
        }
        ResultStatus::Succeeded | ResultStatus::Failed => {
            match decoded.map(|value| value.outcome) {
                Some(DecodedOutcome::Passed) => QualityTerminalKind::Passed,
                Some(DecodedOutcome::PredicateFailed | DecodedOutcome::UnsuccessfulExit) => {
                    QualityTerminalKind::CandidateFailure
                }
                Some(DecodedOutcome::InvalidResult) | None => QualityTerminalKind::MalformedOutput,
                Some(DecodedOutcome::Infrastructure) => QualityTerminalKind::InfrastructureFailure,
            }
        }
    }
}

fn normalize_retry(
    kind: QualityTerminalKind,
    retryability: Retryability,
    recovery: RecoveryRoute,
) -> (Retryability, RecoveryRoute) {
    match kind {
        QualityTerminalKind::Passed | QualityTerminalKind::CandidateFailure => {
            (Retryability::Never, RecoveryRoute::None)
        }
        QualityTerminalKind::MalformedOutput | QualityTerminalKind::IncompleteEvidence
            if retryability == Retryability::Never =>
        {
            (Retryability::NewAction, RecoveryRoute::Reauthorize)
        }
        _ => (retryability, recovery),
    }
}

#[cfg(test)]
mod tests;

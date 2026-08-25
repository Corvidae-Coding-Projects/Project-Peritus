//! Closed quality-attempt results used by the pure D1 reducer.

use peritus_tool_protocol::{RecoveryRoute, Retryability};
use peritus_tools_quality::{QualityTerminal, QualityTerminalKind};
use peritus_types::{GateId, ProcessId, Sha256Digest};

use crate::error::{GateError, GateRejection, reject};

/// Maximum output artifacts retained on one D1 attempt observation.
pub const MAX_GATE_ARTIFACTS: usize = 16;

/// Whether the reducer may schedule a fresh action after this result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RetryPermission {
    /// The result is authoritative and cannot be retried automatically.
    Never,
    /// A fresh action may be prepared if the attempt limit permits it.
    FreshAction,
    /// Recovery must first establish that the prior effect is terminal.
    AfterRecovery,
}

/// Closed recovery route retained without C4 implementation types.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryRequirement {
    /// No recovery step applies.
    None,
    /// Obtain fresh independent authority.
    Reauthorize,
    /// Reconcile the immutable workspace target.
    ReconcileWorkspace,
    /// Reconcile the owned C2 process.
    ReconcileProcess,
    /// Republish already-produced artifact bytes.
    RepublishArtifact,
    /// Require authenticated human handling.
    HumanReview,
}

/// Closed terminal class for one gate attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateOutcomeKind {
    /// Complete trustworthy evidence reports that the gate passed.
    Passed,
    /// The frozen gate predicate authoritatively failed.
    CandidateFailure,
    /// Infrastructure prevented a trustworthy candidate result.
    InfrastructureFailure,
    /// Cancellation completed without success.
    Cancelled,
    /// The attempt deadline elapsed without success.
    TimedOut,
    /// The quality terminal could not be decoded or contradicted itself.
    MalformedOutput,
    /// Some output, cleanup, progress, or artifact evidence was incomplete.
    IncompleteEvidence,
}

/// One content-addressed output artifact with no filesystem path or raw bytes.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GateArtifact {
    digest: Sha256Digest,
    size: u64,
    media_type: String,
    label: String,
}

impl GateArtifact {
    pub(crate) fn from_parts(
        digest: Sha256Digest,
        size: u64,
        media_type: String,
        label: String,
    ) -> Result<Self, GateError> {
        if size == 0
            || media_type.is_empty()
            || media_type.len() > 255
            || label.is_empty()
            || label.len() > 256
            || media_type.chars().any(char::is_control)
            || label.chars().any(char::is_control)
        {
            return Err(reject(
                GateRejection::EvidenceInvalid,
                "decoded gate artifact metadata is invalid",
            ));
        }
        Ok(Self { digest, size, media_type, label })
    }
    /// Returns the exact content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns the exact published byte size.
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Borrows the bounded media type.
    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    /// Borrows the bounded safe label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Complete normalized attempt terminal safe for durable recording.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateAttemptResult {
    gate_id: GateId,
    kind: GateOutcomeKind,
    tool_result_digest: Sha256Digest,
    candidate_result_digest: Option<Sha256Digest>,
    execution_plan_digest: Option<Sha256Digest>,
    process_id: Option<ProcessId>,
    artifacts: Vec<GateArtifact>,
    retry: RetryPermission,
    recovery: RecoveryRequirement,
}

impl GateAttemptResult {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        gate_id: GateId,
        kind: GateOutcomeKind,
        tool_result_digest: Sha256Digest,
        candidate_result_digest: Option<Sha256Digest>,
        execution_plan_digest: Option<Sha256Digest>,
        process_id: Option<ProcessId>,
        artifacts: Vec<GateArtifact>,
        retry: RetryPermission,
        recovery: RecoveryRequirement,
    ) -> Result<Self, GateError> {
        if artifacts.len() > MAX_GATE_ARTIFACTS
            || artifacts.windows(2).any(|pair| pair[0] >= pair[1])
            || artifacts.windows(2).any(|pair| pair[0].digest == pair[1].digest)
            || (kind == GateOutcomeKind::Passed
                && (candidate_result_digest.is_none()
                    || execution_plan_digest.is_none()
                    || process_id.is_none()
                    || retry != RetryPermission::Never
                    || recovery != RecoveryRequirement::None))
            || (kind == GateOutcomeKind::CandidateFailure
                && (retry != RetryPermission::Never || recovery != RecoveryRequirement::None))
        {
            return Err(reject(
                GateRejection::EvidenceInvalid,
                "decoded gate attempt result violates canonical or pass invariants",
            ));
        }
        Ok(Self {
            gate_id,
            kind,
            tool_result_digest,
            candidate_result_digest,
            execution_plan_digest,
            process_id,
            artifacts,
            retry,
            recovery,
        })
    }
    /// Converts a strict C4 quality terminal into bounded D1 data.
    ///
    /// # Errors
    /// Rejects another gate identity, excessive/duplicate artifacts, or a purported pass without
    /// complete candidate, C2 plan, and process digests.
    pub fn from_quality(
        expected_gate: GateId,
        terminal: &QualityTerminal,
    ) -> Result<Self, GateError> {
        if terminal.gate_id() != expected_gate {
            return Err(reject(
                GateRejection::IdentityMismatch,
                "quality terminal belongs to another gate",
            ));
        }
        if terminal.artifacts().len() > MAX_GATE_ARTIFACTS {
            return Err(reject(
                GateRejection::LimitExceeded,
                "quality terminal exceeds the D1 artifact bound",
            ));
        }
        let mut artifacts = terminal
            .artifacts()
            .iter()
            .map(|artifact| GateArtifact {
                digest: artifact.digest(),
                size: artifact.size(),
                media_type: artifact.media_type().to_owned(),
                label: artifact.label().to_owned(),
            })
            .collect::<Vec<_>>();
        artifacts.sort_unstable();
        if artifacts.windows(2).any(|pair| pair[0].digest == pair[1].digest) {
            return Err(reject(
                GateRejection::EvidenceInvalid,
                "quality terminal contains duplicate artifact digests",
            ));
        }
        let kind = map_kind(terminal.kind());
        if kind == GateOutcomeKind::Passed
            && (terminal.candidate_result_digest().is_none()
                || terminal.execution_plan_digest().is_none()
                || terminal.process_id().is_none())
        {
            return Err(reject(
                GateRejection::EvidenceInvalid,
                "passing quality terminal lacks exact candidate or execution provenance",
            ));
        }
        let retry = match terminal.retryability() {
            Retryability::Never => RetryPermission::Never,
            Retryability::NewAction => RetryPermission::FreshAction,
            Retryability::AfterRecovery => RetryPermission::AfterRecovery,
        };
        Ok(Self {
            gate_id: expected_gate,
            kind,
            tool_result_digest: terminal.tool_result_digest(),
            candidate_result_digest: terminal.candidate_result_digest(),
            execution_plan_digest: terminal.execution_plan_digest(),
            process_id: terminal.process_id(),
            artifacts,
            retry,
            recovery: map_recovery(terminal.recovery()),
        })
    }

    /// Returns the exact gate identity.
    #[must_use]
    pub const fn gate_id(&self) -> GateId {
        self.gate_id
    }

    /// Returns the closed result class.
    #[must_use]
    pub const fn kind(&self) -> GateOutcomeKind {
        self.kind
    }

    /// Returns the digest of the complete canonical C4 terminal.
    #[must_use]
    pub const fn tool_result_digest(&self) -> Sha256Digest {
        self.tool_result_digest
    }

    /// Returns the normalized candidate digest when available.
    #[must_use]
    pub const fn candidate_result_digest(&self) -> Option<Sha256Digest> {
        self.candidate_result_digest
    }

    /// Returns the exact C2 plan digest when available.
    #[must_use]
    pub const fn execution_plan_digest(&self) -> Option<Sha256Digest> {
        self.execution_plan_digest
    }

    /// Returns the C2 process identity when available.
    #[must_use]
    pub const fn process_id(&self) -> Option<ProcessId> {
        self.process_id
    }

    /// Borrows canonical unique artifact projections.
    #[must_use]
    pub fn artifacts(&self) -> &[GateArtifact] {
        &self.artifacts
    }

    /// Returns retry permission reported by the normalized terminal.
    #[must_use]
    pub const fn retry_permission(&self) -> RetryPermission {
        self.retry
    }

    /// Returns the required pre-retry recovery route.
    #[must_use]
    pub const fn recovery_requirement(&self) -> RecoveryRequirement {
        self.recovery
    }

    /// Returns whether this result alone is eligible to satisfy a gate.
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self.kind, GateOutcomeKind::Passed)
    }
}

const fn map_kind(kind: QualityTerminalKind) -> GateOutcomeKind {
    match kind {
        QualityTerminalKind::Passed => GateOutcomeKind::Passed,
        QualityTerminalKind::CandidateFailure => GateOutcomeKind::CandidateFailure,
        QualityTerminalKind::InfrastructureFailure => GateOutcomeKind::InfrastructureFailure,
        QualityTerminalKind::Cancelled => GateOutcomeKind::Cancelled,
        QualityTerminalKind::TimedOut => GateOutcomeKind::TimedOut,
        QualityTerminalKind::MalformedOutput => GateOutcomeKind::MalformedOutput,
        QualityTerminalKind::IncompleteEvidence => GateOutcomeKind::IncompleteEvidence,
    }
}

const fn map_recovery(route: RecoveryRoute) -> RecoveryRequirement {
    match route {
        RecoveryRoute::None => RecoveryRequirement::None,
        RecoveryRoute::Reauthorize => RecoveryRequirement::Reauthorize,
        RecoveryRoute::ReconcileWorkspace => RecoveryRequirement::ReconcileWorkspace,
        RecoveryRoute::ReconcileProcess => RecoveryRequirement::ReconcileProcess,
        RecoveryRoute::RepublishArtifact => RecoveryRequirement::RepublishArtifact,
        RecoveryRoute::HumanReview => RecoveryRequirement::HumanReview,
    }
}

//! Truthful terminal classifications for the orchestration lifecycle.

use peritus_types::{RevisionTuple, Sha256Digest};
use sha2::{Digest, Sha256};

use crate::{OrchestratorError, OrchestratorErrorKind, OrchestratorRecoveryAction};

/// Closed externally visible terminal outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OrchestratorTerminalKind {
    /// B0 durably accepted the exact evaluated revision.
    Accepted,
    /// Authoritative policy or review truth rejected the candidate.
    Rejected,
    /// A deterministic infrastructure or child failure ended the run.
    Failed,
    /// A configured finite budget was consumed.
    Exhausted,
    /// Safe autonomous progress is impossible without a human decision.
    NeedsHuman,
    /// Cancellation completed after owned work was reconciled.
    Cancelled,
}

/// Closed causal reason behind a terminal outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TerminalCause {
    /// B0 durably accepted the exact revision.
    KernelAccepted,
    /// An explicit authoritative rejection was recorded.
    ExplicitRejection,
    /// An explicit unrecoverable failure observation was recorded.
    ExplicitFailure,
    /// An explicit bounded-budget exhaustion observation was recorded.
    ExplicitExhaustion,
    /// The writer failed before producing a valid candidate.
    WriterFailed,
    /// The fixer failed before producing a checked successor candidate.
    FixerFailed,
    /// Gates proved the candidate invalid.
    GateCandidateFailed,
    /// Gate infrastructure failed deterministically.
    GateInfrastructureFailed,
    /// Independent review failed deterministically.
    ReviewFailed,
    /// Review truth cannot be resolved autonomously.
    ReviewNeedsHuman,
    /// Repeated review/fix cycles no longer make progress.
    ReviewOscillation,
    /// B2 acceptance evaluation failed without an acceptable certificate.
    AcceptanceEvaluationFailed,
    /// B0 acceptance processing failed before authoritative truth was recorded.
    KernelAcceptanceFailed,
    /// D3 scheduler or collaboration terminal truth was non-successful.
    CoordinationFailed,
    /// B0 requested changes when no bounded autonomous fix branch remained.
    KernelNeedsChanges,
    /// The writer-cycle budget was consumed.
    WriterLimit,
    /// The fixer-cycle budget was consumed.
    FixerLimit,
    /// The gate-cycle budget was consumed.
    GateLimit,
    /// The review-cycle budget was consumed.
    ReviewLimit,
    /// The candidate-revision budget was consumed.
    RevisionLimit,
    /// The role-handoff budget was consumed.
    HandoffLimit,
    /// The external-directive budget was consumed.
    DirectiveLimit,
    /// The retained-observation budget was consumed.
    ObservationLimit,
    /// A child terminal could not be reconciled unambiguously.
    ChildAmbiguous,
    /// Cancellation reconciled every owned child.
    CancellationReconciled,
}

impl TerminalCause {
    /// Returns the only terminal class permitted for this cause.
    #[must_use]
    pub const fn terminal_kind(self) -> OrchestratorTerminalKind {
        match self {
            Self::KernelAccepted => OrchestratorTerminalKind::Accepted,
            Self::ExplicitRejection | Self::GateCandidateFailed => {
                OrchestratorTerminalKind::Rejected
            }
            Self::ExplicitFailure
            | Self::WriterFailed
            | Self::FixerFailed
            | Self::GateInfrastructureFailed
            | Self::ReviewFailed
            | Self::AcceptanceEvaluationFailed
            | Self::KernelAcceptanceFailed
            | Self::CoordinationFailed => OrchestratorTerminalKind::Failed,
            Self::ExplicitExhaustion
            | Self::WriterLimit
            | Self::FixerLimit
            | Self::GateLimit
            | Self::ReviewLimit
            | Self::RevisionLimit
            | Self::HandoffLimit
            | Self::DirectiveLimit
            | Self::ObservationLimit => OrchestratorTerminalKind::Exhausted,
            Self::ReviewNeedsHuman
            | Self::ReviewOscillation
            | Self::KernelNeedsChanges
            | Self::ChildAmbiguous => OrchestratorTerminalKind::NeedsHuman,
            Self::CancellationReconciled => OrchestratorTerminalKind::Cancelled,
        }
    }
}

/// Immutable terminal fact bound to one exact revision and causal digest.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OrchestratorTerminal {
    kind: OrchestratorTerminalKind,
    cause: TerminalCause,
    cause_digest: Sha256Digest,
    revision: RevisionTuple,
    digest: Sha256Digest,
}

impl OrchestratorTerminal {
    /// Creates a terminal fact with the class implied by its cause.
    ///
    /// # Errors
    /// Rejects a zero causal digest.
    pub fn new(
        cause: TerminalCause,
        cause_digest: Sha256Digest,
        revision: RevisionTuple,
    ) -> Result<Self, OrchestratorError> {
        if zero_digest(cause_digest) {
            return Err(invalid("terminal cause digest must be nonzero"));
        }
        let kind = cause.terminal_kind();
        let digest = terminal_digest(kind, cause, cause_digest, revision);
        Ok(Self { kind, cause, cause_digest, revision, digest })
    }

    pub(crate) const fn from_wire(
        kind: OrchestratorTerminalKind,
        cause: TerminalCause,
        cause_digest: Sha256Digest,
        revision: RevisionTuple,
        digest: Sha256Digest,
    ) -> Self {
        Self { kind, cause, cause_digest, revision, digest }
    }

    pub(crate) fn validate(&self) -> Result<(), OrchestratorError> {
        if zero_digest(self.cause_digest)
            || self.kind != self.cause.terminal_kind()
            || self.digest
                != terminal_digest(self.kind, self.cause, self.cause_digest, self.revision)
        {
            Err(invalid("terminal fact is inconsistent or noncanonical"))
        } else {
            Ok(())
        }
    }

    /// Returns the truthful terminal class.
    #[must_use]
    pub const fn kind(self) -> OrchestratorTerminalKind {
        self.kind
    }

    /// Returns the closed terminal cause.
    #[must_use]
    pub const fn cause(self) -> TerminalCause {
        self.cause
    }

    /// Returns the exact causal evidence digest.
    #[must_use]
    pub const fn cause_digest(self) -> Sha256Digest {
        self.cause_digest
    }

    /// Returns the exact terminal revision.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }

    /// Returns the canonical terminal digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}

fn terminal_digest(
    kind: OrchestratorTerminalKind,
    cause: TerminalCause,
    cause_digest: Sha256Digest,
    revision: RevisionTuple,
) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.orchestrator.terminal.v1");
    hasher.update([terminal_kind_tag(kind), terminal_cause_tag(cause)]);
    hasher.update(cause_digest.as_bytes());
    hasher.update(revision.acceptance_spec_id().as_bytes());
    hasher.update(revision.harness_id().as_bytes());
    hasher.update(revision.workspace_id().as_bytes());
    hasher.update(revision.workspace_generation().get().to_be_bytes());
    hasher.update(revision.workspace_revision().get().to_be_bytes());
    hasher.update(revision.policy_id().as_bytes());
    hasher.update(revision.provider_profile_id().as_bytes());
    Sha256Digest::new(hasher.finalize().into())
}

const fn terminal_kind_tag(kind: OrchestratorTerminalKind) -> u8 {
    match kind {
        OrchestratorTerminalKind::Accepted => 1,
        OrchestratorTerminalKind::Rejected => 2,
        OrchestratorTerminalKind::Failed => 3,
        OrchestratorTerminalKind::Exhausted => 4,
        OrchestratorTerminalKind::NeedsHuman => 5,
        OrchestratorTerminalKind::Cancelled => 6,
    }
}

const fn terminal_cause_tag(cause: TerminalCause) -> u8 {
    match cause {
        TerminalCause::KernelAccepted => 1,
        TerminalCause::ExplicitRejection => 2,
        TerminalCause::ExplicitFailure => 3,
        TerminalCause::ExplicitExhaustion => 4,
        TerminalCause::WriterFailed => 5,
        TerminalCause::FixerFailed => 6,
        TerminalCause::GateCandidateFailed => 7,
        TerminalCause::GateInfrastructureFailed => 8,
        TerminalCause::ReviewFailed => 9,
        TerminalCause::ReviewNeedsHuman => 10,
        TerminalCause::ReviewOscillation => 11,
        TerminalCause::AcceptanceEvaluationFailed => 12,
        TerminalCause::KernelAcceptanceFailed => 13,
        TerminalCause::CoordinationFailed => 14,
        TerminalCause::KernelNeedsChanges => 15,
        TerminalCause::WriterLimit => 16,
        TerminalCause::FixerLimit => 17,
        TerminalCause::GateLimit => 18,
        TerminalCause::ReviewLimit => 19,
        TerminalCause::RevisionLimit => 20,
        TerminalCause::HandoffLimit => 21,
        TerminalCause::DirectiveLimit => 22,
        TerminalCause::ObservationLimit => 23,
        TerminalCause::ChildAmbiguous => 24,
        TerminalCause::CancellationReconciled => 25,
    }
}

fn zero_digest(digest: Sha256Digest) -> bool {
    digest.as_bytes().iter().all(|byte| *byte == 0)
}

const fn invalid(detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(
        OrchestratorErrorKind::InvalidInput,
        OrchestratorRecoveryAction::CorrectInput,
        detail,
    )
}

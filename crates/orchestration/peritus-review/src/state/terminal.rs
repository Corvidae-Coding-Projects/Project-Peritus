//! Truthful immutable terminal summary for a D2 review aggregate.

use peritus_types::{FindingId, Sha256Digest};

use crate::{OscillationReport, QuorumReport};

/// Closed truthful D2 terminal kind.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReviewTerminalKind {
    /// Current quorum and finding conservation are both complete.
    Completed,
    /// Review cannot continue autonomously and retained evidence needs human action.
    NeedsHuman,
    /// Explicitly unrecoverable or impossible state was recorded.
    Failed,
    /// The run was cancelled without success.
    Cancelled,
}

/// Complete terminal review summary; it never claims overall run acceptance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewTerminal {
    pub(super) kind: ReviewTerminalKind,
    pub(super) unconserved_findings: Vec<FindingId>,
    pub(super) quorum: QuorumReport,
    pub(super) oscillation: OscillationReport,
    pub(super) cause_digest: Sha256Digest,
    pub(super) digest: Sha256Digest,
}

impl ReviewTerminal {
    pub(crate) const fn from_wire(
        kind: ReviewTerminalKind,
        unconserved_findings: Vec<FindingId>,
        quorum: QuorumReport,
        oscillation: OscillationReport,
        cause_digest: Sha256Digest,
        digest: Sha256Digest,
    ) -> Self {
        Self { kind, unconserved_findings, quorum, oscillation, cause_digest, digest }
    }

    /// Returns the closed terminal kind.
    #[must_use]
    pub const fn kind(&self) -> ReviewTerminalKind {
        self.kind
    }

    /// Returns canonical current findings lacking a permitted closure.
    #[must_use]
    pub const fn unconserved_findings(&self) -> &[FindingId] {
        self.unconserved_findings.as_slice()
    }

    /// Returns the terminal quorum snapshot.
    #[must_use]
    pub const fn quorum(&self) -> &QuorumReport {
        &self.quorum
    }

    /// Returns the terminal oscillation/exhaustion snapshot.
    #[must_use]
    pub const fn oscillation(&self) -> &OscillationReport {
        &self.oscillation
    }

    /// Returns the explicit failure/cancellation/exhaustion cause digest, or zero for finalization.
    #[must_use]
    pub const fn cause_digest(&self) -> Sha256Digest {
        self.cause_digest
    }

    /// Returns the canonical terminal digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

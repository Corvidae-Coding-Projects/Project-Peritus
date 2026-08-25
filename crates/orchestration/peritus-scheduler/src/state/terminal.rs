//! Truthful scheduler terminal evaluation.

use peritus_types::Sha256Digest;

use crate::{WorkId, WorkRecord, WorkTerminal};

/// Stable scheduler terminal classification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SchedulerTerminalKind {
    /// Every admitted work item succeeded.
    Completed,
    /// At least one work item failed or was abandoned.
    Failed,
    /// At least one work item was blocked by a failed dependency.
    DependencyFailed,
    /// At least one work item has ambiguous external outcome.
    Ambiguous,
    /// At least one work item exhausted its attempt bound.
    Exhausted,
    /// At least one work item was cancelled.
    Cancelled,
}

/// Immutable truthful final scheduler summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchedulerTerminal {
    kind: SchedulerTerminalKind,
    non_successful_work: Vec<WorkId>,
    digest: Sha256Digest,
}

impl SchedulerTerminal {
    pub(crate) fn evaluate(work: &[WorkRecord]) -> Self {
        let mut kind = SchedulerTerminalKind::Completed;
        let mut non_successful_work = Vec::new();
        for record in work {
            let candidate = match record.terminal() {
                Some(WorkTerminal::Succeeded { .. }) => continue,
                Some(WorkTerminal::Failed { .. } | WorkTerminal::Abandoned { .. }) => {
                    SchedulerTerminalKind::Failed
                }
                Some(WorkTerminal::DependencyFailed { .. }) => {
                    SchedulerTerminalKind::DependencyFailed
                }
                Some(WorkTerminal::Ambiguous { .. }) => SchedulerTerminalKind::Ambiguous,
                Some(WorkTerminal::Exhausted { .. }) => SchedulerTerminalKind::Exhausted,
                Some(WorkTerminal::Cancelled) => SchedulerTerminalKind::Cancelled,
                None => SchedulerTerminalKind::Failed,
            };
            if terminal_precedence(candidate) > terminal_precedence(kind) {
                kind = candidate;
            }
            non_successful_work.push(record.spec().id());
        }
        let mut terminal = Self { kind, non_successful_work, digest: Sha256Digest::new([0; 32]) };
        terminal.digest = crate::canonical::terminal_digest(&terminal);
        terminal
    }

    /// Returns overall terminal classification.
    #[must_use]
    pub const fn kind(&self) -> SchedulerTerminalKind {
        self.kind
    }

    /// Borrows canonical non-successful work identities.
    #[must_use]
    pub fn non_successful_work(&self) -> &[WorkId] {
        &self.non_successful_work
    }

    /// Returns canonical terminal digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    pub(crate) const fn from_wire(
        kind: SchedulerTerminalKind,
        non_successful_work: Vec<WorkId>,
        digest: Sha256Digest,
    ) -> Self {
        Self { kind, non_successful_work, digest }
    }
}

const fn terminal_precedence(kind: SchedulerTerminalKind) -> u8 {
    match kind {
        SchedulerTerminalKind::Completed => 0,
        SchedulerTerminalKind::Cancelled => 1,
        SchedulerTerminalKind::Failed => 2,
        SchedulerTerminalKind::DependencyFailed => 3,
        SchedulerTerminalKind::Exhausted => 4,
        SchedulerTerminalKind::Ambiguous => 5,
    }
}

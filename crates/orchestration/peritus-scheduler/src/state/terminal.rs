//! Truthful scheduler terminal evaluation.

mod evaluation;

use peritus_types::Sha256Digest;

use crate::{WorkId, WorkRecord};
use vstd::prelude::*;

verus! {

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

} // verus!

verus! {

/// Immutable truthful final scheduler summary.
#[derive(Debug, Eq, PartialEq)]
pub struct SchedulerTerminal {
    kind: SchedulerTerminalKind,
    non_successful_work: Vec<WorkId>,
    digest: Sha256Digest,
}

impl SchedulerTerminal {
    /// Relates an exact semantic clone of a terminal summary.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.kind == right.kind
            && left.non_successful_work@ == right.non_successful_work@
            && left.digest == right.digest
    }
}

impl Clone for SchedulerTerminal {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            kind: self.kind,
            non_successful_work: self.non_successful_work.clone(),
            digest: self.digest,
        }
    }
}

} // verus!

impl SchedulerTerminal {
    pub(crate) fn evaluate(work: &[WorkRecord]) -> Self {
        let (kind, non_successful_work) = evaluation::summarize(work);
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

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

/// Immutable truthful final scheduler summary.
#[derive(Debug, Eq, PartialEq)]
pub struct SchedulerTerminal {
    kind: SchedulerTerminalKind,
    non_successful_work: Vec<WorkId>,
    digest: Sha256Digest,
}

impl SchedulerTerminal {
    /// Returns the mathematical terminal classification.
    pub closed spec fn spec_kind(&self) -> SchedulerTerminalKind { self.kind }

    /// Returns the mathematical canonical non-success identity sequence.
    pub closed spec fn spec_non_successful_work(&self) -> Seq<WorkId> {
        self.non_successful_work@
    }

    /// Returns the mathematical terminal digest.
    pub closed spec fn spec_digest(&self) -> Sha256Digest { self.digest }

    /// Relates an exact semantic clone of a terminal summary.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_kind() == right.spec_kind()
            && left.spec_non_successful_work() == right.spec_non_successful_work()
            && left.spec_digest() == right.spec_digest()
    }

    /// Relates exact evaluator output while treating hashing as a supplied-value boundary.
    pub open spec fn evaluation_matches(
        work: Seq<WorkRecord>,
        supplied_digest: Sha256Digest,
        terminal: &Self,
    ) -> bool {
        terminal.spec_digest() == supplied_digest
            && Self::summary_matches(
                work,
                terminal.spec_kind(),
                terminal.spec_non_successful_work(),
            )
    }

    /// Builds the exact classified summary with an ordinary-code supplied digest.
    pub(crate) fn evaluate_with_digest(
        work: &[WorkRecord],
        supplied_digest: Sha256Digest,
    ) -> (result: Self)
        ensures Self::evaluation_matches(work@, supplied_digest, &result),
    {
        let (kind, non_successful_work) = evaluation::summarize(work);
        Self { kind, non_successful_work, digest: supplied_digest }
    }

    /// Returns overall terminal classification.
    #[must_use]
    pub const fn kind(&self) -> (result: SchedulerTerminalKind)
        ensures result == self.spec_kind(),
    {
        self.kind
    }

    /// Borrows canonical non-successful work identities.
    #[must_use]
    pub fn non_successful_work(&self) -> (result: &[WorkId])
        ensures result@ == self.spec_non_successful_work(),
    {
        &self.non_successful_work
    }

    /// Returns canonical terminal digest.
    #[must_use]
    pub const fn digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_digest(),
    {
        self.digest
    }

    #[must_use]
    pub(crate) fn with_digest(
        self,
        digest: Sha256Digest,
    ) -> (result: Self)
        ensures
            result.spec_kind() == self.spec_kind(),
            result.spec_non_successful_work() == self.spec_non_successful_work(),
            result.spec_digest() == digest,
    {
        Self {
            kind: self.kind,
            non_successful_work: self.non_successful_work,
            digest,
        }
    }

    pub(crate) const fn from_wire(
        kind: SchedulerTerminalKind,
        non_successful_work: Vec<WorkId>,
        digest: Sha256Digest,
    ) -> (result: Self)
        ensures
            result.spec_kind() == kind,
            result.spec_non_successful_work() == non_successful_work@,
            result.spec_digest() == digest,
    {
        Self { kind, non_successful_work, digest }
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
        let mut terminal = Self::evaluate_with_digest(work, Sha256Digest::new([0; 32]));
        terminal.digest = crate::canonical::terminal_digest(&terminal);
        terminal
    }
}

//! Truthful scheduler terminal evaluation.

mod evaluation;

use peritus_types::Sha256Digest;

use crate::{WorkId, WorkRecord};

#[doc = " Stable scheduler terminal classification."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SchedulerTerminalKind {
    #[doc = " Every admitted work item succeeded."]
    Completed,
    #[doc = " At least one work item failed or was abandoned."]
    Failed,
    #[doc = " At least one work item was blocked by a failed dependency."]
    DependencyFailed,
    #[doc = " At least one work item has ambiguous external outcome."]
    Ambiguous,
    #[doc = " At least one work item exhausted its attempt bound."]
    Exhausted,
    #[doc = " At least one work item was cancelled."]
    Cancelled,
}
#[doc = " Immutable truthful final scheduler summary."]
#[derive(Debug, Eq, PartialEq)]
pub struct SchedulerTerminal {
    kind: SchedulerTerminalKind,
    non_successful_work: Vec<WorkId>,
    digest: Sha256Digest,
}
impl SchedulerTerminal {
    #[doc = " Builds the exact classified summary with an ordinary-code supplied digest."]
    pub(crate) fn evaluate_with_digest(work: &[WorkRecord], supplied_digest: Sha256Digest) -> Self {
        let (kind, non_successful_work) = evaluation::summarize(work);
        Self { kind, non_successful_work, digest: supplied_digest }
    }
    #[doc = " Returns overall terminal classification."]
    #[must_use]
    pub const fn kind(&self) -> SchedulerTerminalKind {
        self.kind
    }
    #[doc = " Borrows canonical non-successful work identities."]
    #[must_use]
    pub fn non_successful_work(&self) -> &[WorkId] {
        &self.non_successful_work
    }
    #[doc = " Returns canonical terminal digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    #[must_use]
    pub(crate) fn with_digest(self, digest: Sha256Digest) -> Self {
        Self { kind: self.kind, non_successful_work: self.non_successful_work, digest }
    }
    pub(crate) const fn from_wire(
        kind: SchedulerTerminalKind,
        non_successful_work: Vec<WorkId>,
        digest: Sha256Digest,
    ) -> Self {
        Self { kind, non_successful_work, digest }
    }
}
impl Clone for SchedulerTerminal {
    fn clone(&self) -> Self {
        Self {
            kind: self.kind,
            non_successful_work: self.non_successful_work.clone(),
            digest: self.digest,
        }
    }
}

impl SchedulerTerminal {
    pub(crate) fn evaluate(work: &[WorkRecord]) -> Self {
        let mut terminal = Self::evaluate_with_digest(work, Sha256Digest::new([0; 32]));
        terminal.digest = crate::canonical::terminal_digest(&terminal);
        terminal
    }
}

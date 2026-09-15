//! Stable accessors for checked compaction evidence.

use super::{CompactionPolicyId, ContextNode, SourceRange, ValidatedCompaction};
use vstd::prelude::*;

verus! {

impl ValidatedCompaction {
    /// Returns the new derived node with complete source dependencies.
    #[must_use]
    pub const fn node(&self) -> (result: &ContextNode)
        ensures *result == self.spec_node(),
    {
        &self.node
    }

    /// Returns the validated policy revision.
    #[must_use]
    pub const fn policy_id(&self) -> (result: CompactionPolicyId)
        ensures result == self.spec_policy_id(),
    {
        self.policy_id
    }

    /// Returns the exact canonical source ranges.
    #[must_use]
    pub const fn source_ranges(&self) -> (result: &[SourceRange])
        ensures result@ == self.spec_source_ranges(),
    {
        self.source_ranges.as_slice()
    }

    /// Returns the complete selected-source token estimate replaced by the output.
    #[must_use]
    pub const fn replaced_tokens(&self) -> (result: u64)
        ensures result == self.spec_replaced_tokens(),
    {
        self.replaced_tokens
    }

    /// Consumes validation evidence and returns the checked derived node.
    #[must_use]
    pub fn into_node(self) -> ContextNode { self.node }
}

} // verus!

//! Stable accessors for checked compaction evidence.

use super::{CompactionPolicyId, ContextNode, SourceRange, ValidatedCompaction};

impl ValidatedCompaction {
    #[doc = " Returns the new derived node with complete source dependencies."]
    #[must_use]
    pub const fn node(&self) -> &ContextNode {
        &self.node
    }
    #[doc = " Returns the validated policy revision."]
    #[must_use]
    pub const fn policy_id(&self) -> CompactionPolicyId {
        self.policy_id
    }
    #[doc = " Returns the exact canonical source ranges."]
    #[must_use]
    pub const fn source_ranges(&self) -> &[SourceRange] {
        self.source_ranges.as_slice()
    }
    #[doc = " Returns the complete selected-source token estimate replaced by the output."]
    #[must_use]
    pub const fn replaced_tokens(&self) -> u64 {
        self.replaced_tokens
    }
    #[doc = " Consumes validation evidence and returns the checked derived node."]
    #[must_use]
    pub fn into_node(self) -> ContextNode {
        self.node
    }
}

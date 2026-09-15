//! Exact logical views for validated compaction sources.

use super::{CompactionProposal, ContextNodeId, SourceRange};
use vstd::prelude::*;

verus! {

impl CompactionProposal {
    /// Canonical source identities obtained by collapsing adjacent ranges for the same source.
    pub open spec fn source_ids(ranges: Seq<SourceRange>) -> Seq<ContextNodeId>
        decreases ranges.len(),
    {
        if ranges.len() == 0 {
            Seq::empty()
        } else {
            let prior = Self::source_ids(ranges.drop_last());
            let id = ranges.last().spec_source_id();
            if prior.len() == 0 || !prior.last().spec_matches(&id) { prior.push(id) } else { prior }
        }
    }

    /// Logical view of the exact canonical source identity set.
    pub open spec fn spec_source_ids(&self) -> Seq<ContextNodeId> {
        Self::source_ids(self.spec_source_ranges())
    }
}

} // verus!

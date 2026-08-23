//! Verified duplication of unprivileged transition records.

use super::LeaseTransitionRecord;
use vstd::prelude::*;

verus! {

impl LeaseTransitionRecord {
    /// Returns whether every semantic record and source-command field exactly duplicates another.
    pub closed spec fn spec_exactly_duplicates(&self, source: &Self) -> bool {
        self.command_id == source.command_id
            && self.scope == source.scope
            && self.before_version == source.before_version
            && self.after_version == source.after_version
            && self.before_generation == source.before_generation
            && self.after_generation == source.after_generation
            && self.before_phase == source.before_phase
            && self.after_phase == source.after_phase
            && self.kind == source.kind
            && self.binding.exactly_duplicates(&source.binding)
    }

    /// Explicitly duplicates this unprivileged record with proof-visible exact semantics.
    #[must_use]
    pub fn duplicate(&self) -> (duplicate: Self)
        ensures duplicate.spec_exactly_duplicates(self),
    {
        let duplicate = Self {
            command_id: self.command_id,
            scope: self.scope,
            before_version: self.before_version,
            after_version: self.after_version,
            before_generation: self.before_generation,
            after_generation: self.after_generation,
            before_phase: self.before_phase,
            after_phase: self.after_phase,
            kind: self.kind,
            binding: Box::new(self.binding.duplicate()),
        };
        proof {
            assert(duplicate.spec_exactly_duplicates(self));
        }
        duplicate
    }
}

} // verus!

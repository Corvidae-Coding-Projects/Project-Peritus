//! Verified duplication of unprivileged transition records.

use super::LeaseTransitionRecord;

impl LeaseTransitionRecord {
    #[doc = " Explicitly duplicates this unprivileged record with proof-visible exact semantics."]
    #[must_use]
    pub fn duplicate(&self) -> Self {
        Self {
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
        }
    }
}

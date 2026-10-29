//! Host-only dependency reduction; model updates cannot invoke these event variants.

use super::state_revision::next_revision;
use super::validation::{find_entry, invalidate_entries};
use super::{WorkingBinding, WorkingError, WorkingState, WorkingValidity};
use crate::ContextNodeId;
use peritus_types::Sha256Digest;

/// Conservative source closure of an originating host view. It grants no evidence authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingHostBinding {
    through: u64,
    request: Option<Sha256Digest>,
}
impl WorkingHostBinding {
    /// Records a source-prefix bound and optional exact originating request fingerprint.
    #[must_use]
    pub const fn new(through: u64, request: Option<Sha256Digest>) -> Self {
        Self { through, request }
    }
    /// Every possibly supplied observation through this inclusive sequence is a dependency.
    #[must_use]
    pub const fn through_observation(self) -> u64 {
        self.through
    }
    /// Exact originating request identity. Missing identity requires unknown validity.
    #[must_use]
    pub const fn request(self) -> Option<Sha256Digest> {
        self.request
    }
}

pub(super) fn invalidate(
    state: &WorkingState,
    binding: WorkingBinding,
    base_revision: u64,
    ids: &[ContextNodeId],
) -> Result<WorkingState, WorkingError> {
    state.check_binding(binding)?;
    if base_revision != state.revision() {
        return Err(WorkingError::RevisionMismatch);
    }
    if ids.is_empty() || ids.len() > state.limits().operations() {
        return Err(WorkingError::Capacity);
    }
    let mut entries = state.clone_entries();
    let mut previous = None;
    for id in ids {
        if previous.is_some_and(|previous| previous >= *id) {
            return Err(WorkingError::NonCanonicalOrder);
        }
        let index = find_entry(&entries, *id).ok_or(WorkingError::MissingEntry)?;
        entries[index].host_declared_status = entries[index].declared_status();
        entries[index].validity = WorkingValidity::uncertain();
        entries[index].host_binding =
            Some(WorkingHostBinding::new(state.through_observation(), None));
        previous = Some(*id);
    }
    let entries = invalidate_entries(&entries, state.environment(), state.through_observation());
    Ok(state.with_entries(entries, next_revision(state.revision())?))
}

//! Immutable observation prefix and bounded investigation model.

use super::validation::{find_entry, invalidate_entries};
use super::{ObservationId, ObservationSource, WorkingBinding, WorkingEntry, WorkingEnvironment, WorkingError, WorkingLimits};
use crate::ContextNodeId;
use vstd::prelude::*;

verus! {
/// One role-scoped working lineage. Rejected reducers leave the caller's state untouched.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingState {
    pub(super) environment: WorkingEnvironment,
    pub(super) revision: u64,
    pub(super) observations: Vec<ObservationSource>,
    pub(super) entries: Vec<WorkingEntry>,
    pub(super) limits: WorkingLimits,
    pub(super) protocol: super::WorkingProtocol,
}

impl WorkingState {
    /// Opens an empty working model under an explicit host-observed environment.
    ///
    /// # Errors
    /// Rejects an environment built under wider limits than the state allows.
    pub fn new(environment: WorkingEnvironment, limits: WorkingLimits) -> Result<Self, WorkingError> {
        if environment.files().len() > limits.entries() { return Err(WorkingError::Capacity); }
        Ok(Self { environment, revision: 0, observations: Vec::new(), entries: Vec::new(), limits, protocol: super::WorkingProtocol::empty() })
    }
    /// Exact current binding, including the incorporated conversation revision.
    #[must_use]
    pub const fn binding(&self) -> WorkingBinding { self.environment.binding() }
    /// Monotonic state revision used for optimistic updates.
    #[must_use]
    pub const fn revision(&self) -> u64 { self.revision }
    /// Contiguous archived observation prefix incorporated by this state.
    #[must_use]
    pub const fn through_observation(&self) -> u64 { self.observations.len() as u64 }
    /// State allocation policy.
    #[must_use]
    pub const fn limits(&self) -> WorkingLimits { self.limits }
    /// Current workspace revision bindings.
    #[must_use]
    pub const fn environment(&self) -> &WorkingEnvironment { &self.environment }

    /// Reads host-owned literal requirement references and unresolved operation state.
    ///
    /// # Errors
    /// Rejects another lineage or stale conversation before exposing any source handles.
    pub fn protocol(&self, binding: WorkingBinding) -> Result<&super::WorkingProtocol, WorkingError> {
        self.check_binding(binding)?;
        Ok(&self.protocol)
    }

    /// Reads the retained model only through an exact task/role/conversation binding.
    ///
    /// # Errors
    /// Rejects cross-run, cross-workspace, cross-task, cross-role, and stale-conversation reads.
    pub fn entries(&self, binding: WorkingBinding) -> Result<&[WorkingEntry], WorkingError> {
        self.check_binding(binding)?;
        Ok(self.entries.as_slice())
    }

    /// Resolves an exact observation handle without returning or materializing artifact bytes.
    ///
    /// # Errors
    /// Rejects mismatched scope or missing handles before the host performs artifact I/O.
    pub fn observation(&self, binding: WorkingBinding, id: ObservationId) -> Result<ObservationSource, WorkingError> {
        self.check_binding(binding)?;
        let mut index = 0;
        while index < self.observations.len()
            invariant index <= self.observations.len(),
            decreases self.observations.len() - index,
        {
            if self.observations[index].id() == id { return Ok(self.observations[index]); }
            index += 1;
        }
        Err(WorkingError::MissingSource)
    }

    /// Resolves an entry without dropping its stale/superseded status or counterevidence.
    ///
    /// # Errors
    /// Rejects another scope or an unknown entry.
    #[allow(clippy::option_if_let_else, reason = "Verus does not support Option::map_or")]
    pub fn entry(&self, binding: WorkingBinding, id: ContextNodeId) -> Result<&WorkingEntry, WorkingError> {
        self.check_binding(binding)?;
        match find_entry(&self.entries, id) {
            Some(index) => Ok(&self.entries[index]),
            None => Err(WorkingError::MissingEntry),
        }
    }

    pub(super) fn check_binding(&self, binding: WorkingBinding) -> Result<(), WorkingError> {
        if self.binding() == binding { Ok(()) } else { Err(WorkingError::BindingMismatch) }
    }
}

/// Idempotently incorporates one verified observation from a contiguous host archive.
///
/// # Errors
/// Rejects scope mismatch, skipped sequence, conflicting reuse, capacity, or revision overflow.
/// An exact already-ingested observation returns the unchanged revision.
pub fn ingest_working_observation(
    state: &WorkingState,
    binding: WorkingBinding,
    source: ObservationSource,
) -> Result<WorkingState, WorkingError> {
    state.check_binding(binding)?;
    if source.id().get() <= state.through_observation() {
        let previous = state.observation(binding, source.id())?;
        if previous == source { return Ok(state.clone()); }
        return Err(WorkingError::SourceConflict);
    }
    if state.observations.len() >= state.limits.observations() { return Err(WorkingError::Capacity); }
    let expected = next_revision(state.through_observation())?;
    if source.id().get() != expected { return Err(WorkingError::SourceSequence); }
    let revision = next_revision(state.revision)?;
    let mut next = state.clone();
    next.observations.push(source);
    next.revision = revision;
    Ok(next)
}

/// Rebinds a continuing task to current files/conversation and transitively invalidates entries.
///
/// Staleness is sticky: changing a file back does not silently restore an earlier conclusion.
/// A fresh source-backed delta is required. Unrelated edits preserve entries with explicit
/// unaffected file dependencies, even when the complete candidate digest changes.
///
/// # Errors
/// Rejects another lineage, stale base revision, backwards conversation revision, or capacity.
pub fn refresh_working_state(
    state: &WorkingState,
    expected_revision: u64,
    environment: WorkingEnvironment,
) -> Result<WorkingState, WorkingError> {
    if expected_revision != state.revision { return Err(WorkingError::RevisionMismatch); }
    if !state.binding().same_lineage(environment.binding()) { return Err(WorkingError::BindingMismatch); }
    if environment.binding().conversation_revision() < state.binding().conversation_revision() {
        return Err(WorkingError::StaleConversation);
    }
    if environment.files().len() > state.limits.entries() { return Err(WorkingError::Capacity); }
    if environment == state.environment { return Ok(state.clone()); }
    let revision = next_revision(state.revision)?;
    let mut next = state.clone();
    next.entries = invalidate_entries(&next.entries, &environment, state.through_observation());
    next.environment = environment;
    next.revision = revision;
    Ok(next)
}

pub(super) const fn next_revision(current: u64) -> (next: Result<u64, WorkingError>)
    ensures match next {
        Ok(value) => value as int == current as int + 1 && value > current,
        Err(_) => current == u64::MAX,
    },
{
    if current == u64::MAX { Err(WorkingError::RevisionExhausted) } else { Ok(current + 1) }
}
}

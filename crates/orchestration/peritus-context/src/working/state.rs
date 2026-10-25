//! Immutable observation prefix and bounded investigation model.

use super::state_revision::next_revision;
use super::validation::invalidate_entries;
use super::{
    ObservationSource, WorkingBinding, WorkingEntry, WorkingEnvironment, WorkingError,
    WorkingLimits,
};

#[doc = " One role-scoped working lineage. Rejected reducers leave the caller\'s state untouched."]
#[derive(Debug, Eq, PartialEq)]
pub struct WorkingState {
    pub(super) environment: WorkingEnvironment,
    pub(super) revision: u64,
    pub(super) observations: super::observation_prefix::ObservationPrefix,
    pub(super) entries: Vec<WorkingEntry>,
    pub(super) limits: WorkingLimits,
    pub(super) protocol: super::WorkingProtocol,
}
impl Clone for WorkingState {
    fn clone(&self) -> Self {
        Self {
            environment: self.environment.clone(),
            revision: self.revision,
            observations: self.observations.clone(),
            entries: WorkingEntry::clone_sequence(&self.entries),
            limits: self.limits,
            protocol: self.protocol.clone(),
        }
    }
}
impl WorkingState {
    #[doc = " Opens an empty working model under an explicit host-observed environment."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects an environment built under wider limits than the state allows."]
    pub fn new(
        environment: WorkingEnvironment,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        if environment.files().len() > limits.entries() {
            return Err(WorkingError::Capacity);
        }
        let state = Self {
            environment,
            revision: 0,
            observations: super::observation_prefix::ObservationPrefix::default(),
            entries: Vec::new(),
            limits,
            protocol: super::WorkingProtocol::empty(),
        };
        Ok(state)
    }
    #[doc = " Exact current binding, including the incorporated conversation revision."]
    #[must_use]
    pub const fn binding(&self) -> WorkingBinding {
        self.environment.binding()
    }
    #[doc = " Monotonic state revision used for optimistic updates."]
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    #[doc = " Contiguous archived observation prefix incorporated by this state."]
    #[must_use]
    pub const fn through_observation(&self) -> u64 {
        self.observations.len() as u64
    }
    #[doc = " State allocation policy."]
    #[must_use]
    pub const fn limits(&self) -> WorkingLimits {
        self.limits
    }
    #[doc = " Current workspace revision bindings."]
    #[must_use]
    pub const fn environment(&self) -> &WorkingEnvironment {
        &self.environment
    }
    pub(super) fn with_entries(&self, entries: Vec<WorkingEntry>, revision: u64) -> Self {
        Self {
            environment: self.environment.clone(),
            revision,
            observations: self.observations.clone(),
            entries,
            limits: self.limits,
            protocol: self.protocol.clone(),
        }
    }
    pub(super) fn clone_entries(&self) -> Vec<WorkingEntry> {
        WorkingEntry::clone_sequence(&self.entries)
    }
    pub(super) fn with_protocol(&self, protocol: super::WorkingProtocol, revision: u64) -> Self {
        Self {
            environment: self.environment.clone(),
            revision,
            observations: self.observations.clone(),
            entries: self.entries.clone(),
            limits: self.limits,
            protocol,
        }
    }
    /// Widens host allocation policy without changing content, bindings, or semantic revision.
    /// Old saved policy is not a lifetime limit when a host adopts a wider current policy.
    ///
    /// # Errors
    /// Rejects narrowing any existing allocation policy.
    pub fn with_limits(&self, limits: WorkingLimits) -> Result<Self, WorkingError> {
        if limits.observations() < self.limits.observations()
            || limits.entries() < self.limits.entries()
            || limits.entry_bytes() < self.limits.entry_bytes()
            || limits.links() < self.limits.links()
            || limits.operations() < self.limits.operations()
        {
            return Err(WorkingError::InvalidLimit);
        }
        let mut next = self.clone();
        next.limits = limits;
        Ok(next)
    }

    pub(super) fn check_binding(&self, binding: WorkingBinding) -> Result<(), WorkingError> {
        if self.binding() == binding { Ok(()) } else { Err(WorkingError::BindingMismatch) }
    }
}
#[doc = " Idempotently incorporates one verified observation from a contiguous host archive."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = " Rejects scope mismatch, skipped sequence, conflicting reuse, capacity, or revision overflow."]
#[doc = " An exact already-ingested observation returns the unchanged revision."]
pub fn ingest_working_observation(
    state: &WorkingState,
    binding: WorkingBinding,
    source: ObservationSource,
) -> Result<WorkingState, WorkingError> {
    state.check_binding(binding)?;
    if source.id().get() <= state.through_observation() {
        let previous = state.observation(binding, source.id())?;
        if previous == source {
            let next = state.clone();
            return Ok(next);
        }
        return Err(WorkingError::SourceConflict);
    }
    if state.observations.len() >= state.limits.observations() {
        return Err(WorkingError::Capacity);
    }
    let expected = next_revision(state.through_observation())?;
    if source.id().get() != expected {
        return Err(WorkingError::SourceSequence);
    }
    let revision = next_revision(state.revision)?;
    let mut next = state.clone();
    next.observations.push(source)?;
    next.revision = revision;
    Ok(next)
}
#[doc = " Rebinds a continuing task to current files/conversation and transitively invalidates entries."]
#[doc = ""]
#[doc = " Staleness is sticky: changing a file back does not silently restore an earlier conclusion."]
#[doc = " A fresh source-backed delta is required. Unrelated edits preserve entries with explicit"]
#[doc = " unaffected file dependencies, even when the complete candidate digest changes."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = " Rejects another lineage, stale base revision, backwards conversation revision, or capacity."]
pub fn refresh_working_state(
    state: &WorkingState,
    expected_revision: u64,
    environment: WorkingEnvironment,
) -> Result<WorkingState, WorkingError> {
    if expected_revision != state.revision {
        return Err(WorkingError::RevisionMismatch);
    }
    if !state.binding().same_lineage(environment.binding()) {
        return Err(WorkingError::BindingMismatch);
    }
    if environment.binding().conversation_revision() < state.binding().conversation_revision() {
        return Err(WorkingError::StaleConversation);
    }
    if environment.files().len() > state.limits.entries() {
        return Err(WorkingError::Capacity);
    }
    if environment == state.environment {
        let mut next = state.clone();
        next.environment = environment;
        return Ok(next);
    }
    let revision = next_revision(state.revision)?;
    let mut next = state.clone();
    next.entries = invalidate_entries(&state.entries, &environment, state.through_observation());
    next.environment = environment;
    next.revision = revision;
    Ok(next)
}

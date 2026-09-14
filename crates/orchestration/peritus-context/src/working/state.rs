//! Immutable observation prefix and bounded investigation model.

use super::validation::{find_entry, invalidate_entries};
use super::{ObservationId, ObservationSource, WorkingBinding, WorkingEntry, WorkingEnvironment, WorkingError, WorkingLimits};
use crate::ContextNodeId;
use vstd::prelude::*;

verus! {
/// One role-scoped working lineage. Rejected reducers leave the caller's state untouched.
#[derive(Debug, Eq, PartialEq)]
pub struct WorkingState {
    pub(super) environment: WorkingEnvironment,
    pub(super) revision: u64,
    pub(super) observations: Vec<ObservationSource>,
    pub(super) entries: Vec<WorkingEntry>,
    pub(super) limits: WorkingLimits,
    pub(super) protocol: super::WorkingProtocol,
}
impl Clone for WorkingState {
    fn clone(&self) -> (result: Self)
        ensures
            result.spec_environment().spec_binding()
                == self.spec_environment().spec_binding(),
            result.spec_environment().spec_candidate()
                == self.spec_environment().spec_candidate(),
            result.spec_environment().spec_files()
                == self.spec_environment().spec_files(),
            result.spec_revision() == self.spec_revision(),
            result.spec_observations() == self.spec_observations(),
            WorkingEntry::sequence_clone_equivalent(
                self.spec_entries(),
                result.spec_entries(),
            ),
            result.spec_limits() == self.spec_limits(),
            result.spec_protocol().spec_requirements()
                == self.spec_protocol().spec_requirements(),
            result.spec_protocol().spec_pending() == self.spec_protocol().spec_pending(),
    {
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
    /// Logical host-observed environment.
    pub closed spec fn spec_environment(&self) -> WorkingEnvironment { self.environment }
    /// Logical reducer revision.
    pub closed spec fn spec_revision(&self) -> u64 { self.revision }
    /// Logical contiguous observation prefix.
    pub closed spec fn spec_observations(&self) -> Seq<ObservationSource> { self.observations@ }
    /// Logical retained working entries.
    pub closed spec fn spec_entries(&self) -> Seq<WorkingEntry> { self.entries@ }
    /// Logical allocation policy.
    pub closed spec fn spec_limits(&self) -> WorkingLimits { self.limits }
    /// Logical host-owned semantic pins.
    pub closed spec fn spec_protocol(&self) -> super::WorkingProtocol { self.protocol }
    /// Complete semantic equality used to frame reducers and replay.
    pub open spec fn spec_same(&self, other: &Self) -> bool {
        &&& self.spec_environment().spec_binding()
            == other.spec_environment().spec_binding()
        &&& self.spec_environment().spec_candidate()
            == other.spec_environment().spec_candidate()
        &&& self.spec_environment().spec_files()
            == other.spec_environment().spec_files()
        &&& self.spec_revision() == other.spec_revision()
        &&& self.spec_observations() == other.spec_observations()
        &&& WorkingEntry::sequence_clone_equivalent(
            self.spec_entries(),
            other.spec_entries(),
        )
        &&& self.spec_limits() == other.spec_limits()
        &&& self.spec_protocol().spec_requirements()
            == other.spec_protocol().spec_requirements()
        &&& self.spec_protocol().spec_pending()
            == other.spec_protocol().spec_pending()
    }

    /// Opens an empty working model under an explicit host-observed environment.
    ///
    /// # Errors
    /// Rejects an environment built under wider limits than the state allows.
    pub fn new(environment: WorkingEnvironment, limits: WorkingLimits) -> (result: Result<Self, WorkingError>)
        ensures match result {
            Ok(state) => {
                &&& state.spec_environment() == environment
                &&& state.spec_revision() == 0
                &&& state.spec_observations().len() == 0
                &&& state.spec_entries().len() == 0
                &&& state.spec_limits() == limits
                &&& state.spec_protocol().spec_requirements().len() == 0
                &&& state.spec_protocol().spec_pending().len() == 0
            }
            Err(_) => true,
        },
    {
        if environment.files().len() > limits.entries() { return Err(WorkingError::Capacity); }
        let state = Self { environment, revision: 0, observations: Vec::new(), entries: Vec::new(), limits, protocol: super::WorkingProtocol::empty() };
        proof {
            reveal(WorkingState::spec_environment);
            reveal(WorkingState::spec_revision);
            reveal(WorkingState::spec_observations);
            reveal(WorkingState::spec_entries);
            reveal(WorkingState::spec_limits);
            reveal(WorkingState::spec_protocol);
        }
        Ok(state)
    }
    /// Exact current binding, including the incorporated conversation revision.
    #[must_use]
    pub const fn binding(&self) -> (result: WorkingBinding)
        ensures result == self.spec_environment().spec_binding(),
    { self.environment.binding() }
    /// Monotonic state revision used for optimistic updates.
    #[must_use]
    pub const fn revision(&self) -> (result: u64)
        ensures result == self.spec_revision(),
    { self.revision }
    /// Contiguous archived observation prefix incorporated by this state.
    #[must_use]
    pub const fn through_observation(&self) -> (result: u64)
        ensures result as nat == self.spec_observations().len(),
    { self.observations.len() as u64 }
    /// State allocation policy.
    #[must_use]
    pub const fn limits(&self) -> (result: WorkingLimits)
        ensures result == self.spec_limits(),
    { self.limits }
    /// Current workspace revision bindings.
    #[must_use]
    pub const fn environment(&self) -> (result: &WorkingEnvironment)
        ensures *result == self.spec_environment(),
    { &self.environment }
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

    pub(super) fn with_entries(
        &self,
        entries: Vec<WorkingEntry>,
        revision: u64,
    ) -> (result: Self)
        ensures
            result.spec_environment().spec_binding()
                == self.spec_environment().spec_binding(),
            result.spec_environment().spec_candidate()
                == self.spec_environment().spec_candidate(),
            result.spec_environment().spec_files()
                == self.spec_environment().spec_files(),
            result.spec_revision() == revision,
            result.spec_observations() == self.spec_observations(),
            result.spec_entries() == entries@,
            result.spec_limits() == self.spec_limits(),
            result.spec_protocol().spec_requirements()
                == self.spec_protocol().spec_requirements(),
            result.spec_protocol().spec_pending() == self.spec_protocol().spec_pending(),
    {
        let result = Self {
            environment: self.environment.clone(),
            revision,
            observations: self.observations.clone(),
            entries,
            limits: self.limits,
            protocol: self.protocol.clone(),
        };
        proof {
            reveal(WorkingState::spec_revision);
            reveal(WorkingState::spec_observations);
            reveal(WorkingState::spec_entries);
        }
        result
    }

    pub(super) fn with_protocol(
        &self,
        protocol: super::WorkingProtocol,
        revision: u64,
    ) -> (result: Self)
        ensures
            result.spec_environment().spec_binding()
                == self.spec_environment().spec_binding(),
            result.spec_environment().spec_candidate()
                == self.spec_environment().spec_candidate(),
            result.spec_environment().spec_files()
                == self.spec_environment().spec_files(),
            result.spec_revision() == revision,
            result.spec_observations() == self.spec_observations(),
            WorkingEntry::sequence_clone_equivalent(
                self.spec_entries(),
                result.spec_entries(),
            ),
            result.spec_limits() == self.spec_limits(),
            result.spec_protocol() == protocol,
    {
        let result = Self {
            environment: self.environment.clone(),
            revision,
            observations: self.observations.clone(),
            entries: self.entries.clone(),
            limits: self.limits,
            protocol,
        };
        proof {
            reveal(WorkingState::spec_revision);
            reveal(WorkingState::spec_observations);
            reveal(WorkingState::spec_protocol);
        }
        result
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
) -> (result: Result<WorkingState, WorkingError>)
    ensures match result {
        Ok(next) => {
            &&& next.spec_environment().spec_binding()
                == state.spec_environment().spec_binding()
            &&& next.spec_environment().spec_candidate()
                == state.spec_environment().spec_candidate()
            &&& next.spec_environment().spec_files()
                == state.spec_environment().spec_files()
            &&& next.spec_revision() >= state.spec_revision()
            &&& next.spec_revision() as int <= state.spec_revision() as int + 1
            &&& next.spec_observations().len() >= state.spec_observations().len()
            &&& next.spec_observations().len() <= state.spec_observations().len() + 1
            &&& (source.spec_id().spec_value() <= state.spec_observations().len()
                ==> next.spec_revision() == state.spec_revision()
                    && next.spec_observations().len() == state.spec_observations().len())
            &&& (source.spec_id().spec_value() > state.spec_observations().len()
                ==> next.spec_revision() as int == state.spec_revision() as int + 1
                    && next.spec_observations() == state.spec_observations().push(source))
            &&& WorkingEntry::sequence_clone_equivalent(
                state.spec_entries(), next.spec_entries(),
            )
            &&& next.spec_limits() == state.spec_limits()
            &&& next.spec_protocol().spec_requirements()
                == state.spec_protocol().spec_requirements()
            &&& next.spec_protocol().spec_pending()
                == state.spec_protocol().spec_pending()
        }
        Err(_) => true,
    },
{
    state.check_binding(binding)?;
    if source.id().get() <= state.through_observation() {
        let previous = state.observation(binding, source.id())?;
        if previous == source {
            let next = state.clone();
            proof {
                assert(source.spec_id().spec_value() <= state.spec_observations().len());
                assert(next.spec_revision() == state.spec_revision());
                assert(next.spec_observations() == state.spec_observations());
            }
            return Ok(next);
        }
        return Err(WorkingError::SourceConflict);
    }
    if state.observations.len() >= state.limits.observations() { return Err(WorkingError::Capacity); }
    let expected = next_revision(state.through_observation())?;
    if source.id().get() != expected { return Err(WorkingError::SourceSequence); }
    let revision = next_revision(state.revision)?;
    let mut next = state.clone();
    next.observations.push(source);
    next.revision = revision;
    proof {
        assert(source.spec_id().spec_value() > state.spec_observations().len());
        assert(next.spec_revision() as int == state.spec_revision() as int + 1);
        assert(next.spec_observations() == state.spec_observations().push(source));
    }
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
) -> (result: Result<WorkingState, WorkingError>)
    ensures match result {
        Ok(next) => {
            &&& next.spec_environment().spec_binding()
                == environment.spec_binding()
            &&& next.spec_environment().spec_candidate()
                == environment.spec_candidate()
            &&& next.spec_environment().spec_files() == environment.spec_files()
            &&& next.spec_revision() >= state.spec_revision()
            &&& next.spec_revision() as int <= state.spec_revision() as int + 1
            &&& next.spec_observations() == state.spec_observations()
            &&& WorkingEntry::sequence_payload_equivalent(
                state.spec_entries(), next.spec_entries(),
            )
            &&& next.spec_limits() == state.spec_limits()
            &&& next.spec_protocol().spec_requirements()
                == state.spec_protocol().spec_requirements()
            &&& next.spec_protocol().spec_pending()
                == state.spec_protocol().spec_pending()
        }
        Err(_) => true,
    },
{
    if expected_revision != state.revision { return Err(WorkingError::RevisionMismatch); }
    if !state.binding().same_lineage(environment.binding()) { return Err(WorkingError::BindingMismatch); }
    if environment.binding().conversation_revision() < state.binding().conversation_revision() {
        return Err(WorkingError::StaleConversation);
    }
    if environment.files().len() > state.limits.entries() { return Err(WorkingError::Capacity); }
    if environment == state.environment {
        let mut next = state.clone();
        next.environment = environment;
        proof {
            WorkingEntry::sequence_clone_implies_payload(
                state.spec_entries(),
                next.spec_entries(),
            );
        }
        return Ok(next);
    }
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

//! Revision-checked, all-or-nothing working-model updates.

use super::state_revision::next_revision;
use super::validation::{
    find_entry, invalidate_entries, unusable, validate_graph, validate_references,
};

use super::delta_reducer::{apply_supersession, upsert_entry, validate_upsert};
use super::{
    WorkingBinding, WorkingEntry, WorkingEntryStatus, WorkingError, WorkingLimits, WorkingState,
};

#[doc = " An ordered bounded set of source-backed upserts against one exact state revision."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingDelta {
    binding: WorkingBinding,
    base_revision: u64,
    entries: Vec<WorkingEntry>,
}
impl WorkingDelta {
    #[doc = " Creates an atomic proposal. Rejections do not partially update the model."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects empty, excessive, unordered, duplicate, or host-status-bearing proposals."]
    pub fn new(
        binding: WorkingBinding,
        base_revision: u64,
        entries: Vec<WorkingEntry>,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        if entries.is_empty() {
            return Err(WorkingError::EmptyEntry);
        }
        if entries.len() > limits.operations() {
            return Err(WorkingError::Capacity);
        }
        let mut index = 0;
        while index < entries.len() {
            if unusable(entries[index].status()) || entries[index].host_binding().is_some() {
                return Err(WorkingError::DerivedStatus);
            }
            if index > 0 && entries[index - 1].id() >= entries[index].id() {
                return Err(WorkingError::NonCanonicalOrder);
            }
            index += 1;
        }
        Ok(Self { binding, base_revision, entries })
    }
    #[doc = " Target task/role/conversation binding."]
    #[must_use]
    pub const fn binding(&self) -> WorkingBinding {
        self.binding
    }
    #[doc = " Exact prior state revision."]
    #[must_use]
    pub const fn base_revision(&self) -> u64 {
        self.base_revision
    }
    #[doc = " Source-backed proposals in canonical entry order."]
    #[must_use]
    pub const fn entries(&self) -> &[WorkingEntry] {
        self.entries.as_slice()
    }
}
#[doc = " Applies a complete source-checked proposal and validates the resulting dependency graph."]
#[doc = ""]
#[doc = " Superseded entries and their source references remain retained. Stale entries require fresh"]
#[doc = " supporting or contradicting observations before they can be replaced with current claims."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = " Rejects wrong scope/revision, missing sources or dependencies, cycles, stale proposals,"]
#[doc = " resurrection of superseded records, allocation overflow, or exhausted revisions."]
pub fn apply_working_delta(
    state: &WorkingState,
    delta: &WorkingDelta,
) -> Result<WorkingState, WorkingError> {
    reduce_delta(state, delta, None)
}

#[allow(clippy::too_many_lines, reason = "proof steps mirror the atomic reducer phases")]
pub(super) fn reduce_delta(
    state: &WorkingState,
    delta: &WorkingDelta,
    host: Option<(&super::WorkingValidity, super::WorkingHostBinding)>,
) -> Result<WorkingState, WorkingError> {
    state.check_binding(delta.binding)?;
    if delta.base_revision != state.revision() {
        return Err(WorkingError::RevisionMismatch);
    }
    if delta.entries.len() > state.limits.operations() {
        return Err(WorkingError::Capacity);
    }
    if host.is_some_and(|(_, binding)| binding.through_observation() > state.through_observation())
    {
        return Err(WorkingError::MissingSource);
    }
    let revision = next_revision(state.revision())?;
    let mut entries = state.clone_entries();
    let mut index = 0;
    while index < delta.entries.len() {
        let entry = &delta.entries[index];
        if host.is_none()
            && state
                .entry(state.binding(), entry.id())
                .is_ok_and(|previous| previous.host_binding().is_some())
        {
            return Err(WorkingError::DerivedStatus);
        }
        validate_references(state, entry)?;
        validate_upsert(state, entry)?;
        let _checked = WorkingEntry::new(
            entry.id,
            entry.kind,
            entry.content.clone(),
            entry.links.clone(),
            entry.validity.clone(),
            state.limits,
        )?;
        let mut retained = entry.clone();
        if let Some((host, binding)) = host {
            let unknown = super::WorkingValidity::uncertain();
            let host = if binding.request().is_some() { host } else { &unknown };
            retained.validity = retained.validity.with_host(host, state.limits)?;
            retained.host_declared_status = Some(entry.status());
            retained.host_binding = Some(binding);
        }
        upsert_entry(&mut entries, &retained, state.limits.entries())?;
        index += 1;
    }
    validate_graph(&entries)?;
    index = 0;
    while index < delta.entries.len() {
        let entry = &delta.entries[index];
        apply_supersession(&mut entries, &state.entries, entry)?;
        index += 1;
    }
    let through = state.through_observation();
    entries = invalidate_entries(&entries, &state.environment, through);
    index = 0;
    while index < delta.entries.len() {
        let Some(target) = find_entry(&entries, delta.entries[index].id) else {
            return Err(WorkingError::MissingEntry);
        };
        if host.is_none() && entries[target].status == WorkingEntryStatus::Stale {
            return Err(WorkingError::StaleEntry);
        }
        index += 1;
    }
    let next = state.with_entries(entries, revision);
    Ok(next)
}

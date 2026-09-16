//! Revision-checked, all-or-nothing working-model updates.

use super::state_revision::next_revision;
use super::validation::{find_entry, invalidate_entries, unusable, validate_graph, validate_references};
#[cfg(verus_only)]
use super::delta_model::{
    spec_one_supersession, spec_one_upsert, spec_supersession_prefix, spec_upsert_prefix,
};
use super::delta_reducer::{apply_supersession, upsert_entry, validate_upsert};
use super::{WorkingBinding, WorkingEntry, WorkingEntryStatus, WorkingError, WorkingLimits, WorkingState};
use vstd::prelude::*;

verus! {
/// An ordered bounded set of source-backed upserts against one exact state revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingDelta {
    binding: WorkingBinding,
    base_revision: u64,
    entries: Vec<WorkingEntry>,
}

impl WorkingDelta {
    /// Logical target binding.
    pub closed spec fn spec_binding(&self) -> WorkingBinding { self.binding }
    /// Logical base revision.
    pub closed spec fn spec_base_revision(&self) -> u64 { self.base_revision }
    /// Logical canonical proposal sequence.
    pub closed spec fn spec_entries(&self) -> Seq<WorkingEntry> { self.entries@ }

    /// Exact successful entry sequence after ordered upsert, supersession, and invalidation.
    pub closed spec fn spec_result_entries(
        &self,
        state: &WorkingState,
        entries: Seq<WorkingEntry>,
    ) -> bool {
        exists |upserted: Seq<WorkingEntry>, superseded: Seq<WorkingEntry>|
            spec_upsert_prefix(
                state.spec_entries(),
                self.spec_entries(),
                self.spec_entries().len() as int,
                upserted,
            )
            && spec_supersession_prefix(
                upserted,
                self.spec_entries(),
                self.spec_entries().len() as int,
                superseded,
            )
            && WorkingEntry::sequence_payload_equivalent(superseded, entries)
            && WorkingState::spec_entry_statuses(entries)
                == WorkingState::spec_invalidated_entry_statuses(
                    superseded,
                    &state.spec_environment(),
                    state.spec_observations().len() as u64,
                )
    }

    /// Creates an atomic proposal. Rejections do not partially update the model.
    ///
    /// # Errors
    /// Rejects empty, excessive, unordered, duplicate, or host-status-bearing proposals.
    pub fn new(
        binding: WorkingBinding,
        base_revision: u64,
        entries: Vec<WorkingEntry>,
        limits: WorkingLimits,
    ) -> (result: Result<Self, WorkingError>)
        ensures match result {
            Ok(delta) => {
                &&& delta.spec_binding() == binding
                &&& delta.spec_base_revision() == base_revision
                &&& delta.spec_entries() == entries@
            }
            Err(_) => true,
        },
    {
        if entries.is_empty() { return Err(WorkingError::EmptyEntry); }
        if entries.len() > limits.operations() { return Err(WorkingError::Capacity); }
        let mut index = 0;
        while index < entries.len()
            invariant index <= entries.len(),
            decreases entries.len() - index,
        {
            if unusable(entries[index].status()) { return Err(WorkingError::DerivedStatus); }
            if index > 0 && entries[index - 1].id() >= entries[index].id() {
                return Err(WorkingError::NonCanonicalOrder);
            }
            index += 1;
        }
        Ok(Self { binding, base_revision, entries })
    }
    /// Target task/role/conversation binding.
    #[must_use]
    pub const fn binding(&self) -> (result: WorkingBinding)
        ensures result == self.spec_binding(),
    { self.binding }
    /// Exact prior state revision.
    #[must_use]
    pub const fn base_revision(&self) -> (result: u64)
        ensures result == self.spec_base_revision(),
    { self.base_revision }
    /// Source-backed proposals in canonical entry order.
    #[must_use]
    pub const fn entries(&self) -> (result: &[WorkingEntry])
        ensures result@ == self.spec_entries(),
    { self.entries.as_slice() }
}

/// Applies a complete source-checked proposal and validates the resulting dependency graph.
///
/// Superseded entries and their source references remain retained. Stale entries require fresh
/// supporting or contradicting observations before they can be replaced with current claims.
///
/// # Errors
/// Rejects wrong scope/revision, missing sources or dependencies, cycles, stale proposals,
/// resurrection of superseded records, allocation overflow, or exhausted revisions.
#[allow(clippy::too_many_lines, reason = "proof steps mirror the atomic reducer phases")]
pub fn apply_working_delta(
    state: &WorkingState,
    delta: &WorkingDelta,
) -> (result: Result<WorkingState, WorkingError>)
    ensures match result {
        Ok(next) => {
            &&& next.spec_environment().spec_binding()
                == state.spec_environment().spec_binding()
            &&& next.spec_environment().spec_candidate()
                == state.spec_environment().spec_candidate()
            &&& next.spec_environment().spec_files()
                == state.spec_environment().spec_files()
            &&& next.spec_revision() as int == state.spec_revision() as int + 1
            &&& next.spec_observations() == state.spec_observations()
            &&& delta.spec_result_entries(state, next.spec_entries())
            &&& next.spec_limits() == state.spec_limits()
            &&& next.spec_protocol().spec_requirements()
                == state.spec_protocol().spec_requirements()
            &&& next.spec_protocol().spec_pending()
                == state.spec_protocol().spec_pending()
        }
        Err(error) => error.spec_is_delta_error(),
    },
{
    state.check_binding(delta.binding)?;
    if delta.base_revision != state.revision() { return Err(WorkingError::RevisionMismatch); }
    if delta.entries.len() > state.limits.operations() { return Err(WorkingError::Capacity); }
    let revision = next_revision(state.revision())?;
    let mut entries = state.clone_entries();
    proof {
        assert(WorkingEntry::sequence_clone_equivalent(state.spec_entries(), entries@));
        reveal_with_fuel(spec_upsert_prefix, 1);
        assert(spec_upsert_prefix(state.spec_entries(), delta.spec_entries(), 0, entries@));
    }
    let mut index = 0;
    while index < delta.entries.len()
        invariant
            index <= delta.entries.len(),
            spec_upsert_prefix(
                state.spec_entries(),
                delta.spec_entries(),
                index as int,
                entries@,
            ),
        decreases delta.entries.len() - index,
    {
        let entry = &delta.entries[index];
        validate_references(state, entry)?;
        validate_upsert(state, entry)?;
        let _checked = WorkingEntry::new(entry.id, entry.kind, entry.content.clone(), entry.links.clone(), entry.validity.clone(), state.limits)?;
        let ghost before_upsert = entries@;
        upsert_entry(&mut entries, entry, state.limits.entries())?;
        proof {
            assert(spec_upsert_prefix(
                state.spec_entries(),
                delta.spec_entries(),
                index as int,
                before_upsert,
            ));
            assert(spec_one_upsert(
                before_upsert,
                &delta.spec_entries()[index as int],
                entries@,
            ));
            assert(exists |previous: Seq<WorkingEntry>|
                spec_upsert_prefix(
                    state.spec_entries(),
                    delta.spec_entries(),
                    index as int,
                    previous,
                ) && spec_one_upsert(
                    previous,
                    &delta.spec_entries()[index as int],
                    entries@,
                )) by {
                assert(before_upsert == before_upsert);
            }
            assert(index as int + 1 > 0);
            assert(index as int + 1 <= delta.spec_entries().len());
            assert(index as int + 1 - 1 == index as int);
            reveal_with_fuel(spec_upsert_prefix, 1);
            assert(spec_upsert_prefix(
                state.spec_entries(),
                delta.spec_entries(),
                index as int + 1,
                entries@,
            ));
        }
        index += 1;
    }
    validate_graph(&entries)?;
    let ghost upserted = entries@;
    proof {
        reveal_with_fuel(spec_supersession_prefix, 1);
        assert(spec_supersession_prefix(upserted, delta.spec_entries(), 0, entries@));
    }
    index = 0;
    while index < delta.entries.len()
        invariant
            index <= delta.entries.len(),
            spec_upsert_prefix(
                state.spec_entries(),
                delta.spec_entries(),
                delta.spec_entries().len() as int,
                upserted,
            ),
            spec_supersession_prefix(
                upserted,
                delta.spec_entries(),
                index as int,
                entries@,
            ),
        decreases delta.entries.len() - index,
    {
        let entry = &delta.entries[index];
        let ghost before_supersession = entries@;
        apply_supersession(&mut entries, &state.entries, entry)?;
        proof {
            assert(spec_supersession_prefix(
                upserted,
                delta.spec_entries(),
                index as int,
                before_supersession,
            ));
            assert(spec_one_supersession(
                before_supersession,
                &delta.spec_entries()[index as int],
                entries@,
            ));
            assert(exists |previous: Seq<WorkingEntry>|
                spec_supersession_prefix(
                    upserted,
                    delta.spec_entries(),
                    index as int,
                    previous,
                ) && spec_one_supersession(
                    previous,
                    &delta.spec_entries()[index as int],
                    entries@,
                )) by {
                assert(before_supersession == before_supersession);
            }
            assert(index as int + 1 > 0);
            assert(index as int + 1 <= delta.spec_entries().len());
            assert(index as int + 1 - 1 == index as int);
            reveal_with_fuel(spec_supersession_prefix, 1);
            assert(spec_supersession_prefix(
                upserted,
                delta.spec_entries(),
                index as int + 1,
                entries@,
            ));
        }
        index += 1;
    }
    let ghost superseded = entries@;
    let through = state.through_observation();
    entries = invalidate_entries(&entries, &state.environment, through);
    proof {
        WorkingState::entry_statuses_definition(entries@);
        state.invalidated_state_environment_definition(
            superseded,
            through,
        );
        assert(through == state.spec_observations().len() as u64);
        assert(super::validation::spec_status_view(entries@)
            == super::validation::spec_invalidation_result(
                superseded,
                &state.environment,
                through,
            ));
        reveal(WorkingDelta::spec_result_entries);
        assert(spec_upsert_prefix(
            state.spec_entries(),
            delta.spec_entries(),
            delta.spec_entries().len() as int,
            upserted,
        ));
        assert(spec_supersession_prefix(
            upserted,
            delta.spec_entries(),
            delta.spec_entries().len() as int,
            superseded,
        ));
        assert(WorkingEntry::sequence_payload_equivalent(superseded, entries@));
        assert(WorkingState::spec_entry_statuses(entries@)
            == WorkingState::spec_invalidated_entry_statuses(
                superseded,
                &state.spec_environment(),
                state.spec_observations().len() as u64,
            ));
        assert(exists |upsert_result: Seq<WorkingEntry>, supersession_result: Seq<WorkingEntry>|
            spec_upsert_prefix(
                state.spec_entries(),
                delta.spec_entries(),
                delta.spec_entries().len() as int,
                upsert_result,
            )
            && spec_supersession_prefix(
                upsert_result,
                delta.spec_entries(),
                delta.spec_entries().len() as int,
                supersession_result,
            )
            && WorkingEntry::sequence_payload_equivalent(supersession_result, entries@)
            && WorkingState::spec_entry_statuses(entries@)
                == WorkingState::spec_invalidated_entry_statuses(
                    supersession_result,
                    &state.spec_environment(),
                    state.spec_observations().len() as u64,
                ));
        assert(delta.spec_result_entries(state, entries@));
    }
    index = 0;
    while index < delta.entries.len()
        invariant
            index <= delta.entries.len(),
            delta.spec_result_entries(state, entries@),
        decreases delta.entries.len() - index,
    {
        let Some(target) = find_entry(&entries, delta.entries[index].id) else { return Err(WorkingError::MissingEntry); };
        if entries[target].status == WorkingEntryStatus::Stale { return Err(WorkingError::StaleEntry); }
        index += 1;
    }
    let next = state.with_entries(entries, revision);
    proof {
        assert(next.spec_entries() == entries@);
        assert(delta.spec_result_entries(state, next.spec_entries()));
    }
    Ok(next)
}

}

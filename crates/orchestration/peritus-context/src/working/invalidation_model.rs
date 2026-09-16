//! Logical model for conservative fixed-point invalidation.

#[cfg(verus_only)]
use super::validation::spec_find_entry_from;
#[cfg(verus_only)]
use super::{WorkingEntry, WorkingEntryStatus, WorkingEnvironment};
#[cfg(verus_only)]
use crate::ContextNodeId;
use vstd::prelude::*;

verus! {
pub(super) open spec fn spec_status_view(
    entries: Seq<WorkingEntry>,
) -> Seq<(WorkingEntryStatus, u64)> {
    Seq::new(entries.len(), |index: int| {
        (entries[index].spec_status(), entries[index].spec_stale_through())
    })
}

pub(super) open spec fn spec_unusable(status: WorkingEntryStatus) -> bool {
    status == WorkingEntryStatus::Stale || status == WorkingEntryStatus::Superseded
}

pub(super) open spec fn spec_dependency_unusable(
    entries: Seq<WorkingEntry>,
    statuses: Seq<(WorkingEntryStatus, u64)>,
    id: ContextNodeId,
) -> bool {
    match spec_find_entry_from(entries, id, 0) {
        Some(index) => spec_unusable(statuses[index].0),
        None => true,
    }
}

pub(super) open spec fn spec_stale_dependency_from(
    entries: Seq<WorkingEntry>,
    statuses: Seq<(WorkingEntryStatus, u64)>,
    dependencies: Seq<ContextNodeId>,
    start: int,
) -> bool
    decreases dependencies.len() - start,
{
    if start < 0 || start >= dependencies.len() {
        false
    } else if spec_dependency_unusable(entries, statuses, dependencies[start]) {
        true
    } else {
        spec_stale_dependency_from(entries, statuses, dependencies, start + 1)
    }
}

pub(super) open spec fn spec_stale_dependency(
    entries: Seq<WorkingEntry>,
    statuses: Seq<(WorkingEntryStatus, u64)>,
    entry: &WorkingEntry,
) -> bool {
    spec_stale_dependency_from(
        entries,
        statuses,
        entry.spec_links().spec_depends_on(),
        0,
    )
}

pub(super) open spec fn spec_invalidation_step(
    entries: Seq<WorkingEntry>,
    statuses: Seq<(WorkingEntryStatus, u64)>,
    environment: &WorkingEnvironment,
    through: u64,
    index: int,
) -> (WorkingEntryStatus, u64) {
    let current = statuses[index];
    let entry = &entries[index];
    if current.0 == WorkingEntryStatus::Stale
        && !entry.spec_validity().spec_holds(environment)
    {
        (WorkingEntryStatus::Stale, through)
    } else if !spec_unusable(current.0)
        && (!entry.spec_validity().spec_holds(environment)
            || spec_stale_dependency(entries, statuses, entry))
    {
        (WorkingEntryStatus::Stale, through)
    } else {
        current
    }
}

pub(super) open spec fn spec_invalidation_scan_prefix(
    entries: Seq<WorkingEntry>,
    statuses: Seq<(WorkingEntryStatus, u64)>,
    environment: &WorkingEnvironment,
    through: u64,
    count: nat,
) -> (Seq<(WorkingEntryStatus, u64)>, bool)
    decreases count,
{
    if count == 0 {
        (statuses, false)
    } else {
        let prior = spec_invalidation_scan_prefix(
            entries,
            statuses,
            environment,
            through,
            (count - 1) as nat,
        );
        let index = count as int - 1;
        let updated = spec_invalidation_step(entries, prior.0, environment, through, index);
        (
            prior.0.update(index, updated),
            prior.1 || (!spec_unusable(prior.0[index].0)
                && updated.0 == WorkingEntryStatus::Stale),
        )
    }
}

pub(super) open spec fn spec_invalidation_execute(
    entries: Seq<WorkingEntry>,
    statuses: Seq<(WorkingEntryStatus, u64)>,
    environment: &WorkingEnvironment,
    through: u64,
    remaining: nat,
) -> Seq<(WorkingEntryStatus, u64)>
    decreases remaining,
{
    if remaining == 0 {
        statuses
    } else {
        let scan = spec_invalidation_scan_prefix(
            entries,
            statuses,
            environment,
            through,
            entries.len(),
        );
        if scan.1 {
            spec_invalidation_execute(
                entries,
                scan.0,
                environment,
                through,
                (remaining - 1) as nat,
            )
        } else {
            scan.0
        }
    }
}

pub(super) open spec fn spec_invalidation_result(
    entries: Seq<WorkingEntry>,
    environment: &WorkingEnvironment,
    through: u64,
) -> Seq<(WorkingEntryStatus, u64)> {
    spec_invalidation_execute(
        entries,
        spec_status_view(entries),
        environment,
        through,
        entries.len(),
    )
}

proof fn find_entry_payload_invariant(
    left: Seq<WorkingEntry>,
    right: Seq<WorkingEntry>,
    id: ContextNodeId,
    start: int,
)
    requires
        WorkingEntry::sequence_payload_equivalent(left, right),
        0 <= start <= left.len(),
    ensures
        spec_find_entry_from(left, id, start)
            == spec_find_entry_from(right, id, start),
    decreases left.len() - start,
{
    reveal(WorkingEntry::sequence_payload_equivalent);
    if start < left.len() {
        reveal(WorkingEntry::payload_equivalent);
        reveal_with_fuel(spec_find_entry_from, 1);
        if !left[start].spec_id().spec_matches(&id) {
            find_entry_payload_invariant(left, right, id, start + 1);
        }
    }
}

proof fn stale_dependency_from_payload_invariant(
    left: Seq<WorkingEntry>,
    right: Seq<WorkingEntry>,
    statuses: Seq<(WorkingEntryStatus, u64)>,
    left_dependencies: Seq<ContextNodeId>,
    right_dependencies: Seq<ContextNodeId>,
    start: int,
)
    requires
        WorkingEntry::sequence_payload_equivalent(left, right),
        statuses.len() == left.len(),
        left_dependencies == right_dependencies,
        0 <= start <= left_dependencies.len(),
    ensures
        spec_stale_dependency_from(left, statuses, left_dependencies, start)
            == spec_stale_dependency_from(right, statuses, right_dependencies, start),
    decreases left_dependencies.len() - start,
{
    if start < left_dependencies.len() {
        find_entry_payload_invariant(left, right, left_dependencies[start], 0);
        reveal_with_fuel(spec_stale_dependency_from, 1);
        if !spec_dependency_unusable(left, statuses, left_dependencies[start]) {
            stale_dependency_from_payload_invariant(
                left,
                right,
                statuses,
                left_dependencies,
                right_dependencies,
                start + 1,
            );
        }
    }
}

pub(super) proof fn invalidation_step_payload_invariant(
    left: Seq<WorkingEntry>,
    right: Seq<WorkingEntry>,
    statuses: Seq<(WorkingEntryStatus, u64)>,
    environment: &WorkingEnvironment,
    through: u64,
    index: int,
)
    requires
        WorkingEntry::sequence_payload_equivalent(left, right),
        statuses.len() == left.len(),
        0 <= index < left.len(),
    ensures
        spec_invalidation_step(left, statuses, environment, through, index)
            == spec_invalidation_step(right, statuses, environment, through, index),
{
    reveal(WorkingEntry::sequence_payload_equivalent);
    reveal(WorkingEntry::payload_equivalent);
    reveal(super::WorkingLinks::clone_equivalent);
    super::WorkingValidity::clone_equivalent_holds(
        &left[index].spec_validity(),
        &right[index].spec_validity(),
        environment,
    );
    stale_dependency_from_payload_invariant(
        left,
        right,
        statuses,
        left[index].spec_links().spec_depends_on(),
        right[index].spec_links().spec_depends_on(),
        0,
    );
}

pub(super) proof fn status_view_update(
    entries: Seq<WorkingEntry>,
    index: int,
    replacement: WorkingEntry,
)
    requires 0 <= index < entries.len(),
    ensures
        spec_status_view(entries.update(index, replacement))
            == spec_status_view(entries).update(
                index,
                (replacement.spec_status(), replacement.spec_stale_through()),
            ),
{
    reveal(spec_status_view);
    assert(spec_status_view(entries.update(index, replacement)) =~=
        spec_status_view(entries).update(
            index,
            (replacement.spec_status(), replacement.spec_stale_through()),
        ));
}

}

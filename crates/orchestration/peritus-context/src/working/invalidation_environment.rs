//! Extensional proof bridge for fixed-point invalidation environments.

#[cfg(verus_only)]
use super::invalidation_model::{
    spec_invalidation_execute, spec_invalidation_result, spec_invalidation_scan_prefix,
    spec_invalidation_step, spec_status_view,
};
#[cfg(verus_only)]
use super::{WorkingEntry, WorkingEnvironment, WorkingValidity};
use vstd::prelude::*;

verus! {
pub(super) open spec fn same_environment(
    left: &WorkingEnvironment,
    right: &WorkingEnvironment,
) -> bool {
    &&& left.spec_binding() == right.spec_binding()
    &&& left.spec_candidate() == right.spec_candidate()
    &&& left.spec_files() == right.spec_files()
}

proof fn invalidation_step_environment_equivalent(
    entries: Seq<WorkingEntry>,
    statuses: Seq<(super::WorkingEntryStatus, u64)>,
    left: &WorkingEnvironment,
    right: &WorkingEnvironment,
    through: u64,
    index: int,
)
    requires
        same_environment(left, right),
        0 <= index < entries.len(),
        statuses.len() == entries.len(),
    ensures spec_invalidation_step(entries, statuses, left, through, index)
        == spec_invalidation_step(entries, statuses, right, through, index),
{
    reveal(spec_invalidation_step);
    reveal(WorkingValidity::spec_holds);
}

proof fn invalidation_scan_environment_equivalent(
    entries: Seq<WorkingEntry>,
    statuses: Seq<(super::WorkingEntryStatus, u64)>,
    left: &WorkingEnvironment,
    right: &WorkingEnvironment,
    through: u64,
    count: nat,
)
    requires
        same_environment(left, right),
        statuses.len() == entries.len(),
        count <= entries.len(),
    ensures
        spec_invalidation_scan_prefix(entries, statuses, left, through, count)
            == spec_invalidation_scan_prefix(entries, statuses, right, through, count),
        spec_invalidation_scan_prefix(entries, statuses, left, through, count).0.len()
            == statuses.len(),
    decreases count,
{
    reveal_with_fuel(spec_invalidation_scan_prefix, 1);
    if count > 0 {
        invalidation_scan_environment_equivalent(
            entries,
            statuses,
            left,
            right,
            through,
            (count - 1) as nat,
        );
        let prior = spec_invalidation_scan_prefix(
            entries,
            statuses,
            left,
            through,
            (count - 1) as nat,
        );
        invalidation_step_environment_equivalent(
            entries,
            prior.0,
            left,
            right,
            through,
            count as int - 1,
        );
    }
}

proof fn invalidation_execute_environment_equivalent(
    entries: Seq<WorkingEntry>,
    statuses: Seq<(super::WorkingEntryStatus, u64)>,
    left: &WorkingEnvironment,
    right: &WorkingEnvironment,
    through: u64,
    remaining: nat,
)
    requires
        same_environment(left, right),
        statuses.len() == entries.len(),
        remaining <= entries.len(),
    ensures spec_invalidation_execute(entries, statuses, left, through, remaining)
        == spec_invalidation_execute(entries, statuses, right, through, remaining),
    decreases remaining,
{
    reveal_with_fuel(spec_invalidation_execute, 1);
    if remaining > 0 {
        invalidation_scan_environment_equivalent(
            entries,
            statuses,
            left,
            right,
            through,
            entries.len(),
        );
        let scan = spec_invalidation_scan_prefix(
            entries,
            statuses,
            left,
            through,
            entries.len(),
        );
        if scan.1 {
            invalidation_execute_environment_equivalent(
                entries,
                scan.0,
                left,
                right,
                through,
                (remaining - 1) as nat,
            );
        }
    }
}

pub(super) proof fn invalidation_result_environment_equivalent(
    entries: Seq<WorkingEntry>,
    left: &WorkingEnvironment,
    right: &WorkingEnvironment,
    through: u64,
)
    requires same_environment(left, right),
    ensures spec_invalidation_result(entries, left, through)
        == spec_invalidation_result(entries, right, through),
{
    reveal(spec_invalidation_result);
    reveal(spec_status_view);
    invalidation_execute_environment_equivalent(
        entries,
        spec_status_view(entries),
        left,
        right,
        through,
        entries.len(),
    );
}
}

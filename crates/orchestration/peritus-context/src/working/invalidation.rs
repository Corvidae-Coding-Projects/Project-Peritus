//! Exact bounded invalidation reducer.

#[cfg(verus_only)]
use super::invalidation_model::{
    invalidation_step_payload_invariant, spec_invalidation_execute,
    spec_invalidation_result, spec_invalidation_scan_prefix, spec_invalidation_step,
    spec_stale_dependency, spec_status_view, spec_unusable, status_view_update,
};
use super::staleness::stale_dependency;
use super::validation::unusable;
use super::{WorkingEntry, WorkingEntryStatus, WorkingEnvironment};
use vstd::prelude::*;

verus! {
#[allow(
    clippy::branches_sharing_code,
    clippy::semicolon_if_nothing_returned,
    clippy::too_many_lines,
    clippy::useless_let_if_seq,
    reason = "the executable loop is kept aligned with its fixed-point proof"
)]
pub(super) fn invalidate_entries(
    entries: &[WorkingEntry],
    environment: &WorkingEnvironment,
    through: u64,
) -> (result: Vec<WorkingEntry>)
    ensures
        WorkingEntry::sequence_payload_equivalent(entries@, result@),
        spec_status_view(result@) == spec_invalidation_result(entries@, environment, through),
{
    let mut result = WorkingEntry::clone_sequence(entries);
    let ghost expected = spec_invalidation_result(entries@, environment, through);
    proof {
        WorkingEntry::sequence_clone_implies_payload(entries@, result@);
        reveal(WorkingEntry::sequence_clone_equivalent);
        reveal(WorkingEntry::clone_equivalent);
        reveal(spec_status_view);
        assert(spec_status_view(result@) =~= spec_status_view(entries@));
    }
    let mut pass = 0;
    while pass < entries.len()
        invariant
            pass <= entries.len(),
            result.len() == entries.len(),
            WorkingEntry::sequence_payload_equivalent(entries@, result@),
            expected == spec_invalidation_result(entries@, environment, through),
            spec_invalidation_execute(
                entries@,
                spec_status_view(result@),
                environment,
                through,
                (entries.len() - pass) as nat,
            ) == expected,
        decreases entries.len() - pass,
    {
        let ghost before_statuses = spec_status_view(result@);
        let ghost remaining = (entries.len() - pass) as nat;
        let ghost before_execution = spec_invalidation_execute(
            entries@,
            before_statuses,
            environment,
            through,
            remaining,
        );
        proof { assert(before_execution == expected); }
        let mut changed = false;
        let mut index = 0;
        while index < result.len()
            invariant
                index <= result.len(),
                result.len() == entries.len(),
                WorkingEntry::sequence_payload_equivalent(entries@, result@),
                expected == spec_invalidation_result(entries@, environment, through),
                spec_status_view(result@)
                    == spec_invalidation_scan_prefix(
                        entries@,
                        before_statuses,
                        environment,
                        through,
                        index as nat,
                    ).0,
                changed == spec_invalidation_scan_prefix(
                    entries@,
                    before_statuses,
                    environment,
                    through,
                    index as nat,
                ).1,
            decreases result.len() - index,
        {
            let ghost before_entries = result@;
            let ghost before_step_statuses = spec_status_view(before_entries);
            let ghost before = result@[index as int];
            let ghost prior_scan = spec_invalidation_scan_prefix(
                entries@,
                before_statuses,
                environment,
                through,
                index as nat,
            );
            let ghost was_changed = changed;
            proof {
                assert(before_step_statuses == prior_scan.0);
                assert(was_changed == prior_scan.1);
                assert(WorkingEntry::payload_equivalent(
                    &entries@[index as int],
                    &before,
                ));
                invalidation_step_payload_invariant(
                    entries@,
                    before_entries,
                    before_step_statuses,
                    environment,
                    through,
                    index as int,
                );
                reveal(spec_status_view);
            }
            let status = result[index].status();
            let validity = result[index].validity();
            let validity_holds = validity.holds(environment);
            let is_unusable = unusable(status);
            let mut dependency_stale = false;
            if !is_unusable && validity_holds {
                dependency_stale = stale_dependency(&result, &result[index]);
            }
            let should_stale = !is_unusable && (!validity_holds || dependency_stale);
            let refresh_stale = is_stale(status) && !validity_holds;
            let mut newly_stale = false;
            proof {
                reveal(spec_status_view);
                reveal(spec_invalidation_step);
                reveal(spec_unusable);
                super::WorkingValidity::clone_equivalent_holds(
                    validity,
                    &before.spec_validity(),
                    environment,
                );
                assert(status == before.spec_status());
                assert(is_unusable == spec_unusable(status));
                assert(validity_holds == before.spec_validity().spec_holds(environment));
                assert(before_step_statuses[index as int]
                    == (before.spec_status(), before.spec_stale_through()));
                assert(dependency_stale == (if !spec_unusable(status) && validity_holds {
                    spec_stale_dependency(
                        before_entries,
                        before_step_statuses,
                        &before_entries[index as int],
                    )
                } else {
                    false
                }));
                assert(should_stale == (!spec_unusable(before_step_statuses[index as int].0)
                    && (!before_entries[index as int].spec_validity().spec_holds(environment)
                        || spec_stale_dependency(
                            before_entries,
                            before_step_statuses,
                            &before_entries[index as int],
                        ))));
            }
            if refresh_stale {
                let replacement = result[index].clone().with_host_status(
                    WorkingEntryStatus::Stale,
                    through,
                );
                result[index] = replacement;
                proof {
                    reveal(spec_invalidation_step);
                    reveal(spec_unusable);
                    assert(refresh_stale);
                    assert(status == WorkingEntryStatus::Stale);
                    assert(!validity_holds);
                    assert(status == before.spec_status());
                    assert(before_step_statuses[index as int]
                        == (before.spec_status(), before.spec_stale_through()));
                    assert(before_step_statuses[index as int].0 == WorkingEntryStatus::Stale);
                    assert(!before_entries[index as int].spec_validity().spec_holds(environment));
                    assert(result@ == before_entries.update(index as int, replacement));
                    status_view_update(before_entries, index as int, replacement);
                    assert(spec_invalidation_step(
                        before_entries,
                        before_step_statuses,
                        environment,
                        through,
                        index as int,
                    ) == (WorkingEntryStatus::Stale, through));
                }
            } else if should_stale {
                let replacement = result[index].clone().with_host_status(
                    WorkingEntryStatus::Stale,
                    through,
                );
                result[index] = replacement;
                newly_stale = true;
                proof {
                    reveal(spec_invalidation_step);
                    reveal(spec_unusable);
                    assert(!spec_unusable(before_step_statuses[index as int].0));
                    assert(!before_entries[index as int].spec_validity().spec_holds(environment)
                        || spec_stale_dependency(
                            before_entries,
                            before_step_statuses,
                            &before_entries[index as int],
                        ));
                    assert(result@ == before_entries.update(index as int, replacement));
                    status_view_update(before_entries, index as int, replacement);
                    assert(spec_invalidation_step(
                        before_entries,
                        before_step_statuses,
                        environment,
                        through,
                        index as int,
                    ) == (WorkingEntryStatus::Stale, through));
                }
            } else {
                proof {
                    reveal(spec_invalidation_step);
                    reveal(spec_unusable);
                    assert(!refresh_stale);
                    assert(!should_stale);
                    assert(status == before.spec_status());
                    assert(validity_holds == before.spec_validity().spec_holds(environment));
                    assert(!(before_step_statuses[index as int].0 == WorkingEntryStatus::Stale
                        && !before_entries[index as int].spec_validity().spec_holds(environment)));
                    assert(!(!spec_unusable(before_step_statuses[index as int].0)
                        && (!before_entries[index as int].spec_validity().spec_holds(environment)
                            || spec_stale_dependency(
                                before_entries,
                                before_step_statuses,
                                &before_entries[index as int],
                            ))));
                    assert(result@ == before_entries);
                    assert(spec_invalidation_step(
                        before_entries,
                        before_step_statuses,
                        environment,
                        through,
                        index as int,
                    ) == before_step_statuses[index as int]);
                }
            }
            if newly_stale { changed = true; }
            proof {
                reveal(spec_invalidation_step);
                reveal(spec_unusable);
                let model_step = spec_invalidation_step(
                    entries@,
                    before_step_statuses,
                    environment,
                    through,
                    index as int,
                );
                assert(WorkingEntry::payload_equivalent(&before, &result@[index as int]));
                WorkingEntry::payload_transitive(
                    &entries@[index as int],
                    &before,
                    &result@[index as int],
                );
                assert(spec_status_view(result@)
                    == before_step_statuses.update(index as int, model_step));
                assert(newly_stale == should_stale);
                assert(newly_stale == (!spec_unusable(before_step_statuses[index as int].0)
                    && model_step.0 == WorkingEntryStatus::Stale));
                assert(changed == (was_changed || newly_stale));
                reveal_with_fuel(spec_invalidation_scan_prefix, 1);
                assert(spec_invalidation_scan_prefix(
                    entries@,
                    before_statuses,
                    environment,
                    through,
                    index as nat + 1,
                ).0 == before_step_statuses.update(index as int, model_step));
                assert(spec_invalidation_scan_prefix(
                    entries@,
                    before_statuses,
                    environment,
                    through,
                    index as nat + 1,
                ).1 == (was_changed || newly_stale));
            }
            index += 1;
        }
        if !changed {
            proof {
                let scan = spec_invalidation_scan_prefix(
                    entries@,
                    before_statuses,
                    environment,
                    through,
                    entries@.len(),
                );
                assert(scan.0 == spec_status_view(result@));
                assert(!scan.1);
                assert(remaining > 0);
                reveal_with_fuel(spec_invalidation_execute, 1);
                assert(before_execution == scan.0);
                assert(spec_status_view(result@) == expected);
                assert(spec_status_view(result@)
                    == spec_invalidation_result(entries@, environment, through));
            }
            return result;
        }
        proof {
            reveal_with_fuel(spec_invalidation_execute, 1);
        }
        pass += 1;
    }
    proof {
        reveal_with_fuel(spec_invalidation_execute, 1);
        assert(spec_status_view(result@) == expected);
        assert(spec_status_view(result@)
            == spec_invalidation_result(entries@, environment, through));
    }
    result
}

const fn is_stale(status: WorkingEntryStatus) -> (stale: bool)
    ensures stale == (status == WorkingEntryStatus::Stale),
{
    matches!(status, WorkingEntryStatus::Stale)
}
}

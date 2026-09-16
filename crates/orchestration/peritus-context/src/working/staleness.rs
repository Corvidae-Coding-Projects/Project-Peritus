//! Exact dependency-staleness scan.

#[cfg(verus_only)]
use super::invalidation_model::{
    spec_dependency_unusable, spec_stale_dependency, spec_stale_dependency_from,
    spec_status_view,
};
#[cfg(verus_only)]
use super::validation::spec_find_entry_from;
use super::validation::{find_entry, unusable};
use super::WorkingEntry;
use vstd::prelude::*;

verus! {
#[allow(clippy::too_many_lines, reason = "each early return proves the exact dependency scan suffix")]
pub(super) fn stale_dependency(entries: &[WorkingEntry], entry: &WorkingEntry) -> (stale: bool)
    ensures stale == spec_stale_dependency(entries@, spec_status_view(entries@), entry),
{
    let links = entry.links();
    let dependencies = links.depends_on();
    proof {
        reveal(super::WorkingLinks::clone_equivalent);
        reveal(spec_stale_dependency);
        assert(links.spec_depends_on() == entry.spec_links().spec_depends_on());
        assert(dependencies@ == links.spec_depends_on());
        assert(dependencies@ == entry.spec_links().spec_depends_on());
        assert(spec_stale_dependency(entries@, spec_status_view(entries@), entry)
            == spec_stale_dependency_from(
                entries@,
                spec_status_view(entries@),
                dependencies@,
                0,
            ));
    }
    let mut index = 0;
    while index < dependencies.len()
        invariant
            index <= dependencies.len(),
            spec_stale_dependency_from(
                entries@,
                spec_status_view(entries@),
                dependencies@,
                index as int,
            ) == spec_stale_dependency_from(
                entries@,
                spec_status_view(entries@),
                dependencies@,
                0,
            ),
            spec_stale_dependency(entries@, spec_status_view(entries@), entry)
                == spec_stale_dependency_from(
                    entries@,
                    spec_status_view(entries@),
                    dependencies@,
                    0,
                ),
        decreases dependencies.len() - index,
    {
        let Some(target) = find_entry(entries, dependencies[index]) else {
            reveal_with_fuel(spec_stale_dependency_from, 1);
            assert(spec_find_entry_from(entries@, dependencies@[index as int], 0) == None);
            assert(spec_dependency_unusable(
                entries@,
                spec_status_view(entries@),
                dependencies@[index as int],
            ));
            assert(spec_stale_dependency_from(
                entries@,
                spec_status_view(entries@),
                dependencies@,
                index as int,
            ));
            assert(spec_stale_dependency_from(
                entries@,
                spec_status_view(entries@),
                dependencies@,
                0,
            ));
            reveal(spec_stale_dependency);
            assert(spec_stale_dependency(entries@, spec_status_view(entries@), entry));
            return true;
        };
        let status = entries[target].status();
        if unusable(status) {
            reveal_with_fuel(spec_stale_dependency_from, 1);
            reveal(spec_status_view);
            assert(spec_find_entry_from(entries@, dependencies@[index as int], 0)
                == Some(target as int));
            assert(spec_status_view(entries@)[target as int].0 == status);
            assert(spec_dependency_unusable(
                entries@,
                spec_status_view(entries@),
                dependencies@[index as int],
            ));
            assert(spec_stale_dependency_from(
                entries@,
                spec_status_view(entries@),
                dependencies@,
                index as int,
            ));
            assert(spec_stale_dependency_from(
                entries@,
                spec_status_view(entries@),
                dependencies@,
                0,
            ));
            reveal(spec_stale_dependency);
            assert(spec_stale_dependency(entries@, spec_status_view(entries@), entry));
            return true;
        }
        reveal_with_fuel(spec_stale_dependency_from, 1);
        reveal(spec_status_view);
        assert(spec_find_entry_from(entries@, dependencies@[index as int], 0)
            == Some(target as int));
        assert(spec_status_view(entries@)[target as int].0 == status);
        assert(!spec_dependency_unusable(
            entries@,
            spec_status_view(entries@),
            dependencies@[index as int],
        ));
        assert(spec_stale_dependency_from(
            entries@,
            spec_status_view(entries@),
            dependencies@,
            index as int,
        ) == spec_stale_dependency_from(
            entries@,
            spec_status_view(entries@),
            dependencies@,
            index as int + 1,
        ));
        index += 1;
    }
    reveal_with_fuel(spec_stale_dependency_from, 1);
    assert(!spec_stale_dependency_from(
        entries@,
        spec_status_view(entries@),
        dependencies@,
        index as int,
    ));
    assert(!spec_stale_dependency(entries@, spec_status_view(entries@), entry));
    reveal_with_fuel(spec_stale_dependency_from, 1);
    false
}

}

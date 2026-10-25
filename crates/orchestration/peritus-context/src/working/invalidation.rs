//! Exact bounded invalidation reducer.

use super::staleness::stale_dependency;
use super::validation::unusable;
use super::{WorkingEntry, WorkingEntryStatus, WorkingEnvironment};

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
) -> Vec<WorkingEntry> {
    let mut result = WorkingEntry::clone_sequence(entries);
    let mut pass = 0;
    while pass < entries.len() {
        let mut changed = false;
        let mut index = 0;
        while index < result.len() {
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
            if refresh_stale {
                let replacement =
                    result[index].clone().with_host_status(WorkingEntryStatus::Stale, through);
                result[index] = replacement;
            } else if should_stale {
                let replacement =
                    result[index].clone().with_host_status(WorkingEntryStatus::Stale, through);
                result[index] = replacement;
                newly_stale = true;
            }
            if newly_stale {
                changed = true;
            }
            index += 1;
        }
        if !changed {
            return result;
        }
        pass += 1;
    }
    result
}
const fn is_stale(status: WorkingEntryStatus) -> bool {
    matches!(status, WorkingEntryStatus::Stale)
}

//! Canonical finite identity set used by cancellation traversal.

use crate::WorkId;

fn lower_bound(values: &[WorkId], id: WorkId) -> usize {
    let mut low = 0;
    let mut high = values.len();
    while low < high {
        let middle = low + (high - low) / 2;
        let observed = values[middle];
        let goes_right = observed.precedes(&id);
        if goes_right {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}
pub(super) fn contains(values: &[WorkId], id: WorkId) -> bool {
    let at = lower_bound(values, id);
    if at < values.len() && values[at].same(&id) { true } else { false }
}
pub(super) fn insert(values: &mut Vec<WorkId>, id: WorkId) -> bool {
    let at = lower_bound(values, id);
    if at < values.len() && values[at].same(&id) {
        return false;
    }
    values.insert(at, id);
    true
}

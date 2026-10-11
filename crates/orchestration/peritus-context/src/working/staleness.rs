//! Exact dependency-staleness scan.

use super::WorkingEntry;
use super::validation::{find_entry, unusable};

#[allow(
    clippy::too_many_lines,
    reason = "each early return proves the exact dependency scan suffix"
)]
pub(super) fn stale_dependency(entries: &[WorkingEntry], entry: &WorkingEntry) -> bool {
    let links = entry.links();
    let dependencies = links.depends_on();
    let mut index = 0;
    while index < dependencies.len() {
        let Some(target) = find_entry(entries, dependencies[index]) else {
            return true;
        };
        let status = entries[target].status();
        if unusable(status) {
            return true;
        }
        index += 1;
    }
    false
}

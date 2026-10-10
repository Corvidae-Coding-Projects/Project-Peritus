//! Executable merge state for exact canonical resource addition.

use std::cmp::Ordering;

use crate::{ResourceEntry, ResourceVector};

pub(super) enum MergeOutcome {
    Value(Vec<ResourceEntry>),
    Overflow,
}
struct MergeCursor {
    entries: Vec<ResourceEntry>,
    left: usize,
    right: usize,
}
enum MergeAdvance {
    Value(MergeCursor),
    Overflow,
}
fn advance_equal(
    mut cursor: MergeCursor,
    left_entries: &[ResourceEntry],
    right_entries: &[ResourceEntry],
) -> MergeAdvance {
    let left = &left_entries[cursor.left];
    let right = &right_entries[cursor.right];
    let Some(next) = super::combined_entry(left, right) else {
        return MergeAdvance::Overflow;
    };
    cursor.left += 1;
    cursor.right += 1;
    cursor.entries.push(next);
    MergeAdvance::Value(cursor)
}
fn advance_left(
    mut cursor: MergeCursor,
    left_entries: &[ResourceEntry],
    _right_entries: &[ResourceEntry],
) -> MergeCursor {
    let entry = left_entries[cursor.left];
    cursor.left += 1;
    cursor.entries.push(entry);
    cursor
}
fn advance_right(
    mut cursor: MergeCursor,
    _left_entries: &[ResourceEntry],
    right_entries: &[ResourceEntry],
) -> MergeCursor {
    let entry = right_entries[cursor.right];
    cursor.right += 1;
    cursor.entries.push(entry);
    cursor
}
fn advance(
    cursor: MergeCursor,
    left_entries: &[ResourceEntry],
    right_entries: &[ResourceEntry],
) -> MergeAdvance {
    if cursor.left >= left_entries.len() {
        MergeAdvance::Value(advance_right(cursor, left_entries, right_entries))
    } else if cursor.right >= right_entries.len() {
        MergeAdvance::Value(advance_left(cursor, left_entries, right_entries))
    } else {
        let left_tag = left_entries[cursor.left].kind().tag();
        let right_tag = right_entries[cursor.right].kind().tag();
        match left_tag.cmp(&right_tag) {
            Ordering::Equal => advance_equal(cursor, left_entries, right_entries),
            Ordering::Less => {
                MergeAdvance::Value(advance_left(cursor, left_entries, right_entries))
            }
            Ordering::Greater => {
                MergeAdvance::Value(advance_right(cursor, left_entries, right_entries))
            }
        }
    }
}
pub(super) fn merge_entries(
    left_vector: &ResourceVector,
    right_vector: &ResourceVector,
) -> MergeOutcome {
    let left_entries = left_vector.entries();
    let right_entries = right_vector.entries();
    let mut cursor = MergeCursor {
        entries: Vec::with_capacity(left_entries.len().saturating_add(right_entries.len())),
        left: 0,
        right: 0,
    };
    while cursor.left < left_entries.len() || cursor.right < right_entries.len() {
        cursor = match advance(cursor, left_entries, right_entries) {
            MergeAdvance::Value(next) => next,
            MergeAdvance::Overflow => return MergeOutcome::Overflow,
        };
    }
    MergeOutcome::Value(cursor.entries)
}

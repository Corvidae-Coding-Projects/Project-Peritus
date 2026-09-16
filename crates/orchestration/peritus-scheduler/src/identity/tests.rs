//! Executable agreement checks for canonical scheduler identity ordering.

use super::{DispatchId, WorkId, WorkerId};
use crate::SchedulerError;
use core::fmt::Debug;

fn assert_precedes_matches_derived_order<T>(
    create: fn([u8; 16]) -> Result<T, SchedulerError>,
    precedes: fn(&T, &T) -> bool,
) where
    T: Copy + Debug + Ord,
{
    let equal_left = create([0x80; 16]).expect("nonzero identity must be valid");
    let equal_right = create([0x80; 16]).expect("nonzero identity must be valid");
    assert_eq!(equal_left, equal_right);
    assert!(!precedes(&equal_left, &equal_right));
    assert_eq!(precedes(&equal_left, &equal_right), equal_left < equal_right);

    for first_difference in 0..16 {
        let mut left_bytes = [0x80; 16];
        let mut right_bytes = [0x80; 16];
        left_bytes[first_difference] = 0x7f;
        right_bytes[first_difference] = 0x81;
        let left = create(left_bytes).expect("nonzero identity must be valid");
        let right = create(right_bytes).expect("nonzero identity must be valid");
        assert_eq!(precedes(&left, &right), left < right);
        assert_eq!(precedes(&right, &left), right < left);
    }
}

#[test]
fn work_id_precedes_matches_derived_order_at_every_byte() {
    assert_precedes_matches_derived_order(WorkId::new, WorkId::precedes);
}

#[test]
fn worker_id_precedes_matches_derived_order_at_every_byte() {
    assert_precedes_matches_derived_order(WorkerId::new, WorkerId::precedes);
}

#[test]
fn dispatch_id_precedes_matches_derived_order_at_every_byte() {
    assert_precedes_matches_derived_order(DispatchId::new, DispatchId::precedes);
}

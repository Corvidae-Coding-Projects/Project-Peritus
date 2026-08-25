//! Checked identity, limit, canonical-vector, and arithmetic boundaries.

#![allow(clippy::unwrap_used, reason = "fixed checked test corpus")]

mod support;

use peritus_scheduler::{
    ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerErrorKind, WorkId,
};

use support::{Fixture, resources};

#[test]
fn identities_limits_and_vectors_reject_invalid_values() {
    assert_eq!(WorkId::new([0; 16]).unwrap_err().kind(), SchedulerErrorKind::InvalidInput);
    assert_eq!(ResourceKind::new(0).unwrap_err().kind(), SchedulerErrorKind::InvalidInput);
    assert_eq!(ResourceQuantity::new(0).unwrap_err().kind(), SchedulerErrorKind::InvalidInput);
    let fixture = Fixture::new();
    assert_eq!(
        ResourceVector::new(Vec::new(), fixture.limits.resource_dimensions()).unwrap_err().kind(),
        SchedulerErrorKind::LimitExceeded
    );
    let duplicate = vec![
        ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(1).unwrap()),
        ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(2).unwrap()),
    ];
    assert_eq!(
        ResourceVector::new(duplicate, fixture.limits.resource_dimensions()).unwrap_err().kind(),
        SchedulerErrorKind::NonCanonical
    );
}

#[test]
fn resource_addition_and_subtraction_are_exact_and_checked() {
    let fixture = Fixture::new();
    let left =
        resources(&[(ResourceKind::CPU, 2), (ResourceKind::MEMORY_BYTES, 100)], fixture.limits);
    let right = resources(&[(ResourceKind::CPU, 3)], fixture.limits);
    let sum = left.checked_add(&right, fixture.limits.resource_dimensions()).unwrap();
    assert_eq!(sum.quantity(ResourceKind::CPU), 5);
    assert_eq!(sum.quantity(ResourceKind::MEMORY_BYTES), 100);
    assert_eq!(sum.checked_subtract(&right).unwrap(), Some(left));
    let excessive = resources(&[(ResourceKind::CPU, 6)], fixture.limits);
    assert_eq!(
        sum.checked_subtract(&excessive).unwrap_err().kind(),
        SchedulerErrorKind::ResourceConflict
    );
    let maximum = resources(&[(ResourceKind::CPU, u64::MAX)], fixture.limits);
    assert_eq!(
        maximum.checked_add(&right, fixture.limits.resource_dimensions()).unwrap_err().kind(),
        SchedulerErrorKind::ResourceConflict
    );
}

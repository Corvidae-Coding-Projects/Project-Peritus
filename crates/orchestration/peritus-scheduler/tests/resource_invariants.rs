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

    let duplicate_over_limit = vec![
        ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(1).unwrap()),
        ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(2).unwrap()),
    ];
    assert_eq!(
        ResourceVector::new(duplicate_over_limit, 1).unwrap_err().kind(),
        SchedulerErrorKind::LimitExceeded
    );

    let descending = vec![
        ResourceEntry::new(ResourceKind::MEMORY_BYTES, ResourceQuantity::new(1).unwrap()),
        ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(1).unwrap()),
    ];
    assert_eq!(
        ResourceVector::new(descending, fixture.limits.resource_dimensions()).unwrap_err().kind(),
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
    let underflow = sum.checked_subtract(&excessive).unwrap_err();
    assert_eq!(underflow.kind(), SchedulerErrorKind::ResourceConflict);
    assert_eq!(underflow.detail(), "resource subtraction underflowed");

    let absent = resources(&[(ResourceKind::GPU, 1)], fixture.limits);
    let absent_error = sum.checked_subtract(&absent).unwrap_err();
    assert_eq!(absent_error.kind(), SchedulerErrorKind::ResourceConflict);
    assert_eq!(absent_error.detail(), "resource subtraction names an absent dimension");
    assert_eq!(sum.checked_subtract(&sum).unwrap(), None);

    let maximum = resources(&[(ResourceKind::CPU, u64::MAX)], fixture.limits);
    let overflow_before_limit = maximum.checked_add(&right, 0).unwrap_err();
    assert_eq!(overflow_before_limit.kind(), SchedulerErrorKind::ResourceConflict);
    assert_eq!(overflow_before_limit.detail(), "resource addition overflowed");

    let distinct = resources(&[(ResourceKind::GPU, 1)], fixture.limits);
    let dimension_limit = right.checked_add(&distinct, 1).unwrap_err();
    assert_eq!(dimension_limit.kind(), SchedulerErrorKind::LimitExceeded);
    assert_eq!(dimension_limit.detail(), "resource vector is empty or exceeds its dimension bound");
}

#[test]
fn indexed_quantity_preserves_present_and_absent_dimensions() {
    let fixture = Fixture::new();
    let first = ResourceKind::new(1).unwrap();
    let middle = ResourceKind::new(128).unwrap();
    let last = ResourceKind::new(256).unwrap();
    let vector = resources(&[(first, 11), (middle, 22), (last, 33)], fixture.limits);

    assert_eq!(vector.quantity(first), 11);
    assert_eq!(vector.quantity(middle), 22);
    assert_eq!(vector.quantity(last), 33);
    assert_eq!(vector.quantity(ResourceKind::new(2).unwrap()), 0);
    assert_eq!(vector.quantity(ResourceKind::new(127).unwrap()), 0);
    assert_eq!(vector.quantity(ResourceKind::new(129).unwrap()), 0);
    assert_eq!(vector.quantity(ResourceKind::new(255).unwrap()), 0);
    assert_eq!(vector.quantity(ResourceKind::new(257).unwrap()), 0);
}

//! Boundary tests for resource dimensions and checked arithmetic.

use peritus_types::{ResourceKind, ResourceQuantity, ResourceQuantityError};

#[test]
fn zero_and_all_resource_dimensions_are_representable() {
    assert_eq!(ResourceQuantity::zero().get(), 0);
    assert_eq!(ResourceQuantity::new(0).get(), 0);

    let kinds = [
        ResourceKind::ModelTokens,
        ResourceKind::ProviderCostMicrounits,
        ResourceKind::WallTimeMilliseconds,
        ResourceKind::CpuTimeMilliseconds,
        ResourceKind::MemoryBytes,
        ResourceKind::DiskBytes,
        ResourceKind::OutputBytes,
        ResourceKind::ProcessCount,
        ResourceKind::ConcurrencySlots,
        ResourceKind::AttemptCount,
        ResourceKind::RetryCount,
    ];
    assert_eq!(kinds.len(), 11);
}

#[test]
fn addition_is_exact_or_reports_overflow() {
    let sum =
        ResourceQuantity::new(40).checked_add(ResourceQuantity::new(2)).expect("representable sum");
    assert_eq!(sum.get(), 42);
    assert_eq!(
        ResourceQuantity::new(u64::MAX).checked_add(ResourceQuantity::new(1)),
        Err(ResourceQuantityError::Overflow)
    );
}

#[test]
fn subtraction_is_exact_or_reports_underflow() {
    let difference = ResourceQuantity::new(42)
        .checked_sub(ResourceQuantity::new(2))
        .expect("nonnegative difference");
    assert_eq!(difference.get(), 40);
    assert_eq!(
        ResourceQuantity::new(0).checked_sub(ResourceQuantity::new(1)),
        Err(ResourceQuantityError::Underflow)
    );
}

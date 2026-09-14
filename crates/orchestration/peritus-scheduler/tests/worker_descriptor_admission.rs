//! Public worker-descriptor admission agrees with canonical class and limit rules.

use peritus_scheduler::{
    ExecutionClass, ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector, SchedulerError,
    SchedulerErrorKind, SchedulerLimits, SchedulerRecoveryAction, WorkerDescriptor, WorkerId,
};
use peritus_types::ActorId;

const WORKER_DETAIL: &str =
    "worker classes or concurrency are empty, duplicated, unsorted, or out of bounds";
const CAPACITY_DETAIL: &str = "resource vector is empty or exceeds its dimension bound";

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}

fn limits(
    maximum_dimensions: u16,
    maximum_concurrency: u16,
) -> Result<SchedulerLimits, SchedulerError> {
    SchedulerLimits::new(
        32,
        64,
        8,
        8,
        maximum_dimensions,
        maximum_concurrency,
        4,
        2,
        4,
        1_048_576,
        4_194_304,
    )
}

fn resources(kinds: &[ResourceKind]) -> Result<ResourceVector, SchedulerError> {
    let quantity = ResourceQuantity::new(1)?;
    let entries = kinds.iter().map(|kind| ResourceEntry::new(*kind, quantity)).collect();
    ResourceVector::new(entries, u16::try_from(kinds.len()).unwrap_or(u16::MAX))
}

fn identity() -> Result<(WorkerId, ActorId), Box<dyn std::error::Error>> {
    Ok((WorkerId::new(bytes(1))?, ActorId::new(bytes(2)).expect("fixed nonzero actor identity")))
}

fn assert_rejection<T>(result: Result<T, SchedulerError>, kind: SchedulerErrorKind, detail: &str) {
    match result {
        Ok(_) => panic!("worker descriptor unexpectedly passed admission"),
        Err(error) => {
            assert_eq!(error.kind(), kind);
            assert_eq!(error.recovery(), SchedulerRecoveryAction::CorrectInput);
            assert_eq!(error.detail(), detail);
        }
    }
}

#[test]
fn every_execution_class_pair_agrees_with_derived_canonical_order()
-> Result<(), Box<dyn std::error::Error>> {
    let classes = [
        ExecutionClass::Model,
        ExecutionClass::Tool,
        ExecutionClass::Gate,
        ExecutionClass::Review,
        ExecutionClass::Coordination,
    ];
    let scheduler_limits = limits(1, 2)?;
    for left in classes {
        for right in classes {
            let (id, owner) = identity()?;
            let result = WorkerDescriptor::new(
                id,
                owner,
                vec![left, right],
                resources(&[ResourceKind::CPU])?,
                1,
                scheduler_limits,
            );
            if left < right {
                let descriptor = result?;
                assert_eq!(descriptor.id(), id);
                assert_eq!(descriptor.owner(), owner);
                assert_eq!(descriptor.classes(), [left, right]);
                assert_eq!(descriptor.capacity().entries().len(), 1);
                assert_eq!(descriptor.concurrency(), 1);
            } else {
                assert_rejection(result, SchedulerErrorKind::NonCanonical, WORKER_DETAIL);
            }
        }
    }

    let (id, owner) = identity()?;
    let descriptor = WorkerDescriptor::new(
        id,
        owner,
        classes.to_vec(),
        resources(&[ResourceKind::CPU])?,
        1,
        scheduler_limits,
    )?;
    assert_eq!(descriptor.classes(), classes);
    Ok(())
}

#[test]
fn empty_duplicate_and_descending_classes_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let scheduler_limits = limits(1, 2)?;
    for classes in [
        Vec::new(),
        vec![ExecutionClass::Tool, ExecutionClass::Tool],
        vec![ExecutionClass::Review, ExecutionClass::Gate],
    ] {
        let (id, owner) = identity()?;
        let result = WorkerDescriptor::new(
            id,
            owner,
            classes,
            resources(&[ResourceKind::CPU])?,
            1,
            scheduler_limits,
        );
        assert_rejection(result, SchedulerErrorKind::NonCanonical, WORKER_DETAIL);
    }
    Ok(())
}

#[test]
fn concurrency_accepts_its_maximum_and_rejects_zero_or_excess()
-> Result<(), Box<dyn std::error::Error>> {
    let scheduler_limits = limits(1, 2)?;
    for concurrency in [0, 3] {
        let (id, owner) = identity()?;
        let result = WorkerDescriptor::new(
            id,
            owner,
            vec![ExecutionClass::Tool],
            resources(&[ResourceKind::CPU])?,
            concurrency,
            scheduler_limits,
        );
        assert_rejection(result, SchedulerErrorKind::NonCanonical, WORKER_DETAIL);
    }

    let (id, owner) = identity()?;
    let descriptor = WorkerDescriptor::new(
        id,
        owner,
        vec![ExecutionClass::Tool],
        resources(&[ResourceKind::CPU])?,
        2,
        scheduler_limits,
    )?;
    assert_eq!(descriptor.concurrency(), 2);
    Ok(())
}

#[test]
fn capacity_limit_is_exact_and_class_rejection_keeps_priority()
-> Result<(), Box<dyn std::error::Error>> {
    let scheduler_limits = limits(1, 2)?;
    let (id, owner) = identity()?;
    let capacity_only = WorkerDescriptor::new(
        id,
        owner,
        vec![ExecutionClass::Tool],
        resources(&[ResourceKind::CPU, ResourceKind::MEMORY_BYTES])?,
        1,
        scheduler_limits,
    );
    assert_rejection(capacity_only, SchedulerErrorKind::LimitExceeded, CAPACITY_DETAIL);

    let (id, owner) = identity()?;
    let concurrency_and_capacity = WorkerDescriptor::new(
        id,
        owner,
        vec![ExecutionClass::Tool],
        resources(&[ResourceKind::CPU, ResourceKind::MEMORY_BYTES])?,
        0,
        scheduler_limits,
    );
    assert_rejection(concurrency_and_capacity, SchedulerErrorKind::NonCanonical, WORKER_DETAIL);

    let (id, owner) = identity()?;
    let conflicting = WorkerDescriptor::new(
        id,
        owner,
        vec![ExecutionClass::Tool, ExecutionClass::Tool],
        resources(&[ResourceKind::CPU, ResourceKind::MEMORY_BYTES])?,
        0,
        scheduler_limits,
    );
    assert_rejection(conflicting, SchedulerErrorKind::NonCanonical, WORKER_DETAIL);
    Ok(())
}

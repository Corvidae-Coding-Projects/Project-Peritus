//! Boundary tests for exact resource and queue accounting.

use peritus_benchmarks::{
    CapacityLimits, ConcurrencyLimits, QueueKind, QueueLimits, ResourceAccountant,
    ResourceEnvelope, ResourceEvent,
};

fn envelope() -> ResourceEnvelope {
    ResourceEnvelope::new(
        ConcurrencyLimits::new(2, 1, 1).expect("concurrency"),
        CapacityLimits::new(100, 100, 100).expect("capacity"),
        QueueLimits::new(2, 1, 1, 1).expect("queues"),
    )
}

#[test]
fn rejected_reservation_does_not_corrupt_lifecycle_balance() {
    let mut accounting = ResourceAccountant::new(envelope());
    accounting
        .apply(ResourceEvent::RunStarted { run: 1, memory_bytes: 60, disk_bytes: 10, tokens: 10 })
        .expect("first run");
    assert!(
        accounting
            .apply(ResourceEvent::RunStarted {
                run: 2,
                memory_bytes: 60,
                disk_bytes: 10,
                tokens: 10,
            })
            .is_err()
    );
    accounting.apply(ResourceEvent::RunFinished { run: 1 }).expect("release");
    assert!(accounting.summary().is_balanced());
}

#[test]
fn backpressure_requires_an_exact_full_queue() {
    let mut accounting = ResourceAccountant::new(envelope());
    assert!(
        accounting
            .apply(ResourceEvent::BackpressureObserved {
                queue: QueueKind::Command,
                wait_micros: 10,
            })
            .is_err()
    );
    accounting
        .apply(ResourceEvent::QueuePushed { queue: QueueKind::Command, count: 2 })
        .expect("fill");
    accounting
        .apply(ResourceEvent::BackpressureObserved { queue: QueueKind::Command, wait_micros: 10 })
        .expect("backpressure");
    assert_eq!(accounting.summary().saturation_observations(), 1);
}

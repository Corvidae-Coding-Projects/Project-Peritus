//! Focused resource-accounting unit tests.

use crate::{
    CapacityLimits, ConcurrencyLimits, QueueKind, QueueLimits, ResourceAccountant,
    ResourceEnvelope, ResourceEvent,
};

fn envelope() -> ResourceEnvelope {
    ResourceEnvelope::new(
        ConcurrencyLimits::new(1, 1, 1).expect("concurrency"),
        CapacityLimits::new(100, 100, 100).expect("capacity"),
        QueueLimits::new(1, 1, 1, 1).expect("queues"),
    )
}

#[test]
fn queue_overflow_is_rejected_without_mutation() {
    let mut accounting = ResourceAccountant::new(envelope());
    accounting
        .apply(ResourceEvent::QueuePushed { queue: QueueKind::Exporter, count: 1 })
        .expect("first push");
    assert!(
        accounting
            .apply(ResourceEvent::QueuePushed { queue: QueueKind::Exporter, count: 1 })
            .is_err()
    );
    assert_eq!(accounting.summary().peak_queue(QueueKind::Exporter), 1);
}

#[test]
fn released_run_is_balanced() {
    let mut accounting = ResourceAccountant::new(envelope());
    accounting
        .apply(ResourceEvent::RunStarted { run: 1, memory_bytes: 10, disk_bytes: 20, tokens: 30 })
        .expect("start");
    accounting.apply(ResourceEvent::RunFinished { run: 1 }).expect("finish");
    assert!(accounting.summary().is_balanced());
}

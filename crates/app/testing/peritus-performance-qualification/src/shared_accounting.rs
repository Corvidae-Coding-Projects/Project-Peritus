//! One bounded accounting ledger shared by concurrently executing soak subjects.

use std::sync::{Arc, Mutex};

use peritus_benchmarks::{
    AccountingSink, AccountingSummary, QualificationError, ResourceAccountant, ResourceEnvelope,
    ResourceEvent,
};

#[derive(Clone)]
pub struct SharedAccounting {
    inner: Arc<Mutex<ResourceAccountant>>,
}

impl SharedAccounting {
    pub fn new(envelope: ResourceEnvelope) -> Self {
        Self { inner: Arc::new(Mutex::new(ResourceAccountant::new(envelope))) }
    }

    pub fn summary(&self) -> Result<AccountingSummary, QualificationError> {
        self.inner.lock().map(|ledger| ledger.summary()).map_err(|_| poisoned())
    }
}

impl AccountingSink for SharedAccounting {
    fn apply(&mut self, event: ResourceEvent) -> Result<(), QualificationError> {
        self.inner.lock().map_err(|_| poisoned())?.apply(event)
    }
}

const fn poisoned() -> QualificationError {
    QualificationError::ResourceViolation {
        resource: "shared qualification ledger",
        reason: "an executing workload panicked while holding the accounting lock",
    }
}

#[cfg(test)]
mod tests {
    use peritus_benchmarks::{CapacityLimits, ConcurrencyLimits, QueueLimits, ResourceEvent};

    use super::*;

    #[test]
    fn clones_update_one_balanced_ledger() {
        let envelope = ResourceEnvelope::new(
            ConcurrencyLimits::new(2, 1, 1).expect("concurrency"),
            CapacityLimits::new(2, 2, 2).expect("capacity"),
            QueueLimits::new(1, 1, 1, 1).expect("queues"),
        );
        let mut first = SharedAccounting::new(envelope);
        let mut second = first.clone();
        first
            .apply(ResourceEvent::RunStarted { run: 1, memory_bytes: 1, disk_bytes: 1, tokens: 1 })
            .expect("start");
        second.apply(ResourceEvent::RunFinished { run: 1 }).expect("finish");
        let summary = first.summary().expect("summary");
        assert!(summary.is_balanced());
        assert_eq!(summary.peak_runs(), 1);
    }
}

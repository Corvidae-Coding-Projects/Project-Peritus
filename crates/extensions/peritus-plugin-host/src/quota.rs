//! Atomic lifecycle and concurrent-invocation quota accounting.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use peritus_plugin_sdk::{CumulativeQuota, PluginQuotas};

use crate::{HostError, HostFailureClass, RecoveryDisposition};

#[derive(Debug)]
pub struct QuotaLedger {
    limits: PluginQuotas,
    active: AtomicUsize,
    admitted: AtomicU64,
    replenishable_in_use: AtomicU64,
}

impl QuotaLedger {
    pub(crate) const fn new(limits: PluginQuotas) -> Self {
        Self {
            limits,
            active: AtomicUsize::new(0),
            admitted: AtomicU64::new(0),
            replenishable_in_use: AtomicU64::new(0),
        }
    }

    pub(crate) fn reserve(&self) -> Result<QuotaPermit<'_>, HostError> {
        let active = self.active.fetch_add(1, Ordering::SeqCst);
        if active >= usize::from(self.limits.concurrent_requests) {
            self.active.fetch_sub(1, Ordering::SeqCst);
            return Err(quota("plugin concurrent request quota is exhausted"));
        }
        let replenish_lifecycle = match self.limits.lifecycle_requests {
            CumulativeQuota::Limited { limit } => {
                if !increment_below(&self.admitted, limit) {
                    self.active.fetch_sub(1, Ordering::SeqCst);
                    return Err(quota("plugin lifecycle request quota is exhausted"));
                }
                false
            }
            CumulativeQuota::Replenishable { capacity } => {
                if !increment_below(&self.replenishable_in_use, capacity) {
                    self.active.fetch_sub(1, Ordering::SeqCst);
                    return Err(quota("plugin replenishable request capacity is exhausted"));
                }
                record_admission(&self.admitted);
                true
            }
            CumulativeQuota::Unlimited => {
                record_admission(&self.admitted);
                false
            }
        };
        Ok(QuotaPermit { ledger: self, replenish_lifecycle })
    }

    pub(crate) fn active(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }

    pub(crate) fn used(&self) -> u64 {
        self.admitted.load(Ordering::SeqCst)
    }

    pub(crate) const fn limits(&self) -> PluginQuotas {
        self.limits
    }
}

pub struct QuotaPermit<'a> {
    ledger: &'a QuotaLedger,
    replenish_lifecycle: bool,
}

impl Drop for QuotaPermit<'_> {
    fn drop(&mut self) {
        self.ledger.active.fetch_sub(1, Ordering::SeqCst);
        if self.replenish_lifecycle {
            self.ledger.replenishable_in_use.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

fn increment_below(counter: &AtomicU64, limit: u64) -> bool {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
            (current < limit).then_some(current + 1)
        })
        .is_ok()
}

fn record_admission(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
        Some(current.saturating_add(1))
    });
}

fn quota(detail: &'static str) -> HostError {
    HostError::new(
        HostFailureClass::Quota,
        RecoveryDisposition::RetryLater,
        "reserve plugin quota",
        detail,
    )
}

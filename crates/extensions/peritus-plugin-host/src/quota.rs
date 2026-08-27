//! Atomic lifecycle and concurrent-invocation quota accounting.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use peritus_plugin_sdk::PluginQuotas;

use crate::{HostError, HostFailureClass, RecoveryDisposition};

#[derive(Debug)]
pub struct QuotaLedger {
    limits: PluginQuotas,
    active: AtomicUsize,
    lifecycle_requests: AtomicU64,
}

impl QuotaLedger {
    pub(crate) const fn new(limits: PluginQuotas) -> Self {
        Self { limits, active: AtomicUsize::new(0), lifecycle_requests: AtomicU64::new(0) }
    }

    pub(crate) fn reserve(&self) -> Result<QuotaPermit<'_>, HostError> {
        let count = self.lifecycle_requests.fetch_add(1, Ordering::SeqCst);
        if count >= self.limits.lifecycle_requests {
            self.lifecycle_requests.fetch_sub(1, Ordering::SeqCst);
            return Err(quota("plugin lifecycle request quota is exhausted"));
        }
        let active = self.active.fetch_add(1, Ordering::SeqCst);
        if active >= usize::from(self.limits.concurrent_requests) {
            self.active.fetch_sub(1, Ordering::SeqCst);
            return Err(quota("plugin concurrent request quota is exhausted"));
        }
        Ok(QuotaPermit { ledger: self })
    }

    pub(crate) fn active(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }

    pub(crate) fn used(&self) -> u64 {
        self.lifecycle_requests.load(Ordering::SeqCst)
    }

    pub(crate) const fn limits(&self) -> PluginQuotas {
        self.limits
    }
}

pub struct QuotaPermit<'a> {
    ledger: &'a QuotaLedger,
}

impl Drop for QuotaPermit<'_> {
    fn drop(&mut self) {
        self.ledger.active.fetch_sub(1, Ordering::SeqCst);
    }
}

fn quota(detail: &'static str) -> HostError {
    HostError::new(
        HostFailureClass::Quota,
        RecoveryDisposition::RetryLater,
        "reserve plugin quota",
        detail,
    )
}

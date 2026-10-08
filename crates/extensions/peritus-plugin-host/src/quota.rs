//! Atomic lifecycle and concurrent-invocation quota accounting.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use peritus_plugin_sdk::{CumulativeQuota, PluginQuotas};

use crate::{HostError, HostFailureClass, RecoveryDisposition};

#[derive(Debug)]
pub struct QuotaLedger {
    limits: PluginQuotas,
    active: AtomicUsize,
    admitted: AtomicU64,
    limited_claimed: AtomicU64,
    replenishable_in_use: AtomicU64,
}

impl QuotaLedger {
    pub(crate) const fn new(limits: PluginQuotas) -> Self {
        Self {
            limits,
            active: AtomicUsize::new(0),
            admitted: AtomicU64::new(0),
            limited_claimed: AtomicU64::new(0),
            replenishable_in_use: AtomicU64::new(0),
        }
    }

    pub(crate) fn reserve(&self) -> Result<QuotaPermit<'_>, HostError> {
        if let CumulativeQuota::Limited { limit } = self.limits.lifecycle_requests
            && self.admitted.load(Ordering::SeqCst) >= limit
        {
            return Err(lifetime_exhausted());
        }
        let active = self.active.fetch_add(1, Ordering::SeqCst);
        if active >= usize::from(self.limits.concurrent_requests) {
            self.active.fetch_sub(1, Ordering::SeqCst);
            return Err(capacity_exhausted("plugin concurrent request quota is exhausted"));
        }
        let reservation = match self.limits.lifecycle_requests {
            CumulativeQuota::Limited { limit } => {
                if !increment_below(&self.limited_claimed, limit) {
                    self.active.fetch_sub(1, Ordering::SeqCst);
                    return Err(if self.admitted.load(Ordering::SeqCst) >= limit {
                        lifetime_exhausted()
                    } else {
                        capacity_exhausted(
                            "plugin lifecycle request reservations are temporarily exhausted",
                        )
                    });
                }
                Reservation::Limited
            }
            CumulativeQuota::Replenishable { capacity } => {
                if !increment_below(&self.replenishable_in_use, capacity) {
                    self.active.fetch_sub(1, Ordering::SeqCst);
                    return Err(capacity_exhausted(
                        "plugin replenishable request capacity is exhausted",
                    ));
                }
                Reservation::Replenishable
            }
            CumulativeQuota::Unlimited => Reservation::Unlimited,
        };
        Ok(QuotaPermit { ledger: self, reservation, admitted: false })
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
    reservation: Reservation,
    admitted: bool,
}

impl QuotaPermit<'_> {
    pub(crate) fn admit(&mut self) {
        if !self.admitted {
            record_admission(&self.ledger.admitted);
            self.admitted = true;
        }
    }

    pub(crate) const fn is_admitted(&self) -> bool {
        self.admitted
    }
}

impl Drop for QuotaPermit<'_> {
    fn drop(&mut self) {
        self.ledger.active.fetch_sub(1, Ordering::SeqCst);
        match self.reservation {
            Reservation::Limited if !self.admitted => {
                self.ledger.limited_claimed.fetch_sub(1, Ordering::SeqCst);
            }
            Reservation::Replenishable => {
                self.ledger.replenishable_in_use.fetch_sub(1, Ordering::SeqCst);
            }
            Reservation::Limited | Reservation::Unlimited => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Reservation {
    Limited,
    Replenishable,
    Unlimited,
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

fn capacity_exhausted(detail: &'static str) -> HostError {
    HostError::new(
        HostFailureClass::Quota,
        RecoveryDisposition::RetryLater,
        "reserve plugin quota",
        detail,
    )
}

fn lifetime_exhausted() -> HostError {
    HostError::new(
        HostFailureClass::Quota,
        RecoveryDisposition::CorrectRequest,
        "reserve plugin quota",
        "plugin lifecycle request quota is permanently exhausted for this lifecycle",
    )
}

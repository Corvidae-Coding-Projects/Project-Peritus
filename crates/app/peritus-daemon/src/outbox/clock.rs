//! Restart-monotonic authority-clock ticks for outbox leases.

use std::time::Instant;

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

const EPOCH_SHIFT: u32 = 32;
const EPOCH_WIDTH: u64 = 1_u64 << EPOCH_SHIFT;

/// Monotonic seconds scoped beneath one durable C0 authority epoch.
#[derive(Debug)]
pub(super) struct OutboxClock {
    base: u64,
    started: Instant,
}

impl OutboxClock {
    pub(super) fn new(authority_epoch: u64) -> Result<Self, DaemonError> {
        let base = authority_epoch
            .checked_shl(EPOCH_SHIFT)
            .filter(|value| i64::try_from(*value).is_ok())
            .ok_or_else(clock_exhausted)?;
        Ok(Self { base, started: Instant::now() })
    }

    pub(super) fn lease(&self, duration_seconds: u64) -> Result<(u64, u64), DaemonError> {
        let elapsed = self.started.elapsed().as_secs();
        let remaining = EPOCH_WIDTH.checked_sub(elapsed).ok_or_else(clock_exhausted)?;
        if duration_seconds == 0 || duration_seconds >= remaining {
            return Err(clock_exhausted());
        }
        let now = self.base.checked_add(elapsed).ok_or_else(clock_exhausted)?;
        let lease_until = now.checked_add(duration_seconds).ok_or_else(clock_exhausted)?;
        Ok((now, lease_until))
    }
}

fn clock_exhausted() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        "allocate outbox lease clock",
        "durable authority epoch or local lease window is exhausted",
    )
}

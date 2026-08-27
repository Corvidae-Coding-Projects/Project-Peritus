//! Epoch-bound monotonic authority time owned by the serialized daemon writer.

use std::time::Instant;

use peritus_policy::AuthorityInstant;
use peritus_types::Generation;

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

/// Monotonic millisecond clock whose epoch was durably allocated by C0 at startup.
#[derive(Debug)]
pub(crate) struct AuthorityClock {
    epoch: Generation,
    started: Instant,
}

impl AuthorityClock {
    /// Binds a fresh process-local monotonic clock to one positive durable authority epoch.
    ///
    /// # Errors
    ///
    /// Rejects the reserved zero epoch.
    pub(crate) fn new(epoch: u64) -> Result<Self, DaemonError> {
        let epoch = Generation::new(epoch).map_err(|_| clock_error("authority epoch is zero"))?;
        Ok(Self { epoch, started: Instant::now() })
    }

    /// Returns the current monotonic millisecond observation without crossing authority epochs.
    ///
    /// # Errors
    ///
    /// Rejects a platform duration that cannot be represented by the authority-time contract.
    pub(crate) fn now(&self) -> Result<AuthorityInstant, DaemonError> {
        let tick_millis = u64::try_from(self.started.elapsed().as_millis())
            .map_err(|_| clock_error("authority clock exceeded its millisecond range"))?;
        Ok(AuthorityInstant::new(self.epoch, tick_millis))
    }

    /// Returns the exact durable epoch bound to every observation.
    #[must_use]
    pub(crate) const fn epoch(&self) -> Generation {
        self.epoch
    }
}

fn clock_error(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        "observe approval authority time",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::AuthorityClock;

    #[test]
    fn observations_retain_epoch_and_do_not_regress() {
        let clock = AuthorityClock::new(7).expect("positive epoch");
        let first = clock.now().expect("first observation");
        let second = clock.now().expect("second observation");
        assert_eq!(clock.epoch().get(), 7);
        assert_eq!(first.epoch(), second.epoch());
        assert!(second.tick_millis() >= first.tick_millis());
    }

    #[test]
    fn zero_epoch_is_rejected() {
        assert!(AuthorityClock::new(0).is_err());
    }
}

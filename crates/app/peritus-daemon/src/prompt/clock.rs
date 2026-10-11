//! Epoch-bound monotonic authority time owned by the serialized daemon writer.

use std::time::Instant;

use peritus_policy::AuthorityInstant;
use peritus_types::Generation;

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

/// Monotonic millisecond clock whose epoch was durably allocated by C0 at startup.
#[derive(Debug)]
pub struct AuthorityClock {
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

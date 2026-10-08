//! Non-wrapping connection and aggregate network accounting.

use core::num::NonZeroU64;

use crate::{NetworkError, NetworkErrorKind, NetworkOperation, RecoveryClass};

/// Aggregate managed-network usage.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NetworkUsage {
    accepted_connections: u64,
    active_workers: u64,
    uploaded_bytes: u64,
    downloaded_bytes: u64,
}

impl NetworkUsage {
    /// Returns accepted connections.
    #[must_use]
    pub const fn accepted_connections(self) -> u64 {
        self.accepted_connections
    }
    /// Returns active workers.
    #[must_use]
    pub const fn active_workers(self) -> u64 {
        self.active_workers
    }
    /// Returns bytes sent upstream.
    #[must_use]
    pub const fn uploaded_bytes(self) -> u64 {
        self.uploaded_bytes
    }
    /// Returns bytes received upstream.
    #[must_use]
    pub const fn downloaded_bytes(self) -> u64 {
        self.downloaded_bytes
    }
    /// Returns exact bidirectional bytes when representable.
    #[must_use]
    pub const fn total_bytes(self) -> Option<u64> {
        self.uploaded_bytes.checked_add(self.downloaded_bytes)
    }
}

/// Per-connection byte and time accounting.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConnectionAccount {
    uploaded: u64,
    downloaded: u64,
    byte_limit: Option<NonZeroU64>,
    duration_limit_millis: Option<NonZeroU64>,
}

impl ConnectionAccount {
    /// Creates an empty connection account with optional caller-selected limits.
    #[must_use]
    pub const fn new(
        byte_limit: Option<NonZeroU64>,
        duration_limit_millis: Option<NonZeroU64>,
    ) -> Self {
        Self { uploaded: 0, downloaded: 0, byte_limit, duration_limit_millis }
    }
    /// Charges bytes sent to the upstream.
    ///
    /// # Errors
    /// Rejects overflow or a crossed ceiling without mutating the account.
    pub fn charge_upload(&mut self, bytes: u64) -> Result<(), NetworkError> {
        self.charge(bytes, true)
    }
    /// Charges bytes returned from the upstream.
    ///
    /// # Errors
    /// Rejects overflow or a crossed ceiling without mutating the account.
    pub fn charge_download(&mut self, bytes: u64) -> Result<(), NetworkError> {
        self.charge(bytes, false)
    }
    fn charge(&mut self, bytes: u64, upload: bool) -> Result<(), NetworkError> {
        let total = self
            .uploaded
            .checked_add(self.downloaded)
            .and_then(|value| value.checked_add(bytes))
            .ok_or_else(limit_error)?;
        if let Some(limit) = self.byte_limit {
            let used = self.uploaded.checked_add(self.downloaded).ok_or_else(limit_error)?;
            if !crate::verified::network_charge_allowed(used, bytes, limit.get())
                || total > limit.get()
            {
                return Err(limit_error());
            }
        }
        if upload {
            self.uploaded = self.uploaded.checked_add(bytes).ok_or_else(limit_error)?;
        } else {
            self.downloaded = self.downloaded.checked_add(bytes).ok_or_else(limit_error)?;
        }
        Ok(())
    }
    /// Verifies elapsed connection time.
    ///
    /// # Errors
    /// Returns a limit failure after the configured duration.
    pub const fn check_elapsed(&self, millis: u64) -> Result<(), NetworkError> {
        if matches!(self.duration_limit_millis, Some(limit) if millis > limit.get()) {
            Err(limit_error())
        } else {
            Ok(())
        }
    }
    /// Returns bytes uploaded.
    #[must_use]
    pub const fn uploaded(self) -> u64 {
        self.uploaded
    }
    /// Returns bytes downloaded.
    #[must_use]
    pub const fn downloaded(self) -> u64 {
        self.downloaded
    }
}

const fn limit_error() -> NetworkError {
    NetworkError::new(
        NetworkErrorKind::Limit,
        NetworkOperation::Relay,
        RecoveryClass::CancelAndJoin,
        "managed connection crossed a byte or duration ceiling",
    )
}

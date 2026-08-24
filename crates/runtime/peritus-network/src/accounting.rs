//! Non-wrapping connection and aggregate network accounting.

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
    /// Returns bidirectional bytes.
    #[must_use]
    pub const fn total_bytes(self) -> u64 {
        self.uploaded_bytes.saturating_add(self.downloaded_bytes)
    }
}

/// Per-connection byte and time accounting.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConnectionAccount {
    uploaded: u64,
    downloaded: u64,
    byte_limit: u64,
    duration_limit_millis: u64,
}

impl ConnectionAccount {
    /// Creates an empty bounded connection account.
    #[must_use]
    pub const fn new(byte_limit: u64, duration_limit_millis: u64) -> Self {
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
        if !crate::verified::network_charge_allowed(
            self.uploaded.saturating_add(self.downloaded),
            bytes,
            self.byte_limit,
        ) || total > self.byte_limit
        {
            return Err(limit_error());
        }
        if upload {
            self.uploaded = self.uploaded.saturating_add(bytes);
        } else {
            self.downloaded = self.downloaded.saturating_add(bytes);
        }
        Ok(())
    }
    /// Verifies elapsed connection time.
    ///
    /// # Errors
    /// Returns a limit failure after the configured duration.
    pub const fn check_elapsed(&self, millis: u64) -> Result<(), NetworkError> {
        if millis > self.duration_limit_millis { Err(limit_error()) } else { Ok(()) }
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

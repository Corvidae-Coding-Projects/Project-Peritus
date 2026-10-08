//! Distinct readiness and bounded diagnostic status values.

use peritus_types::Sha256Digest;

use super::{DaemonControlError, DaemonControlErrorKind, error::reject};

/// Closed truthful daemon readiness classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DaemonReadiness {
    /// Startup has not established service readiness.
    Starting,
    /// Read and mutation requests may be admitted subject to their normal checks.
    ReadyReadWrite,
    /// Diagnostic/read requests may be admitted; mutation requests must not be admitted.
    ReadyReadOnly,
    /// New work is closed while accepted work drains.
    Draining,
    /// The daemon cannot currently serve application requests.
    Unavailable,
}

impl DaemonReadiness {
    /// Returns whether this phase may admit mutation requests.
    #[must_use]
    pub const fn mutation_ready(self) -> bool {
        matches!(self, Self::ReadyReadWrite)
    }

    /// Returns whether this phase may answer diagnostic/read-only requests.
    #[must_use]
    pub const fn diagnostic_ready(self) -> bool {
        matches!(self, Self::ReadyReadWrite | Self::ReadyReadOnly | Self::Draining)
    }
}

/// Bounded daemon status observation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DaemonStatus {
    readiness: DaemonReadiness,
    diagnostic: Option<String>,
}

/// Exact non-secret identity of one live daemon process and its configured store.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DaemonInstance {
    store_id: [u8; 16],
    configuration_digest: Sha256Digest,
    executable_digest: Sha256Digest,
    process_id: u32,
    start_token: u64,
}

impl DaemonInstance {
    /// Creates one exact store, configuration, process, and operating-system birth identity.
    ///
    /// # Errors
    ///
    /// Rejects reserved zero store, process, or birth identities.
    pub const fn new(
        store_id: [u8; 16],
        configuration_digest: Sha256Digest,
        executable_digest: Sha256Digest,
        process_id: u32,
        start_token: u64,
    ) -> Result<Self, DaemonControlError> {
        if store_id == [0; 16] || process_id == 0 || start_token == 0 {
            return Err(reject(
                DaemonControlErrorKind::InvalidInput,
                "daemon instance identity contains a reserved zero value",
            ));
        }
        Ok(Self {
            store_id,
            configuration_digest,
            executable_digest,
            process_id,
            start_token,
        })
    }

    /// Returns the exact durable journal store identity.
    #[must_use]
    pub const fn store_id(self) -> [u8; 16] {
        self.store_id
    }
    /// Returns the digest of the exact configuration bytes loaded by this process.
    #[must_use]
    pub const fn configuration_digest(self) -> Sha256Digest {
        self.configuration_digest
    }
    /// Returns the digest of the exact running daemon executable.
    #[must_use]
    pub const fn executable_digest(self) -> Sha256Digest {
        self.executable_digest
    }
    /// Returns the native process identifier.
    #[must_use]
    pub const fn process_id(self) -> u32 {
        self.process_id
    }
    /// Returns the platform-native process birth token guarding against PID reuse.
    #[must_use]
    pub const fn start_token(self) -> u64 {
        self.start_token
    }
}

/// Authenticated protocol health for one exact live daemon instance.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DaemonHealth {
    status: DaemonStatus,
    instance: DaemonInstance,
}

impl DaemonHealth {
    /// Binds truthful readiness to the process and configuration that reported it.
    #[must_use]
    pub const fn new(status: DaemonStatus, instance: DaemonInstance) -> Self {
        Self { status, instance }
    }
    /// Borrows truthful bounded daemon readiness.
    #[must_use]
    pub const fn status(&self) -> &DaemonStatus {
        &self.status
    }
    /// Returns the exact live daemon instance identity.
    #[must_use]
    pub const fn instance(&self) -> DaemonInstance {
        self.instance
    }

    /// Constrains optional diagnostic prose without changing identity evidence.
    #[must_use]
    pub fn constrained(mut self, maximum_diagnostic_bytes: usize) -> Self {
        self.status = self.status.constrained(maximum_diagnostic_bytes);
        self
    }
}

impl DaemonStatus {
    /// Creates a readiness observation with optional inert bounded diagnostic text.
    ///
    /// # Errors
    ///
    /// Rejects a zero diagnostic bound or oversized diagnostic text.
    pub fn new(
        readiness: DaemonReadiness,
        diagnostic: Option<String>,
        maximum_diagnostic_bytes: usize,
    ) -> Result<Self, DaemonControlError> {
        if maximum_diagnostic_bytes == 0 {
            return Err(reject(
                DaemonControlErrorKind::InvalidLimit,
                "daemon diagnostic limit is zero",
            ));
        }
        if diagnostic.as_ref().is_some_and(|text| text.len() > maximum_diagnostic_bytes) {
            return Err(reject(
                DaemonControlErrorKind::InvalidInput,
                "daemon diagnostic exceeds its negotiated bound",
            ));
        }
        Ok(Self { readiness, diagnostic })
    }

    /// Returns the exact readiness phase.
    #[must_use]
    pub const fn readiness(&self) -> DaemonReadiness {
        self.readiness
    }
    /// Borrows optional inert diagnostic text.
    #[must_use]
    pub fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }

    /// Constrains optional diagnostic prose to a peer's negotiated UTF-8 byte ceiling.
    ///
    /// Readiness remains available when the ceiling cannot retain a complete character.
    #[must_use]
    pub fn constrained(mut self, maximum_diagnostic_bytes: usize) -> Self {
        let Some(diagnostic) = self.diagnostic.as_mut() else { return self };
        if diagnostic.len() <= maximum_diagnostic_bytes {
            return self;
        }
        let suffix = if maximum_diagnostic_bytes >= 4 { "..." } else { "" };
        let mut end = maximum_diagnostic_bytes.saturating_sub(suffix.len());
        while end > 0 && !diagnostic.is_char_boundary(end) {
            end -= 1;
        }
        if end == 0 && suffix.is_empty() {
            self.diagnostic = None;
            return self;
        }
        diagnostic.truncate(end);
        diagnostic.push_str(suffix);
        self
    }

    /// Returns whether mutation admission may proceed to its ordinary checks.
    #[must_use]
    pub const fn mutation_ready(&self) -> bool {
        self.readiness.mutation_ready()
    }
}

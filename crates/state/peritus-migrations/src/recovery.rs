//! Durable migration-operation recovery values.

use std::path::Path;

use peritus_types::Sha256Digest;

use crate::{MigrationError, MigrationErrorCode, MigrationVersion, RecoveryClass};

/// Stable caller-supplied identity for one retryable migration operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MigrationOperationId([u8; 16]);

impl MigrationOperationId {
    /// Creates an operation identity, rejecting the all-zero pattern.
    ///
    /// # Errors
    ///
    /// Returns an error for the reserved all-zero identity.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, MigrationError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(MigrationError::message(
            MigrationErrorCode::InvalidConfiguration,
            RecoveryClass::CorrectRequest,
            "validate migration operation identity",
            "operation identity must not be all zero",
        ))
    }

    /// Returns exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub(crate) fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(32);
        for byte in self.0 {
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        output
    }
}

/// Durable migration recovery state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryState {
    /// Operation identity and plan are durable; backup or apply may resume.
    Planned,
    /// Required consistent backup is durable and verified.
    BackupReady,
    /// Apply began; restart must inspect committed versions.
    Applying,
    /// Target committed and was reconciled.
    Applied,
    /// Apply failed after backup; explicit restore is required.
    RestoreRequired,
    /// Backup restoration completed.
    Restored,
    /// Transaction rolled back without a backup; retry requires a fresh explicit call.
    Failed,
}

impl RecoveryState {
    pub(crate) const fn tag(self) -> i64 {
        match self {
            Self::Planned => 1,
            Self::BackupReady => 2,
            Self::Applying => 3,
            Self::Applied => 4,
            Self::RestoreRequired => 5,
            Self::Restored => 6,
            Self::Failed => 7,
        }
    }

    pub(crate) const fn from_tag(tag: i64) -> Result<Self, MigrationError> {
        match tag {
            1 => Ok(Self::Planned),
            2 => Ok(Self::BackupReady),
            3 => Ok(Self::Applying),
            4 => Ok(Self::Applied),
            5 => Ok(Self::RestoreRequired),
            6 => Ok(Self::Restored),
            7 => Ok(Self::Failed),
            _ => Err(corrupt("unknown recovery state tag")),
        }
    }
}

/// Validated durable recovery operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryOperation {
    id: MigrationOperationId,
    from_version: u64,
    target_version: MigrationVersion,
    registry_digest: Sha256Digest,
    backup_required: bool,
    backup_digest: Option<Sha256Digest>,
    state: RecoveryState,
}

impl RecoveryOperation {
    pub(crate) const fn new(
        id: MigrationOperationId,
        from_version: u64,
        target_version: MigrationVersion,
        registry_digest: Sha256Digest,
        backup_required: bool,
        backup_digest: Option<Sha256Digest>,
        state: RecoveryState,
    ) -> Self {
        Self {
            id,
            from_version,
            target_version,
            registry_digest,
            backup_required,
            backup_digest,
            state,
        }
    }

    /// Returns operation identity.
    #[must_use]
    pub const fn id(&self) -> MigrationOperationId {
        self.id
    }
    /// Returns the exact pre-migration version.
    #[must_use]
    pub const fn from_version(&self) -> u64 {
        self.from_version
    }
    /// Returns planned target version.
    #[must_use]
    pub const fn target_version(&self) -> MigrationVersion {
        self.target_version
    }
    /// Returns registry digest.
    #[must_use]
    pub const fn registry_digest(&self) -> Sha256Digest {
        self.registry_digest
    }
    /// Returns whether backup was mandatory.
    #[must_use]
    pub const fn backup_required(&self) -> bool {
        self.backup_required
    }
    /// Returns verified backup digest when ready.
    #[must_use]
    pub const fn backup_digest(&self) -> Option<Sha256Digest> {
        self.backup_digest
    }
    /// Returns durable recovery state.
    #[must_use]
    pub const fn state(&self) -> RecoveryState {
        self.state
    }
}

/// Explicit action selected by restart reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryAction {
    /// Resume the same operation so its backup can be created.
    ResumeBackup(MigrationOperationId),
    /// Resume application using the already durable backup when present.
    ResumeApply(MigrationOperationId),
    /// Explicitly restore the verified backup.
    RestoreBackup(MigrationOperationId),
    /// Apply had committed; reconciliation marked it complete.
    ReconciledApplied(MigrationOperationId),
    /// A no-backup operation rolled back and may be explicitly retried.
    RetryApply(MigrationOperationId),
}

/// Canonically ordered restart reconciliation report.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryReport {
    actions: Vec<RecoveryAction>,
}

impl RecoveryReport {
    pub(crate) const fn new(actions: Vec<RecoveryAction>) -> Self {
        Self { actions }
    }

    /// Returns required actions ordered by operation identity.
    #[must_use]
    pub fn actions(&self) -> &[RecoveryAction] {
        &self.actions
    }
}

pub fn backup_path(
    directory: &Path,
    operation: MigrationOperationId,
    from_version: u64,
) -> std::path::PathBuf {
    directory.join(format!("migration-{}-from-{from_version}.sqlite3", operation.to_hex()))
}

pub const fn corrupt(message: &'static str) -> MigrationError {
    MigrationError::message(
        MigrationErrorCode::RecoveryCorrupt,
        RecoveryClass::Terminal,
        "validate recovery metadata",
        message,
    )
}

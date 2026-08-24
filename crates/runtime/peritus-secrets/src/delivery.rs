//! Exact environment, private-file, and brokered-handle delivery staging.

use core::fmt;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use peritus_sandbox::{BrokeredHandleLabel, EnvironmentName, SandboxPath, SecretDelivery};
use peritus_types::{EnvironmentId, ProcessId, Sha256Digest};

use crate::{
    RecoveryClass, SecretError, SecretErrorKind, SecretLease, SecretLeaseId, SecretMaterial,
    SecretOperation,
};

/// Nonsensitive delivery receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryReceipt {
    lease_id: SecretLeaseId,
    delivery: SecretDelivery,
    released: bool,
}

/// Exact live execution binding supplied to one secret delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecretDeliveryContext {
    owner: ProcessId,
    environment: EnvironmentId,
    sandbox_digest: Sha256Digest,
    execution_digest: Sha256Digest,
    now_epoch_millis: u64,
}

impl SecretDeliveryContext {
    /// Creates an exact current delivery context.
    #[must_use]
    pub const fn new(
        owner: ProcessId,
        environment: EnvironmentId,
        sandbox_digest: Sha256Digest,
        execution_digest: Sha256Digest,
        now_epoch_millis: u64,
    ) -> Self {
        Self { owner, environment, sandbox_digest, execution_digest, now_epoch_millis }
    }
}

impl DeliveryReceipt {
    /// Returns lease identity.
    #[must_use]
    pub const fn lease_id(&self) -> SecretLeaseId {
        self.lease_id
    }
    /// Returns exact destination.
    #[must_use]
    pub const fn delivery(&self) -> &SecretDelivery {
        &self.delivery
    }
    /// Returns whether cleanup completed.
    #[must_use]
    pub const fn released(&self) -> bool {
        self.released
    }
}

/// Backend-consumable delivery artifact with redacted formatting.
pub enum DeliveryArtifact {
    /// Environment value installed by the native helper immediately before target exec.
    Environment {
        /// Exact canonical environment name.
        name: EnvironmentName,
        /// Zeroizing bytes retained until helper delivery.
        material: SecretMaterial,
    },
    /// Private staging file mapped to an exact sandbox path.
    File {
        /// Exact sandbox-visible destination.
        sandbox_path: SandboxPath,
        /// Private host staging file owned by this session.
        staging_path: PathBuf,
    },
    /// Material delivered through a backend-owned protected handle.
    Brokered {
        /// Exact checked brokered-handle label.
        label: BrokeredHandleLabel,
        /// Zeroizing bytes retained until protected-handle delivery.
        material: SecretMaterial,
    },
}

impl DeliveryArtifact {
    /// Exposes environment material only to a scoped backend operation.
    pub fn expose_environment<R>(
        &self,
        operation: impl FnOnce(&EnvironmentName, &[u8]) -> R,
    ) -> Option<R> {
        match self {
            Self::Environment { name, material } => {
                Some(material.expose(|bytes| operation(name, bytes)))
            }
            _ => None,
        }
    }
    /// Exposes brokered material only to a scoped backend operation.
    pub fn expose_brokered<R>(
        &self,
        operation: impl FnOnce(&BrokeredHandleLabel, &[u8]) -> R,
    ) -> Option<R> {
        match self {
            Self::Brokered { label, material } => {
                Some(material.expose(|bytes| operation(label, bytes)))
            }
            _ => None,
        }
    }
    /// Returns private staging and sandbox paths for file mapping.
    #[must_use]
    pub fn file_paths(&self) -> Option<(&Path, &SandboxPath)> {
        match self {
            Self::File { sandbox_path, staging_path } => Some((staging_path, sandbox_path)),
            _ => None,
        }
    }
}

impl fmt::Debug for DeliveryArtifact {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Environment { name, .. } => formatter
                .debug_struct("Environment")
                .field("name", name)
                .field("material", &"[REDACTED]")
                .finish(),
            Self::File { sandbox_path, staging_path } => formatter
                .debug_struct("File")
                .field("sandbox_path", sandbox_path)
                .field("staging_path", staging_path)
                .finish(),
            Self::Brokered { label, .. } => formatter
                .debug_struct("Brokered")
                .field("label", label)
                .field("material", &"[REDACTED]")
                .finish(),
        }
    }
}

/// Owner of partial deliveries and their idempotent cleanup.
#[derive(Debug, Default)]
pub struct SecretDeliverySession {
    artifacts: Vec<DeliveryArtifact>,
    receipts: Vec<DeliveryReceipt>,
    leases: Vec<SecretLease>,
    released: bool,
}

impl SecretDeliverySession {
    /// Creates an empty delivery owner.
    #[must_use]
    pub const fn new() -> Self {
        Self { artifacts: Vec::new(), receipts: Vec::new(), leases: Vec::new(), released: false }
    }

    /// Consumes one lease use and stages its exact delivery.
    ///
    /// # Errors
    /// Rejects an inactive/mismatched lease or any staging failure. Existing artifacts remain
    /// owned and can be released after a partial failure.
    pub fn deliver(
        &mut self,
        mut lease: SecretLease,
        material: SecretMaterial,
        context: SecretDeliveryContext,
        staging_root: &Path,
    ) -> Result<&DeliveryReceipt, SecretError> {
        if self.released {
            return Err(delivery_error("delivery session is already released"));
        }
        let delivery = lease.delivery().clone();
        lease.consume(
            context.owner,
            context.environment,
            context.sandbox_digest,
            context.execution_digest,
            lease.reference(),
            &delivery,
            context.now_epoch_millis,
        )?;
        let artifact = match &delivery {
            SecretDelivery::Environment(name) => {
                DeliveryArtifact::Environment { name: name.clone(), material }
            }
            SecretDelivery::BrokeredHandle(label) => {
                DeliveryArtifact::Brokered { label: label.clone(), material }
            }
            SecretDelivery::File(sandbox_path) => {
                let staging_path = stage_file(staging_root, lease.id(), &material)?;
                DeliveryArtifact::File { sandbox_path: sandbox_path.clone(), staging_path }
            }
        };
        self.artifacts.push(artifact);
        self.receipts.push(DeliveryReceipt { lease_id: lease.id(), delivery, released: false });
        self.leases.push(lease);
        self.receipts.last().ok_or_else(|| delivery_error("delivery receipt was not retained"))
    }

    /// Returns staged artifacts for native helper handle/environment/mount projection.
    #[must_use]
    pub fn artifacts(&self) -> &[DeliveryArtifact] {
        &self.artifacts
    }
    /// Returns nonsensitive receipts.
    #[must_use]
    pub fn receipts(&self) -> &[DeliveryReceipt] {
        &self.receipts
    }
    /// Returns retained exact leases without material.
    #[must_use]
    pub fn leases(&self) -> &[SecretLease] {
        &self.leases
    }

    /// Removes private files and drops all zeroizing material idempotently.
    ///
    /// # Errors
    /// Returns a cleanup failure if any exact staging file cannot be removed.
    pub fn release(&mut self) -> Result<(), SecretError> {
        if self.released {
            return Ok(());
        }
        let mut retained = Vec::new();
        for artifact in std::mem::take(&mut self.artifacts) {
            if let DeliveryArtifact::File { staging_path, .. } = &artifact
                && fs::remove_file(staging_path).is_err()
                && staging_path.exists()
            {
                retained.push(artifact);
            }
        }
        for lease in &mut self.leases {
            lease.revoke();
        }
        if !retained.is_empty() {
            self.artifacts = retained;
            return Err(cleanup_error("one or more private secret files could not be removed"));
        }
        for receipt in &mut self.receipts {
            receipt.released = true;
        }
        self.released = true;
        Ok(())
    }
}

impl Drop for SecretDeliverySession {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

fn stage_file(
    root: &Path,
    lease_id: SecretLeaseId,
    material: &SecretMaterial,
) -> Result<PathBuf, SecretError> {
    fs::create_dir_all(root)
        .map_err(|_| delivery_error("secret staging root cannot be created"))?;
    let path = root.join(format!("{}.secret", hex(lease_id.as_bytes())));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file =
        options.open(&path).map_err(|_| delivery_error("private secret file cannot be created"))?;
    let write = material.expose(|bytes| file.write_all(bytes).and_then(|()| file.sync_all()));
    if write.is_err() {
        let _ = fs::remove_file(&path);
        return Err(delivery_error("private secret file cannot be synchronized"));
    }
    Ok(path)
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}

const fn delivery_error(detail: &'static str) -> SecretError {
    SecretError::new(
        SecretErrorKind::Delivery,
        SecretOperation::Deliver,
        RecoveryClass::RevokeAndClean,
        detail,
    )
}

const fn cleanup_error(detail: &'static str) -> SecretError {
    SecretError::new(
        SecretErrorKind::Cleanup,
        SecretOperation::Cleanup,
        RecoveryClass::RevokeAndClean,
        detail,
    )
}

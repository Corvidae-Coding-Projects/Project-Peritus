//! Exact ACL installation, retained backup ownership, and retryable reversal.

use core::fmt;
#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
use peritus_sandbox::{FileOperation, PathScope, RuleEffect};
use peritus_types::Sha256Digest;

#[cfg(target_os = "windows")]
use super::{AclAccess, AclEntry, AclPlan};
use crate::WindowsError;
#[cfg(target_os = "windows")]
use crate::{WindowsErrorKind, WindowsOperation, WindowsRecovery};

/// Owner of exact ACL backups and idempotent reversal.
pub struct AclTransaction {
    digest: Sha256Digest,
    reversals: Vec<AclReversal>,
    state: AclState,
    restore_failed: bool,
}

impl AclTransaction {
    pub(super) const fn planned(digest: Sha256Digest) -> Self {
        Self { digest, reversals: Vec::new(), state: AclState::Planned, restore_failed: false }
    }

    #[cfg(target_os = "windows")]
    pub(super) fn install(plan: &AclPlan, backup_root: &Path) -> Result<Self, WindowsError> {
        std::fs::create_dir_all(backup_root).map_err(|_| {
            acl_error(WindowsOperation::InstallAcl, "ACL backup root cannot be created")
        })?;
        let mut transaction = Self::planned(plan.digest);
        transaction.state = AclState::Applied;
        for (index, entry) in plan.entries.iter().enumerate() {
            let native = entry.path.to_path_buf();
            if !native.exists() {
                let _ = transaction.restore();
                return Err(acl_error(
                    WindowsOperation::InstallAcl,
                    "exact ACL target does not exist",
                ));
            }
            let backup = backup_root.join(format!("{}-{index}.acl", hex(plan.digest.as_bytes())));
            if let Err(error) = save_acl(&native, &backup) {
                let _ = transaction.restore();
                return Err(error);
            }
            let parent = native.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
            transaction.reversals.push(AclReversal { parent, backup });
            if let Err(error) = apply_entry(&native, &plan.principal_sid, entry) {
                let _ = transaction.restore();
                return Err(error);
            }
        }
        Ok(transaction)
    }

    /// Returns the bound plan digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Reports whether every installed change was restored.
    #[must_use]
    pub const fn restored(&self) -> bool {
        matches!(self.state, AclState::Planned | AclState::Restored)
    }

    /// Returns explicit progress retained across failed restore attempts.
    #[must_use]
    pub const fn cleanup_state(&self) -> crate::CleanupState {
        if self.restored() {
            crate::CleanupState::Complete
        } else if self.restore_failed {
            crate::CleanupState::RetryRequired
        } else {
            crate::CleanupState::Pending
        }
    }

    /// Returns the exact backup records still requiring reversal.
    #[must_use]
    pub const fn pending_reversal_count(&self) -> usize {
        self.reversals.len()
    }

    /// Restores all exact saved ACLs in reverse order.
    ///
    /// # Errors
    /// Returns a typed cleanup failure if any restore remains incomplete.
    pub fn restore(&mut self) -> Result<(), WindowsError> {
        if self.restored() {
            return Ok(());
        }
        #[cfg(target_os = "windows")]
        {
            let mut failed = Vec::new();
            for reversal in core::mem::take(&mut self.reversals).into_iter().rev() {
                if restore_acl(&reversal).is_err() {
                    failed.push(reversal);
                }
            }
            if !failed.is_empty() {
                failed.reverse();
                self.reversals = failed;
                self.restore_failed = true;
                return Err(acl_error(
                    WindowsOperation::RestoreAcl,
                    "one or more exact ACL backups could not be restored",
                ));
            }
        }
        self.reversals.clear();
        self.state = AclState::Restored;
        self.restore_failed = false;
        Ok(())
    }
}

impl fmt::Debug for AclTransaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AclTransaction")
            .field("digest", &self.digest)
            .field("reversal_count", &self.reversals.len())
            .field("state", &self.state)
            .field("restore_failed", &self.restore_failed)
            .finish()
    }
}

impl Drop for AclTransaction {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AclState {
    Planned,
    #[cfg(target_os = "windows")]
    Applied,
    Restored,
}

#[cfg(target_os = "windows")]
struct AclReversal {
    parent: PathBuf,
    backup: PathBuf,
}

#[cfg(not(target_os = "windows"))]
type AclReversal = ();

#[cfg(target_os = "windows")]
fn save_acl(target: &Path, backup: &Path) -> Result<(), WindowsError> {
    let status = std::process::Command::new("icacls.exe")
        .arg(target)
        .args(["/save", backup.to_string_lossy().as_ref(), "/q"])
        .status()
        .map_err(|_| acl_error(WindowsOperation::InstallAcl, "icacls ACL save could not start"))?;
    if status.success() {
        Ok(())
    } else {
        Err(acl_error(WindowsOperation::InstallAcl, "icacls ACL save failed"))
    }
}

#[cfg(target_os = "windows")]
fn apply_entry(target: &Path, principal: &str, entry: &AclEntry) -> Result<(), WindowsError> {
    let switch = if entry.effect == RuleEffect::Allow { "/grant:r" } else { "/deny" };
    let inheritance = if entry.scope == PathScope::Descendants { "(OI)(CI)" } else { "" };
    let grant = format!("{principal}:{inheritance}{}", rights(entry.access));
    let status = std::process::Command::new("icacls.exe")
        .arg(target)
        .args([switch, &grant, "/q"])
        .status()
        .map_err(|_| acl_error(WindowsOperation::InstallAcl, "icacls mutation could not start"))?;
    if status.success() {
        Ok(())
    } else {
        Err(acl_error(WindowsOperation::InstallAcl, "icacls exact mutation failed"))
    }
}

#[cfg(target_os = "windows")]
fn restore_acl(reversal: &AclReversal) -> Result<(), WindowsError> {
    let status = std::process::Command::new("icacls.exe")
        .arg(&reversal.parent)
        .args(["/restore", reversal.backup.to_string_lossy().as_ref(), "/q"])
        .status()
        .map_err(|_| acl_error(WindowsOperation::RestoreAcl, "icacls restore could not start"))?;
    if status.success() {
        let _ = std::fs::remove_file(&reversal.backup);
        Ok(())
    } else {
        Err(acl_error(WindowsOperation::RestoreAcl, "icacls exact restore failed"))
    }
}

#[cfg(target_os = "windows")]
fn rights(access: AclAccess) -> String {
    let mut values = Vec::new();
    for (operation, text) in [
        (FileOperation::Discover, "RD"),
        (FileOperation::Metadata, "RA"),
        (FileOperation::Read, "REA"),
        (FileOperation::Execute, "X"),
        (FileOperation::Create, "AD"),
        (FileOperation::Write, "WD"),
        (FileOperation::Remove, "DE"),
    ] {
        if access.contains(operation) {
            values.push(text);
        }
    }
    format!("({})", values.join(","))
}

#[cfg(target_os = "windows")]
fn acl_error(operation: WindowsOperation, detail: &'static str) -> WindowsError {
    WindowsError::new(WindowsErrorKind::Acl, operation, WindowsRecovery::RetryCleanup, detail)
}

#[cfg(target_os = "windows")]
fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

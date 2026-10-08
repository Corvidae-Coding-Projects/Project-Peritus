//! Exact ACL installation, retained backup ownership, and retryable reversal.

use core::fmt;
#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
use peritus_sandbox::{FileOperation, PathScope, RuleEffect};
use peritus_types::Sha256Digest;

#[cfg(target_os = "windows")]
use super::{AclAccess, AclEntry, AclPlan};
#[cfg(target_os = "windows")]
use crate::ResolvedWindowsPath;
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
        let mut index = 0;
        while index < plan.entries.len() {
            let entry = &plan.entries[index];
            let mut end = index + 1;
            while end < plan.entries.len() && plan.entries[end].path == entry.path {
                end += 1;
            }
            let group = &plan.entries[index..end];
            let (native, created) = match prepare_target(group) {
                Ok(prepared) => prepared,
                Err(error) => return Err(rollback_install(&mut transaction, error)),
            };
            let backup = backup_root.join(format!("{}-{index}.acl", hex(plan.digest.as_bytes())));
            if let Err(error) = save_acl(&native, &backup) {
                if created {
                    let _ = std::fs::remove_dir(&native);
                }
                return Err(rollback_install(&mut transaction, error));
            }
            let parent = native.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
            transaction.reversals.push(AclReversal {
                parent,
                target: native.clone(),
                backup: Some(backup),
                remove_created: created,
            });
            let mut replaced_grants = false;
            for entry in group {
                let replace_grants = entry.effect == RuleEffect::Allow && !replaced_grants;
                if entry.effect == RuleEffect::Allow {
                    replaced_grants = true;
                }
                if let Err(error) = apply_entry(
                    &native,
                    &plan.principal_sid,
                    entry,
                    replace_grants,
                ) {
                    return Err(rollback_install(&mut transaction, error));
                }
            }
            index = end;
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
            for mut reversal in core::mem::take(&mut self.reversals).into_iter().rev() {
                if restore_acl(&mut reversal).is_err() {
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
    target: PathBuf,
    backup: Option<PathBuf>,
    remove_created: bool,
}

#[cfg(not(target_os = "windows"))]
type AclReversal = ();

#[cfg(target_os = "windows")]
fn rollback_install(transaction: &mut AclTransaction, original: WindowsError) -> WindowsError {
    let incomplete = transaction.restore().is_err() || !transaction.restored();
    original.with_cleanup(crate::PreparationCleanup::new(
        incomplete,
        false,
        false,
        false,
    ))
}

#[cfg(target_os = "windows")]
fn save_acl(target: &Path, backup: &Path) -> Result<(), WindowsError> {
    let status = icacls_command(WindowsOperation::InstallAcl)?
        .arg(target)
        .arg("/save")
        .arg(backup)
        .arg("/q")
        .status()
        .map_err(|_| acl_error(WindowsOperation::InstallAcl, "icacls ACL save could not start"))?;
    if status.success() {
        Ok(())
    } else {
        Err(acl_error(WindowsOperation::InstallAcl, "icacls ACL save failed"))
    }
}

#[cfg(target_os = "windows")]
fn prepare_target(entries: &[AclEntry]) -> Result<(PathBuf, bool), WindowsError> {
    let entry = entries.first().ok_or_else(|| {
        acl_error(WindowsOperation::InstallAcl, "ACL target group is empty")
    })?;
    if entries.iter().any(|candidate| {
        candidate.path != entry.path || candidate.authority_root != entry.authority_root
    }) {
        return Err(acl_error(
            WindowsOperation::InstallAcl,
            "ACL target group has inconsistent native authority",
        ));
    }
    let authority = ResolvedWindowsPath::resolve(entry.authority_root.clone())?;
    let (anchor, exists) = ResolvedWindowsPath::resolve_existing_or_parent(entry.path.clone())?;
    if authority.evidence().volume_serial() != anchor.evidence().volume_serial() {
        return Err(acl_error(
            WindowsOperation::InstallAcl,
            "ACL target differs from its authorized root volume",
        ));
    }
    let native = entry.path.to_path_buf();
    if exists {
        return Ok((native, false));
    }
    if !entries.iter().any(AclEntry::creates_deny_directory) {
        return Err(acl_error(
            WindowsOperation::InstallAcl,
            "absent ACL target has no checked deny-anchor creation authority",
        ));
    }
    let created = match std::fs::create_dir(&native) {
        Ok(()) => true,
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(_) => {
            return Err(acl_error(
                WindowsOperation::InstallAcl,
                "temporary ACL deny directory cannot be created",
            ));
        }
    };
    let resolved = match ResolvedWindowsPath::resolve(entry.path.clone()) {
        Ok(resolved) => resolved,
        Err(error) => {
            if created {
                let _ = std::fs::remove_dir(&native);
            }
            return Err(error);
        }
    };
    if authority.evidence().volume_serial() != resolved.evidence().volume_serial() {
        if created {
            let _ = std::fs::remove_dir(&native);
        }
        return Err(acl_error(
            WindowsOperation::InstallAcl,
            "created ACL target differs from its authorized root volume",
        ));
    }
    Ok((native, created))
}

#[cfg(target_os = "windows")]
fn apply_entry(
    target: &Path,
    principal: &str,
    entry: &AclEntry,
    replace_grants: bool,
) -> Result<(), WindowsError> {
    let switch = match (entry.effect, replace_grants) {
        (RuleEffect::Allow, true) => "/grant:r",
        (RuleEffect::Allow, false) => "/grant",
        (RuleEffect::Deny, _) => "/deny",
    };
    let inheritance = if entry.scope == PathScope::Descendants { "(OI)(CI)" } else { "" };
    let grant = format!("{principal}:{inheritance}{}", rights(entry.access));
    let status = icacls_command(WindowsOperation::InstallAcl)?
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
fn restore_acl(reversal: &mut AclReversal) -> Result<(), WindowsError> {
    if let Some(backup) = reversal.backup.as_ref() {
        let status = icacls_command(WindowsOperation::RestoreAcl)?
            .arg(&reversal.parent)
            .arg("/restore")
            .arg(backup)
            .arg("/q")
            .status()
            .map_err(|_| {
                acl_error(WindowsOperation::RestoreAcl, "icacls restore could not start")
            })?;
        if !status.success() {
            return Err(acl_error(WindowsOperation::RestoreAcl, "icacls exact restore failed"));
        }
        let _ = std::fs::remove_file(backup);
        reversal.backup = None;
    }
    if reversal.remove_created {
        match std::fs::remove_dir(&reversal.target) {
            Ok(()) => reversal.remove_created = false,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                reversal.remove_created = false;
            }
            Err(_) => {
                return Err(acl_error(
                    WindowsOperation::RestoreAcl,
                    "temporary ACL deny directory cannot be removed",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn icacls_command(operation: WindowsOperation) -> Result<std::process::Command, WindowsError> {
    crate::native::probe::system_acl_tool()
        .map(|path| std::process::Command::new(path))
        .ok_or_else(|| acl_error(operation, "system ACL tool is unavailable"))
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

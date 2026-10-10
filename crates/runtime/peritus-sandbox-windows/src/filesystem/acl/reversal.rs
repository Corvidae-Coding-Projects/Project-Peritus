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

#[cfg(target_os = "windows")]
mod snapshots;
#[cfg(all(test, target_os = "windows"))]
mod tests;

/// Owner of exact ACL backups and idempotent reversal.
pub struct AclTransaction {
    digest: Sha256Digest,
    reversals: Vec<AclReversal>,
    state: AclState,
    restore_failed: bool,
    #[cfg(target_os = "windows")]
    backup_directory: Option<PathBuf>,
    #[cfg(target_os = "windows")]
    originals: snapshots::Snapshots,
    #[cfg(target_os = "windows")]
    mutated: bool,
}

impl AclTransaction {
    pub(crate) const fn planned(digest: Sha256Digest) -> Self {
        Self {
            digest,
            reversals: Vec::new(),
            state: AclState::Planned,
            restore_failed: false,
            #[cfg(target_os = "windows")]
            backup_directory: None,
            #[cfg(target_os = "windows")]
            originals: snapshots::Snapshots::new(),
            #[cfg(target_os = "windows")]
            mutated: false,
        }
    }

    #[cfg(target_os = "windows")]
    pub(super) fn install(plan: &AclPlan, backup_root: &Path) -> Result<Self, WindowsError> {
        let mut transaction = Self::planned(plan.digest);
        transaction.originals.configure(plan);
        transaction.originals.reserve(plan).map_err(|error| error.during_acl_install(false))?;
        std::fs::create_dir_all(backup_root).map_err(|_| {
            acl_error(WindowsOperation::InstallAcl, "ACL backup root cannot be created")
        })?;
        transaction.state = AclState::Applied;
        let private_backup = reserve_backup_directory(backup_root)?;
        transaction.backup_directory = Some(private_backup.clone());
        let mut index = 0;
        let mut groups = Vec::new();
        while index < plan.entries.len() {
            let entry = &plan.entries[index];
            let mut end = index + 1;
            while end < plan.entries.len() && plan.entries[end].path == entry.path {
                end += 1;
            }
            let group = &plan.entries[index..end];
            let (native, created_handle) = match prepare_target(group) {
                Ok(prepared) => prepared,
                Err(error) => return Err(rollback_install(&mut transaction, error)),
            };
            let backup = private_backup.join(format!("{index}.acl"));
            transaction.reversals.push(AclReversal {
                backup: None,
                discard_backup: Some(backup.clone()),
                created_handle,
                original: None,
                inheritance_restored: false,
                mutation_started: false,
            });
            let original = match transaction.originals.capture_target(
                &native,
                transaction.reversals.last().and_then(|entry| entry.created_handle.as_ref()),
            ) {
                Ok(original) => original,
                Err(error) => return Err(rollback_install(&mut transaction, error)),
            };
            transaction.reversals.last_mut().expect("new reversal").original = Some(original);
            groups.push((index, end, native, transaction.reversals.len() - 1));
            index = end;
        }
        // Snapshot the complete affected existing trees before the first inheritable mutation.
        // Overlapping plan targets share one pristine descriptor and exact object handle.
        if let Err(error) = transaction.originals.capture_descendants(&private_backup) {
            return Err(rollback_install(&mut transaction, error));
        }
        let stable = match transaction.originals.stabilize_all() {
            Ok(stable) => stable,
            Err(error) => return Err(rollback_install(&mut transaction, error)),
        };
        for (_, _, native, reversal) in &groups {
            let backup = transaction.reversals[*reversal]
                .discard_backup
                .as_ref()
                .expect("new reversal backup")
                .clone();
            if let Err(error) = save_acl(native, &backup) {
                drop(stable);
                return Err(rollback_install(&mut transaction, error));
            }
            transaction.reversals[*reversal].discard_backup = None;
            transaction.reversals[*reversal].backup = Some(backup);
        }
        for (start, end, native, reversal) in groups {
            transaction.mutated = true;
            transaction.reversals[reversal].mutation_started = true;
            let mut has_allow_entry = false;
            let group = &plan.entries[start..end];
            for entry in group {
                let replace_grants = entry.effect == RuleEffect::Allow && !has_allow_entry;
                if entry.effect == RuleEffect::Allow {
                    has_allow_entry = true;
                }
                if let Err(error) = apply_entry(&native, &plan.principal_sid, entry, replace_grants)
                {
                    drop(stable);
                    return Err(rollback_install(&mut transaction, error));
                }
            }
        }
        drop(stable);
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
            let mut failure = None;
            for reversal in self.reversals.iter_mut().rev() {
                record_failure(&mut failure, restore_acl(reversal, &self.originals));
            }
            // A later inheritance retry could change earlier exact restorations. Keep every
            // original and backup until both phases finish and verify as one transaction.
            if self.mutated {
                record_failure(&mut failure, self.originals.restore_descendant_inheritance());
                record_failure(&mut failure, self.originals.restore_exact());
                record_failure(&mut failure, self.originals.verify_exact());
                record_failure(&mut failure, self.originals.verify_new_descendants());
            }
            if let Some(error) = failure {
                self.restore_failed = true;
                return Err(error);
            }
            for reversal in &mut self.reversals {
                if let Some(backup) = reversal.backup.as_ref() {
                    if let Err(error) = remove_backup(backup) {
                        self.restore_failed = true;
                        return Err(error);
                    }
                    reversal.backup = None;
                }
            }
        }
        #[cfg(target_os = "windows")]
        if let Some(directory) = self.backup_directory.as_ref() {
            if let Err(source) = std::fs::remove_dir(directory)
                && source.kind() != std::io::ErrorKind::NotFound
            {
                self.restore_failed = true;
                return Err(acl_error(
                    WindowsOperation::RestoreAcl,
                    "owned ACL backup directory could not be removed",
                ));
            }
            self.backup_directory = None;
        }
        self.reversals.clear();
        #[cfg(target_os = "windows")]
        self.originals.clear();
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
            .finish_non_exhaustive()
    }
}

impl Drop for AclTransaction {
    fn drop(&mut self) {
        let failed = self.restore().is_err() || !self.restored();
        #[cfg(target_os = "windows")]
        if failed {
            self.originals.quarantine_process_lifetime();
        }
        #[cfg(not(target_os = "windows"))]
        let _ = failed;
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
    backup: Option<PathBuf>,
    discard_backup: Option<PathBuf>,
    created_handle: Option<std::fs::File>,
    original: Option<usize>,
    inheritance_restored: bool,
    mutation_started: bool,
}

#[cfg(not(target_os = "windows"))]
type AclReversal = ();

#[cfg(target_os = "windows")]
fn rollback_install(transaction: &mut AclTransaction, original: WindowsError) -> WindowsError {
    let incomplete = transaction.restore().is_err() || !transaction.restored();
    let original = original
        .during_acl_install(incomplete)
        .with_cleanup(crate::PreparationCleanup::new([incomplete, false, false, false]));
    if incomplete {
        let digest = transaction.digest();
        let owned = core::mem::replace(transaction, AclTransaction::planned(digest));
        original.retain_cleanup(crate::error::CleanupOwner::Acl(Box::new(owned)))
    } else {
        original
    }
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
fn prepare_target(entries: &[AclEntry]) -> Result<(PathBuf, Option<std::fs::File>), WindowsError> {
    let entry = entries
        .first()
        .ok_or_else(|| acl_error(WindowsOperation::InstallAcl, "ACL target group is empty"))?;
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
        return Ok((native, None));
    }
    if !entries.iter().any(AclEntry::creates_deny_directory) {
        return Err(acl_error(
            WindowsOperation::InstallAcl,
            "absent ACL target has no checked deny-anchor creation authority",
        ));
    }
    let handle = crate::native::acl::create(&native)?;
    Ok((native, Some(handle)))
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
    let grant = format!("*{principal}:{inheritance}{}", rights(entry.access));
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
fn restore_acl(
    reversal: &mut AclReversal,
    originals: &snapshots::Snapshots,
) -> Result<(), WindowsError> {
    let mut failure = None;
    if let Some(partial) = reversal.discard_backup.as_ref() {
        if let Err(error) = remove_backup(partial) {
            record_failure(&mut failure, Err(error));
        } else {
            reversal.discard_backup = None;
        }
    }
    if !reversal.inheritance_restored {
        if let Some(original) = reversal.original
            && reversal.mutation_started
        {
            if let Err(error) = originals.get(original).restore_inheritance() {
                record_failure(&mut failure, Err(error));
            } else {
                reversal.inheritance_restored = true;
            }
        } else {
            reversal.inheritance_restored = true;
        }
    }
    if let Some(handle) = reversal.created_handle.as_ref() {
        if let Err(error) = crate::native::acl::remove_created_directory(handle) {
            record_failure(&mut failure, Err(error));
        } else {
            reversal.created_handle = None;
        }
    }
    failure.map_or(Ok(()), Err)
}

#[cfg(target_os = "windows")]
fn record_failure(first: &mut Option<WindowsError>, result: Result<(), WindowsError>) {
    if let Err(error) = result {
        first.get_or_insert(error);
    }
}

#[cfg(target_os = "windows")]
fn remove_backup(path: &Path) -> Result<(), WindowsError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(acl_error(
            WindowsOperation::RestoreAcl,
            "owned ACL backup file could not be removed",
        )),
    }
}

#[cfg(target_os = "windows")]
fn icacls_command(operation: WindowsOperation) -> Result<std::process::Command, WindowsError> {
    crate::native::probe::system_acl_tool()
        .map(std::process::Command::new)
        .ok_or_else(|| acl_error(operation, "system ACL tool is unavailable"))
}

#[cfg(target_os = "windows")]
fn rights(access: AclAccess) -> String {
    let mut values = Vec::new();
    for (operation, text) in [
        (FileOperation::Discover, "RD"),
        (FileOperation::Metadata, "RA"),
        (FileOperation::Read, "RD"),
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
    let recovery = if operation == WindowsOperation::InstallAcl {
        WindowsRecovery::CorrectRequest
    } else {
        WindowsRecovery::RetryCleanup
    };
    WindowsError::new(WindowsErrorKind::Acl, operation, recovery, detail)
}

#[cfg(target_os = "windows")]
fn reserve_backup_directory(root: &Path) -> Result<PathBuf, WindowsError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    loop {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = root.join(format!("transaction-{}-{sequence}", std::process::id()));
        match std::fs::create_dir(&directory) {
            Ok(()) => return Ok(directory),
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => {
                return Err(acl_error(
                    WindowsOperation::InstallAcl,
                    "private ACL backup directory cannot be reserved",
                ));
            }
        }
    }
}

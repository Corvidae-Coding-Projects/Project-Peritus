//! Durable ACL installation, retained ownership, and resumable exact reversal.

use core::fmt;

use peritus_types::Sha256Digest;

#[cfg(target_os = "windows")]
use std::{
    ffi::OsString,
    fs::{File, OpenOptions, TryLockError},
    io::Write as _,
    os::windows::ffi::{OsStrExt as _, OsStringExt as _},
    path::{Path, PathBuf},
};

#[cfg(target_os = "windows")]
use peritus_process::{ProcessId, RetainedOwnerBinding};
#[cfg(target_os = "windows")]
use peritus_sandbox::{FileOperation, PathScope, RuleEffect};

#[cfg(target_os = "windows")]
use super::{AclAccess, AclEntry, AclPlan};
#[cfg(target_os = "windows")]
use crate::{
    ResolvedWindowsPath, WindowsErrorKind, WindowsOperation, WindowsPath, WindowsRecovery,
    native::path::{NativePathIdentity, identity as native_path_identity},
};
use crate::WindowsError;

#[cfg(target_os = "windows")]
const JOURNAL_MAGIC: &[u8; 8] = b"PACLV001";
#[cfg(target_os = "windows")]
const JOURNAL_HEADER_BYTES: usize = 8 + 5 * Sha256Digest::LENGTH + 4 + Sha256Digest::LENGTH;
#[cfg(target_os = "windows")]
const MAX_EVENT_BYTES: usize = 512 * 1024;

/// Owner of one authorization-bound ACL transaction and its durable reversal obligations.
pub struct AclTransaction {
    digest: Sha256Digest,
    transaction_digest: Option<Sha256Digest>,
    receipt: Option<Sha256Digest>,
    owner_operation_digest: Option<Sha256Digest>,
    service_owner_digest: Option<Sha256Digest>,
    reversals: Vec<AclReversal>,
    state: AclState,
    restore_failed: bool,
    #[cfg(target_os = "windows")]
    journal: Option<Journal>,
    #[cfg(target_os = "windows")]
    _owner_lock: Option<File>,
}

impl AclTransaction {
    pub(super) const fn planned(digest: Sha256Digest) -> Self {
        Self {
            digest,
            transaction_digest: None,
            receipt: None,
            owner_operation_digest: None,
            service_owner_digest: None,
            reversals: Vec::new(),
            state: AclState::Planned,
            restore_failed: false,
            #[cfg(target_os = "windows")]
            journal: None,
            #[cfg(target_os = "windows")]
            _owner_lock: None,
        }
    }

    #[cfg(target_os = "windows")]
    pub(super) fn install(
        plan: &AclPlan,
        backup_root: &Path,
        process_id: ProcessId,
        preparation_digest: Sha256Digest,
        retained_owner: Option<RetainedOwnerBinding>,
        should_continue: &dyn Fn() -> bool,
    ) -> Result<Self, WindowsError> {
        if retained_owner.is_some_and(|owner| {
            owner.process_id() != process_id
                || owner.backend_preparation_digest() != preparation_digest
        }) {
            return Err(identity_error("ACL retained owner differs from authorized preparation"));
        }
        prepare_storage_root(backup_root)?;
        let owner_lock = acquire_owner_lock(backup_root, should_continue)?;
        let ranges = group_ranges(plan)?;
        let (owner_operation_digest, service_owner_digest) = retained_owner.map_or(
            (None, None),
            |owner| {
                (
                    Some(owner.operation_digest()),
                    Some(owner.service_owner().digest()),
                )
            },
        );
        let transaction_digest = transaction_digest(
            process_id,
            preparation_digest,
            plan.digest,
            owner_operation_digest,
            service_owner_digest,
        );
        let group_count = u32::try_from(ranges.len()).map_err(|_| {
            acl_error(WindowsOperation::InstallAcl, "ACL target count exceeds journal format")
        })?;
        let receipt = transaction_receipt(
            transaction_digest,
            plan.digest,
            group_count,
            owner_operation_digest,
            service_owner_digest,
        );
        let header = JournalHeader {
            transaction_digest,
            receipt,
            plan_digest: plan.digest,
            owner_operation_digest,
            service_owner_digest,
            group_count,
        };
        let transaction_root = backup_root.join("transactions-v1");
        prepare_storage_root(&transaction_root)?;
        sync_directory(backup_root)?;
        reject_other_incomplete_transactions(&transaction_root, transaction_digest)?;
        let directory = transaction_root.join(hex(transaction_digest.as_bytes()));
        let (journal, reversals, transaction_complete) = Journal::open(&directory, header)?;
        let mut transaction = Self {
            digest: plan.digest,
            transaction_digest: Some(transaction_digest),
            receipt: Some(receipt),
            owner_operation_digest,
            service_owner_digest,
            reversals,
            state: if transaction_complete { AclState::Restored } else { AclState::Applied },
            restore_failed: false,
            journal: Some(journal),
            _owner_lock: Some(owner_lock),
        };
        if transaction_complete {
            return Err(identity_error(
                "ACL authorization transaction was already completed and cannot be duplicated",
            ));
        }
        transaction.validate_recorded_plan(plan, &ranges)?;
        if transaction.reversals.iter().any(AclReversal::cleanup_started) {
            let original = identity_error(
                "ACL transaction had entered reversal before retained-owner restart",
            );
            let incomplete = transaction.restore_while(should_continue).is_err()
                || !transaction.restored();
            return Err(original.with_cleanup(crate::PreparationCleanup::new(
                incomplete,
                false,
                false,
                false,
            )));
        }
        if ranges.is_empty() {
            transaction.append_event(Event::TransactionComplete)?;
            transaction.state = AclState::Restored;
            transaction._owner_lock.take();
            return Ok(transaction);
        }
        for (group_index, &(start, end)) in ranges.iter().enumerate() {
            if let Err(error) = transaction.install_group(
                plan,
                group_index,
                start,
                end,
                should_continue,
            ) {
                return Err(rollback_install(&mut transaction, error, should_continue));
            }
        }
        Ok(transaction)
    }

    /// Returns the bound plan digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }

    /// Returns the unique authorization-bound durable ACL transaction identity.
    #[must_use]
    pub const fn transaction_digest(&self) -> Option<Sha256Digest> { self.transaction_digest }

    /// Returns the exact durable journal receipt for this transaction.
    #[must_use]
    pub const fn receipt(&self) -> Option<Sha256Digest> { self.receipt }

    /// Returns the retained owner operation bound to the journal, when applicable.
    #[must_use]
    pub const fn owner_operation_digest(&self) -> Option<Sha256Digest> {
        self.owner_operation_digest
    }

    /// Returns the retained service-owner generation bound to the journal, when applicable.
    #[must_use]
    pub const fn service_owner_digest(&self) -> Option<Sha256Digest> {
        self.service_owner_digest
    }

    /// Reports whether every installed change and backup was durably retired.
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

    /// Returns the exact live durable ACL reversal owner, including its retained lock.
    #[must_use]
    pub(crate) fn custody_identity(&self) -> Option<Sha256Digest> {
        if self.restored() {
            return None;
        }
        let transaction = self.transaction_digest?;
        let receipt = self.receipt?;
        let owner = self.owner_operation_digest?;
        let service = self.service_owner_digest?;
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::io::AsRawHandle as _;
            use windows_sys::Win32::Foundation::GetHandleInformation;

            if self.journal.is_none() {
                return None;
            }
            let lock = self._owner_lock.as_ref()?;
            let raw = lock.as_raw_handle().cast();
            let mut flags = 0_u32;
            // SAFETY: the retained File owns the handle and the query writes only `flags`.
            if unsafe { GetHandleInformation(raw, &raw mut flags) } == 0 {
                return None;
            }
            let mut bytes = Vec::from(b"PERITUS-WINDOWS-ACL-OWNER-V1\0".as_slice());
            bytes.extend_from_slice(self.digest.as_bytes());
            bytes.extend_from_slice(transaction.as_bytes());
            bytes.extend_from_slice(receipt.as_bytes());
            bytes.extend_from_slice(owner.as_bytes());
            bytes.extend_from_slice(service.as_bytes());
            bytes.extend_from_slice(&(raw as usize as u64).to_be_bytes());
            return Some(peritus_codec::sha256(&bytes));
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (transaction, receipt, owner, service);
            None
        }
    }

    /// Returns the exact durable obligations still requiring reversal or retirement.
    #[must_use]
    pub fn pending_reversal_count(&self) -> usize {
        #[cfg(target_os = "windows")]
        {
            return self.reversals.iter().filter(|reversal| !reversal.complete).count();
        }
        #[cfg(not(target_os = "windows"))]
        {
            0
        }
    }

    /// Restores all saved ACLs, explicitly retires their backups, and removes owned anchors.
    ///
    /// # Errors
    /// Returns a typed cleanup failure while leaving the journal available for an exact retry.
    pub fn restore(&mut self) -> Result<(), WindowsError> {
        #[cfg(target_os = "windows")]
        {
            return self.restore_while(&|| true);
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.reversals.clear();
            self.state = AclState::Restored;
            self.restore_failed = false;
            Ok(())
        }
    }

    #[cfg(target_os = "windows")]
    fn install_group(
        &mut self,
        plan: &AclPlan,
        group_index: usize,
        start: usize,
        end: usize,
        should_continue: &dyn Fn() -> bool,
    ) -> Result<(), WindowsError> {
        let group = plan.entries.get(start..end).ok_or_else(|| {
            identity_error("ACL plan group is outside its authenticated entry range")
        })?;
        if self.reversals.len() == group_index {
            let pending = inspect_target(group, group_index, start, end)?;
            self.append_event(Event::TargetPending(pending.clone()))?;
            let mut reversal = AclReversal::pending(pending);
            reversal.backup = self.journal_directory()?.join(format!("preimage-{group_index}.acl"));
            self.reversals.push(reversal);
        }
        self.ensure_target_ready(group_index)?;
        self.verify_group_identity(group_index)?;
        if self.reversals[group_index].applied {
            return Ok(());
        }
        if self.reversals[group_index].mutation_pending
            || self.reversals[group_index].reset_pending
        {
            self.reset_interrupted_group(group_index, should_continue)?;
        }
        if !self.reversals[group_index].preimage_saved {
            self.save_group_preimage(group_index, should_continue)?;
        }
        self.append_event(Event::MutationPending(group_index))?;
        self.reversals[group_index].mutation_pending = true;
        let target = self.reversals[group_index].target.to_path_buf();
        let mut replaced_grants = false;
        for entry in group {
            self.verify_group_identity(group_index)?;
            let replace_grants = entry.effect == RuleEffect::Allow && !replaced_grants;
            if entry.effect == RuleEffect::Allow { replaced_grants = true; }
            apply_entry(
                &target,
                &plan.principal_sid,
                entry,
                replace_grants,
                should_continue,
            )?;
        }
        self.verify_group_identity(group_index)?;
        self.append_event(Event::Applied(group_index))?;
        let reversal = &mut self.reversals[group_index];
        reversal.mutation_pending = false;
        reversal.applied = true;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn journal_directory(&self) -> Result<&Path, WindowsError> {
        self.journal
            .as_ref()
            .and_then(|journal| journal.path.parent())
            .ok_or_else(|| identity_error("ACL journal directory is unavailable"))
    }

    #[cfg(target_os = "windows")]
    fn validate_recorded_plan(
        &self,
        plan: &AclPlan,
        ranges: &[(usize, usize)],
    ) -> Result<(), WindowsError> {
        if self.reversals.len() > ranges.len() {
            return Err(identity_error("ACL journal contains more targets than its plan"));
        }
        for (group_index, reversal) in self.reversals.iter().enumerate() {
            let (start, end) = ranges[group_index];
            let entry = plan.entries.get(start).ok_or_else(|| {
                identity_error("ACL journal target has no corresponding plan entry")
            })?;
            if reversal.index != group_index
                || reversal.start != start
                || reversal.end != end
                || reversal.target != entry.path
                || reversal.authority != entry.authority_root
                || reversal.create_intent
                    && !plan.entries[start..end]
                        .iter()
                        .any(AclEntry::creates_deny_directory)
            {
                return Err(identity_error(
                    "ACL journal target differs from the authenticated plan",
                ));
            }
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn ensure_target_ready(&mut self, index: usize) -> Result<(), WindowsError> {
        if self.reversals[index].target_identity.is_some() { return Ok(()); }
        let reversal = &self.reversals[index];
        verify_identity(&reversal.authority.to_path_buf(), reversal.authority_identity)?;
        verify_identity(&reversal.anchor.to_path_buf(), reversal.anchor_identity)?;
        let target = reversal.target.to_path_buf();
        let (identity, created) = match reversal.expected_target_identity {
            Some(expected) => {
                verify_identity(&target, expected)?;
                (expected, false)
            }
            None => match std::fs::symlink_metadata(&target) {
                Ok(_) => {
                    return Err(identity_error(
                        "unconfirmed ACL deny anchor exists after interrupted creation",
                    ));
                }
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::create_dir(&target).map_err(|_| {
                        acl_error(
                            WindowsOperation::InstallAcl,
                            "temporary ACL deny directory cannot be created",
                        )
                    })?;
                    (native_path_identity(&target)?, true)
                }
                Err(_) => return Err(identity_error("ACL target cannot be inspected for resumption")),
            },
        };
        self.append_event(Event::TargetReady { index, identity, created })?;
        let reversal = &mut self.reversals[index];
        reversal.target_identity = Some(identity);
        reversal.created = created;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn verify_group_identity(&self, index: usize) -> Result<(), WindowsError> {
        let reversal = &self.reversals[index];
        verify_identity(&reversal.authority.to_path_buf(), reversal.authority_identity)?;
        verify_identity(&reversal.anchor.to_path_buf(), reversal.anchor_identity)?;
        let target_identity = reversal.target_identity.ok_or_else(|| {
            identity_error("ACL target identity was not durably recorded")
        })?;
        verify_identity(&reversal.target.to_path_buf(), target_identity)
    }

    #[cfg(target_os = "windows")]
    fn save_group_preimage(
        &mut self,
        index: usize,
        should_continue: &dyn Fn() -> bool,
    ) -> Result<(), WindowsError> {
        self.verify_group_identity(index)?;
        let backup = self.reversals[index].backup.clone();
        match std::fs::remove_file(&backup) {
            Ok(()) => sync_directory(backup.parent().unwrap_or_else(|| Path::new(".")))?,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(acl_error(
                WindowsOperation::InstallAcl,
                "stale ACL preimage cannot be retired before save",
            )),
        }
        if !self.reversals[index].preimage_save_pending {
            self.append_event(Event::PreimageSavePending(index))?;
            self.reversals[index].preimage_save_pending = true;
        }
        save_acl(
            &self.reversals[index].target.to_path_buf(),
            &backup,
            should_continue,
        )?;
        OpenOptions::new()
            .read(true)
            .open(&backup)
            .and_then(|file| file.sync_all())
            .map_err(|_| acl_error(
                WindowsOperation::InstallAcl,
                "ACL preimage cannot be synchronized before mutation",
            ))?;
        sync_directory(backup.parent().unwrap_or_else(|| Path::new(".")))?;
        self.append_event(Event::PreimageSaved(index))?;
        self.reversals[index].preimage_save_pending = false;
        self.reversals[index].preimage_saved = true;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn reset_interrupted_group(
        &mut self,
        index: usize,
        should_continue: &dyn Fn() -> bool,
    ) -> Result<(), WindowsError> {
        self.verify_group_identity(index)?;
        if !self.reversals[index].reset_pending {
            self.append_event(Event::ResetPending(index))?;
            self.reversals[index].reset_pending = true;
        }
        restore_preimage(&self.reversals[index], should_continue)?;
        self.verify_group_identity(index)?;
        self.append_event(Event::ResetReady(index))?;
        let reversal = &mut self.reversals[index];
        reversal.reset_pending = false;
        reversal.mutation_pending = false;
        reversal.applied = false;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn restore_while(&mut self, should_continue: &dyn Fn() -> bool) -> Result<(), WindowsError> {
        if self.restored() { return Ok(()); }
        for index in (0..self.reversals.len()).rev() {
            if let Err(error) = self.restore_one(index, should_continue) {
                self.restore_failed = true;
                return Err(error);
            }
        }
        self.append_event(Event::TransactionComplete)?;
        self.state = AclState::Restored;
        self.restore_failed = false;
        self._owner_lock.take();
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn restore_one(
        &mut self,
        index: usize,
        should_continue: &dyn Fn() -> bool,
    ) -> Result<(), WindowsError> {
        if self.reversals[index].complete { return Ok(()); }
        if self.reversals[index].target_identity.is_none() {
            let reversal = &self.reversals[index];
            verify_identity(&reversal.authority.to_path_buf(), reversal.authority_identity)?;
            verify_identity(&reversal.anchor.to_path_buf(), reversal.anchor_identity)?;
            if let Some(expected) = reversal.expected_target_identity {
                verify_identity(&reversal.target.to_path_buf(), expected)?;
            } else {
                match std::fs::symlink_metadata(reversal.target.to_path_buf()) {
                    Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                    Ok(_) => return Err(identity_error(
                        "unconfirmed ACL deny anchor cannot be removed during recovery",
                    )),
                    Err(_) => return Err(identity_error(
                        "unconfirmed ACL target cannot be inspected during recovery",
                    )),
                }
            }
            self.append_event(Event::ObligationComplete(index))?;
            self.reversals[index].complete = true;
            return Ok(());
        }
        self.verify_group_identity(index)?;
        let needs_restore = self.reversals[index].preimage_saved
            && (self.reversals[index].mutation_pending
                || self.reversals[index].applied
                || self.reversals[index].reset_pending
                || self.reversals[index].restore_pending);
        if needs_restore && !self.reversals[index].restored {
            if !self.reversals[index].restore_pending {
                self.append_event(Event::RestorePending(index))?;
                self.reversals[index].restore_pending = true;
            }
            restore_preimage(&self.reversals[index], should_continue)?;
            self.verify_group_identity(index)?;
            self.append_event(Event::Restored(index))?;
            let reversal = &mut self.reversals[index];
            reversal.restored = true;
            reversal.restore_pending = false;
            reversal.mutation_pending = false;
            reversal.reset_pending = false;
            reversal.applied = false;
        }
        if (self.reversals[index].preimage_save_pending
            || self.reversals[index].preimage_saved)
            && !self.reversals[index].backup_deleted
        {
            if !self.reversals[index].backup_delete_pending {
                self.append_event(Event::BackupDeletePending(index))?;
                self.reversals[index].backup_delete_pending = true;
            }
            let backup = self.reversals[index].backup.clone();
            match std::fs::remove_file(&backup) {
                Ok(()) => {}
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(acl_error(
                    WindowsOperation::RestoreAcl,
                    "restored ACL preimage cannot be deleted",
                )),
            }
            sync_directory(backup.parent().unwrap_or_else(|| Path::new(".")))?;
            self.append_event(Event::BackupDeleted(index))?;
            self.reversals[index].backup_deleted = true;
            self.reversals[index].backup_delete_pending = false;
        }
        if self.reversals[index].created && !self.reversals[index].anchor_deleted {
            if !self.reversals[index].anchor_delete_pending {
                self.verify_group_identity(index)?;
                self.append_event(Event::AnchorDeletePending(index))?;
                self.reversals[index].anchor_delete_pending = true;
            }
            let target = self.reversals[index].target.to_path_buf();
            match std::fs::symlink_metadata(&target) {
                Ok(_) => {
                    verify_identity(
                        &target,
                        self.reversals[index]
                            .target_identity
                            .expect("created target identity is recorded"),
                    )?;
                    std::fs::remove_dir(&target).map_err(|_| acl_error(
                        WindowsOperation::RestoreAcl,
                        "temporary ACL deny directory cannot be removed",
                    ))?;
                }
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(identity_error(
                    "temporary ACL deny directory cannot be inspected",
                )),
            }
            sync_directory(target.parent().unwrap_or_else(|| Path::new(".")))?;
            self.append_event(Event::AnchorDeleted(index))?;
            self.reversals[index].anchor_deleted = true;
            self.reversals[index].anchor_delete_pending = false;
        }
        self.append_event(Event::ObligationComplete(index))?;
        self.reversals[index].complete = true;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn append_event(&mut self, event: Event) -> Result<(), WindowsError> {
        self.journal
            .as_mut()
            .ok_or_else(|| identity_error("ACL journal ownership is unavailable"))?
            .append(event)
    }
}

impl fmt::Debug for AclTransaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AclTransaction")
            .field("digest", &self.digest)
            .field("transaction_digest", &self.transaction_digest)
            .field("receipt", &self.receipt)
            .field("reversal_count", &self.pending_reversal_count())
            .field("state", &self.state)
            .field("restore_failed", &self.restore_failed)
            .finish_non_exhaustive()
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
#[derive(Clone, Debug)]
struct TargetPending {
    index: usize,
    start: usize,
    end: usize,
    create_intent: bool,
    target: WindowsPath,
    authority: WindowsPath,
    authority_identity: NativePathIdentity,
    anchor: WindowsPath,
    anchor_identity: NativePathIdentity,
    expected_target_identity: Option<NativePathIdentity>,
}

#[cfg(target_os = "windows")]
struct AclReversal {
    index: usize,
    start: usize,
    end: usize,
    create_intent: bool,
    target: WindowsPath,
    authority: WindowsPath,
    authority_identity: NativePathIdentity,
    anchor: WindowsPath,
    anchor_identity: NativePathIdentity,
    expected_target_identity: Option<NativePathIdentity>,
    target_identity: Option<NativePathIdentity>,
    backup: PathBuf,
    created: bool,
    preimage_save_pending: bool,
    preimage_saved: bool,
    mutation_pending: bool,
    applied: bool,
    reset_pending: bool,
    restore_pending: bool,
    restored: bool,
    backup_delete_pending: bool,
    backup_deleted: bool,
    anchor_delete_pending: bool,
    anchor_deleted: bool,
    complete: bool,
}

#[cfg(target_os = "windows")]
impl AclReversal {
    fn pending(value: TargetPending) -> Self {
        Self {
            index: value.index,
            start: value.start,
            end: value.end,
            create_intent: value.create_intent,
            target: value.target,
            authority: value.authority,
            authority_identity: value.authority_identity,
            anchor: value.anchor,
            anchor_identity: value.anchor_identity,
            expected_target_identity: value.expected_target_identity,
            target_identity: None,
            backup: PathBuf::new(),
            created: false,
            preimage_save_pending: false,
            preimage_saved: false,
            mutation_pending: false,
            applied: false,
            reset_pending: false,
            restore_pending: false,
            restored: false,
            backup_delete_pending: false,
            backup_deleted: false,
            anchor_delete_pending: false,
            anchor_deleted: false,
            complete: false,
        }
    }

    const fn cleanup_started(&self) -> bool {
        self.restore_pending
            || self.restored
            || self.backup_delete_pending
            || self.backup_deleted
            || self.anchor_delete_pending
            || self.anchor_deleted
            || self.complete
    }
}

#[cfg(not(target_os = "windows"))]
type AclReversal = ();

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct JournalHeader {
    transaction_digest: Sha256Digest,
    receipt: Sha256Digest,
    plan_digest: Sha256Digest,
    owner_operation_digest: Option<Sha256Digest>,
    service_owner_digest: Option<Sha256Digest>,
    group_count: u32,
}

#[cfg(target_os = "windows")]
struct Journal {
    file: File,
    path: PathBuf,
}

#[cfg(target_os = "windows")]
impl Journal {
    fn open(
        directory: &Path,
        expected: JournalHeader,
    ) -> Result<(Self, Vec<AclReversal>, bool), WindowsError> {
        match std::fs::create_dir(directory) {
            Ok(()) => sync_directory(directory.parent().unwrap_or_else(|| Path::new(".")))?,
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = std::fs::symlink_metadata(directory).map_err(|_| {
                    identity_error("ACL transaction directory cannot be inspected")
                })?;
                if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                    return Err(identity_error("ACL transaction path is not a real directory"));
                }
            }
            Err(_) => return Err(acl_error(
                WindowsOperation::InstallAcl,
                "ACL transaction directory cannot be created",
            )),
        }
        let path = directory.join("journal-v1.bin");
        let (mut file, parsed) = match OpenOptions::new()
            .read(true)
            .append(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let bytes = encode_header(expected);
                file.write_all(&bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(|_| acl_error(
                        WindowsOperation::InstallAcl,
                        "ACL transaction header cannot be synchronized",
                    ))?;
                sync_directory(directory)?;
                (file, ParsedJournal {
                    header: expected,
                    reversals: Vec::new(),
                    complete: false,
                    valid_bytes: bytes.len(),
                })
            }
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
                let parsed = parse_journal(&path)?;
                let file = OpenOptions::new()
                    .read(true)
                    .append(true)
                    .open(&path)
                    .map_err(|_| identity_error("ACL transaction journal cannot be reopened"))?;
                (file, parsed)
            }
            Err(_) => return Err(acl_error(
                WindowsOperation::InstallAcl,
                "ACL transaction journal cannot be created",
            )),
        };
        if parsed.header != expected {
            return Err(identity_error(
                "ACL transaction journal differs from the retained authorization",
            ));
        }
        file.set_len(u64::try_from(parsed.valid_bytes).map_err(|_| {
            identity_error("ACL transaction journal length is not representable")
        })?)
        .and_then(|()| file.sync_all())
        .map_err(|_| identity_error("ACL transaction journal tail cannot be repaired"))?;
        let mut reversals = parsed.reversals;
        for reversal in &mut reversals {
            reversal.backup = directory.join(format!("preimage-{}.acl", reversal.index));
        }
        Ok((Self { file, path }, reversals, parsed.complete))
    }

    fn append(&mut self, event: Event) -> Result<(), WindowsError> {
        let payload = event.encode()?;
        let length = u32::try_from(payload.len()).map_err(|_| {
            acl_error(WindowsOperation::InstallAcl, "ACL journal event is too large")
        })?;
        let checksum = peritus_codec::sha256(&payload);
        self.file
            .write_all(&length.to_be_bytes())
            .and_then(|()| self.file.write_all(&payload))
            .and_then(|()| self.file.write_all(checksum.as_bytes()))
            .and_then(|()| self.file.sync_all())
            .map_err(|_| acl_error(
                WindowsOperation::InstallAcl,
                "ACL reversal obligation cannot be durably appended",
            ))
    }
}

#[cfg(target_os = "windows")]
struct ParsedJournal {
    header: JournalHeader,
    reversals: Vec<AclReversal>,
    complete: bool,
    valid_bytes: usize,
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug)]
enum Event {
    TargetPending(TargetPending),
    TargetReady { index: usize, identity: NativePathIdentity, created: bool },
    PreimageSaved(usize),
    PreimageSavePending(usize),
    MutationPending(usize),
    Applied(usize),
    ResetPending(usize),
    ResetReady(usize),
    RestorePending(usize),
    Restored(usize),
    BackupDeletePending(usize),
    BackupDeleted(usize),
    AnchorDeletePending(usize),
    AnchorDeleted(usize),
    ObligationComplete(usize),
    TransactionComplete,
}

#[cfg(target_os = "windows")]
impl Event {
    fn encode(self) -> Result<Vec<u8>, WindowsError> {
        let mut bytes = Vec::new();
        match self {
            Self::TargetPending(value) => {
                bytes.push(1);
                push_index(&mut bytes, value.index)?;
                push_index(&mut bytes, value.start)?;
                push_index(&mut bytes, value.end)?;
                bytes.push(u8::from(value.create_intent));
                push_path(&mut bytes, &value.target)?;
                push_path(&mut bytes, &value.authority)?;
                push_identity(&mut bytes, value.authority_identity);
                push_path(&mut bytes, &value.anchor)?;
                push_identity(&mut bytes, value.anchor_identity);
                bytes.push(u8::from(value.expected_target_identity.is_some()));
                if let Some(identity) = value.expected_target_identity {
                    push_identity(&mut bytes, identity);
                }
            }
            Self::TargetReady { index, identity, created } => {
                bytes.push(2);
                push_index(&mut bytes, index)?;
                push_identity(&mut bytes, identity);
                bytes.push(u8::from(created));
            }
            Self::PreimageSaved(index) => push_simple(&mut bytes, 3, index)?,
            Self::PreimageSavePending(index) => push_simple(&mut bytes, 16, index)?,
            Self::MutationPending(index) => push_simple(&mut bytes, 4, index)?,
            Self::Applied(index) => push_simple(&mut bytes, 5, index)?,
            Self::ResetPending(index) => push_simple(&mut bytes, 6, index)?,
            Self::ResetReady(index) => push_simple(&mut bytes, 7, index)?,
            Self::RestorePending(index) => push_simple(&mut bytes, 8, index)?,
            Self::Restored(index) => push_simple(&mut bytes, 9, index)?,
            Self::BackupDeletePending(index) => push_simple(&mut bytes, 10, index)?,
            Self::BackupDeleted(index) => push_simple(&mut bytes, 11, index)?,
            Self::AnchorDeletePending(index) => push_simple(&mut bytes, 12, index)?,
            Self::AnchorDeleted(index) => push_simple(&mut bytes, 13, index)?,
            Self::ObligationComplete(index) => push_simple(&mut bytes, 14, index)?,
            Self::TransactionComplete => bytes.push(15),
        }
        if bytes.len() > MAX_EVENT_BYTES {
            return Err(identity_error("ACL journal event exceeds its representation bound"));
        }
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, WindowsError> {
        let mut reader = EventReader::new(bytes);
        let event = match reader.u8()? {
            1 => {
                let index = reader.index()?;
                let start = reader.index()?;
                let end = reader.index()?;
                let create_intent = reader.boolean()?;
                let target = reader.path()?;
                let authority = reader.path()?;
                let authority_identity = reader.identity()?;
                let anchor = reader.path()?;
                let anchor_identity = reader.identity()?;
                let expected_target_identity = if reader.boolean()? {
                    Some(reader.identity()?)
                } else { None };
                Self::TargetPending(TargetPending {
                    index,
                    start,
                    end,
                    create_intent,
                    target,
                    authority,
                    authority_identity,
                    anchor,
                    anchor_identity,
                    expected_target_identity,
                })
            }
            2 => Self::TargetReady {
                index: reader.index()?,
                identity: reader.identity()?,
                created: reader.boolean()?,
            },
            3 => Self::PreimageSaved(reader.index()?),
            4 => Self::MutationPending(reader.index()?),
            5 => Self::Applied(reader.index()?),
            6 => Self::ResetPending(reader.index()?),
            7 => Self::ResetReady(reader.index()?),
            8 => Self::RestorePending(reader.index()?),
            9 => Self::Restored(reader.index()?),
            10 => Self::BackupDeletePending(reader.index()?),
            11 => Self::BackupDeleted(reader.index()?),
            12 => Self::AnchorDeletePending(reader.index()?),
            13 => Self::AnchorDeleted(reader.index()?),
            14 => Self::ObligationComplete(reader.index()?),
            15 => Self::TransactionComplete,
            16 => Self::PreimageSavePending(reader.index()?),
            _ => return Err(identity_error("ACL journal event kind is unknown")),
        };
        reader.finish()?;
        Ok(event)
    }
}

#[cfg(target_os = "windows")]
fn parse_journal(path: &Path) -> Result<ParsedJournal, WindowsError> {
    let bytes = std::fs::read(path)
        .map_err(|_| identity_error("ACL transaction journal cannot be read"))?;
    if bytes.len() < JOURNAL_HEADER_BYTES {
        return Err(identity_error("ACL transaction journal header is truncated"));
    }
    let header = decode_header(&bytes[..JOURNAL_HEADER_BYTES])?;
    let mut reversals = Vec::new();
    let mut complete = false;
    let mut offset = JOURNAL_HEADER_BYTES;
    let mut valid_bytes = offset;
    while offset < bytes.len() {
        let frame_start = offset;
        let Some(length_bytes) = bytes.get(offset..offset.saturating_add(4)) else { break; };
        let length = usize::try_from(u32::from_be_bytes(
            length_bytes.try_into().expect("checked four-byte length"),
        )).map_err(|_| identity_error("ACL journal event length is not representable"))?;
        if length == 0 || length > MAX_EVENT_BYTES {
            return Err(identity_error("ACL journal event length is invalid"));
        }
        offset += 4;
        let payload_end = offset.checked_add(length)
            .ok_or_else(|| identity_error("ACL journal event length overflowed"))?;
        let frame_end = payload_end.checked_add(Sha256Digest::LENGTH)
            .ok_or_else(|| identity_error("ACL journal frame length overflowed"))?;
        if frame_end > bytes.len() {
            valid_bytes = frame_start;
            break;
        }
        let payload = &bytes[offset..payload_end];
        if peritus_codec::sha256(payload).as_bytes() != &bytes[payload_end..frame_end] {
            return Err(identity_error("ACL journal event checksum mismatched"));
        }
        if complete {
            return Err(identity_error("ACL journal contains events after completion"));
        }
        apply_replayed_event(Event::decode(payload)?, &mut reversals, &mut complete)?;
        offset = frame_end;
        valid_bytes = frame_end;
    }
    if reversals.len() > usize::try_from(header.group_count).unwrap_or(usize::MAX) {
        return Err(identity_error("ACL journal target count differs from its header"));
    }
    Ok(ParsedJournal { header, reversals, complete, valid_bytes })
}

#[cfg(target_os = "windows")]
fn apply_replayed_event(
    event: Event,
    reversals: &mut Vec<AclReversal>,
    transaction_complete: &mut bool,
) -> Result<(), WindowsError> {
    if let Event::TargetPending(value) = event {
        if value.index != reversals.len()
            || value.start >= value.end
            || value.create_intent != value.expected_target_identity.is_none()
        {
            return Err(identity_error("ACL journal target declaration is inconsistent"));
        }
        reversals.push(AclReversal::pending(value));
        return Ok(());
    }
    if matches!(event, Event::TransactionComplete) {
        if reversals.iter().any(|reversal| !reversal.complete) {
            return Err(identity_error("ACL journal completed with pending obligations"));
        }
        *transaction_complete = true;
        return Ok(());
    }
    let index = event_index(&event)
        .ok_or_else(|| identity_error("ACL journal event lacks a target identity"))?;
    let reversal = reversals.get_mut(index)
        .ok_or_else(|| identity_error("ACL journal event precedes its target declaration"))?;
    if reversal.complete {
        return Err(identity_error("ACL journal modifies a completed obligation"));
    }
    match event {
        Event::TargetReady { identity, created, .. } => {
            if reversal.target_identity.is_some()
                || created != reversal.create_intent
                || reversal.expected_target_identity.is_some_and(|expected| expected != identity)
            {
                return Err(identity_error("ACL journal target identity transition is invalid"));
            }
            reversal.target_identity = Some(identity);
            reversal.created = created;
        }
        Event::PreimageSavePending(_) => {
            if reversal.target_identity.is_none()
                || reversal.preimage_save_pending
                || reversal.preimage_saved
                || reversal.cleanup_started()
            {
                return Err(identity_error("ACL journal preimage save transition is invalid"));
            }
            reversal.preimage_save_pending = true;
        }
        Event::PreimageSaved(_) => {
            if reversal.target_identity.is_none()
                || !reversal.preimage_save_pending
                || reversal.preimage_saved
            {
                return Err(identity_error("ACL journal preimage transition is invalid"));
            }
            reversal.preimage_save_pending = false;
            reversal.preimage_saved = true;
        }
        Event::MutationPending(_) => {
            if !reversal.preimage_saved
                || reversal.mutation_pending
                || reversal.applied
                || reversal.cleanup_started()
            {
                return Err(identity_error("ACL journal mutation transition is invalid"));
            }
            reversal.mutation_pending = true;
        }
        Event::Applied(_) => {
            if !reversal.mutation_pending || reversal.applied {
                return Err(identity_error("ACL journal applied transition is invalid"));
            }
            reversal.mutation_pending = false;
            reversal.applied = true;
        }
        Event::ResetPending(_) => {
            if !reversal.preimage_saved
                || !reversal.mutation_pending
                || reversal.reset_pending
                || reversal.cleanup_started()
            {
                return Err(identity_error("ACL journal reset transition is invalid"));
            }
            reversal.reset_pending = true;
        }
        Event::ResetReady(_) => {
            if !reversal.reset_pending {
                return Err(identity_error("ACL journal reset completion is invalid"));
            }
            reversal.reset_pending = false;
            reversal.mutation_pending = false;
            reversal.applied = false;
        }
        Event::RestorePending(_) => {
            if reversal.target_identity.is_none()
                || reversal.restore_pending
                || reversal.restored
            {
                return Err(identity_error("ACL journal restore transition is invalid"));
            }
            reversal.restore_pending = true;
        }
        Event::Restored(_) => {
            if !reversal.restore_pending || reversal.restored {
                return Err(identity_error("ACL journal restore completion is invalid"));
            }
            reversal.restore_pending = false;
            reversal.restored = true;
            reversal.mutation_pending = false;
            reversal.reset_pending = false;
            reversal.applied = false;
        }
        Event::BackupDeletePending(_) => {
            if !(reversal.preimage_save_pending || reversal.preimage_saved)
                || reversal.backup_delete_pending
                || reversal.backup_deleted
            {
                return Err(identity_error("ACL journal backup deletion transition is invalid"));
            }
            reversal.backup_delete_pending = true;
        }
        Event::BackupDeleted(_) => {
            if !reversal.backup_delete_pending || reversal.backup_deleted {
                return Err(identity_error("ACL journal backup deletion completion is invalid"));
            }
            reversal.backup_delete_pending = false;
            reversal.backup_deleted = true;
        }
        Event::AnchorDeletePending(_) => {
            if !reversal.created
                || reversal.anchor_delete_pending
                || reversal.anchor_deleted
            {
                return Err(identity_error("ACL journal anchor deletion transition is invalid"));
            }
            reversal.anchor_delete_pending = true;
        }
        Event::AnchorDeleted(_) => {
            if !reversal.anchor_delete_pending || reversal.anchor_deleted {
                return Err(identity_error("ACL journal anchor deletion completion is invalid"));
            }
            reversal.anchor_delete_pending = false;
            reversal.anchor_deleted = true;
        }
        Event::ObligationComplete(_) => {
            if (reversal.preimage_save_pending || reversal.preimage_saved)
                && !reversal.backup_deleted
                || reversal.created && !reversal.anchor_deleted
            {
                return Err(identity_error("ACL journal retired an incomplete obligation"));
            }
            reversal.complete = true;
        }
        Event::TargetPending(_) | Event::TransactionComplete => unreachable!("handled above"),
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn inspect_target(
    entries: &[AclEntry],
    index: usize,
    start: usize,
    end: usize,
) -> Result<TargetPending, WindowsError> {
    let entry = entries.first().ok_or_else(|| {
        acl_error(WindowsOperation::InstallAcl, "ACL target group is empty")
    })?;
    if entries.iter().any(|candidate| {
        candidate.path != entry.path || candidate.authority_root != entry.authority_root
    }) {
        return Err(identity_error("ACL target group has inconsistent native authority"));
    }
    let authority = ResolvedWindowsPath::resolve(entry.authority_root.clone())?;
    let authority_identity = native_path_identity(&entry.authority_root.to_path_buf())?;
    let (anchor, exists) = ResolvedWindowsPath::resolve_existing_or_parent(entry.path.clone())?;
    if authority.evidence().volume_serial() != anchor.evidence().volume_serial() {
        return Err(identity_error("ACL target differs from its authorized root volume"));
    }
    let anchor_path = anchor.evidence().lexical().clone();
    let anchor_identity = native_path_identity(&anchor_path.to_path_buf())?;
    let may_create = entries.iter().any(AclEntry::creates_deny_directory);
    if !exists && !may_create {
        return Err(identity_error(
            "absent ACL target has no checked deny-anchor creation authority",
        ));
    }
    let create_intent = !exists;
    let expected_target_identity = if exists {
        Some(native_path_identity(&entry.path.to_path_buf())?)
    } else { None };
    Ok(TargetPending {
        index,
        start,
        end,
        create_intent,
        target: entry.path.clone(),
        authority: entry.authority_root.clone(),
        authority_identity,
        anchor: anchor_path,
        anchor_identity,
        expected_target_identity,
    })
}

#[cfg(target_os = "windows")]
fn rollback_install(
    transaction: &mut AclTransaction,
    original: WindowsError,
    should_continue: &dyn Fn() -> bool,
) -> WindowsError {
    let incomplete = transaction.restore_while(should_continue).is_err() || !transaction.restored();
    original.with_cleanup(crate::PreparationCleanup::new(incomplete, false, false, false))
}

#[cfg(target_os = "windows")]
fn save_acl(
    target: &Path,
    backup: &Path,
    should_continue: &dyn Fn() -> bool,
) -> Result<(), WindowsError> {
    let mut command = icacls_command(WindowsOperation::InstallAcl)?;
    command.arg(target).arg("/save").arg(backup).arg("/q");
    run_icacls(
        command,
        WindowsOperation::InstallAcl,
        "icacls ACL save could not start",
        "icacls ACL save failed",
        should_continue,
    )
}

#[cfg(target_os = "windows")]
fn apply_entry(
    target: &Path,
    principal: &str,
    entry: &AclEntry,
    replace_grants: bool,
    should_continue: &dyn Fn() -> bool,
) -> Result<(), WindowsError> {
    let switch = match (entry.effect, replace_grants) {
        (RuleEffect::Allow, true) => "/grant:r",
        (RuleEffect::Allow, false) => "/grant",
        (RuleEffect::Deny, _) => "/deny",
    };
    let inheritance = if entry.scope == PathScope::Descendants { "(OI)(CI)" } else { "" };
    let grant = format!("{principal}:{inheritance}{}", rights(entry.access));
    let mut command = icacls_command(WindowsOperation::InstallAcl)?;
    command.arg(target).args([switch, &grant, "/q"]);
    run_icacls(
        command,
        WindowsOperation::InstallAcl,
        "icacls mutation could not start",
        "icacls exact mutation failed",
        should_continue,
    )
}

#[cfg(target_os = "windows")]
fn restore_preimage(
    reversal: &AclReversal,
    should_continue: &dyn Fn() -> bool,
) -> Result<(), WindowsError> {
    if !reversal.preimage_saved { return Ok(()); }
    let target = reversal.target.to_path_buf();
    let parent = target.parent().unwrap_or(target.as_path());
    let mut command = icacls_command(WindowsOperation::RestoreAcl)?;
    command.arg(parent).arg("/restore").arg(&reversal.backup).arg("/q");
    run_icacls(
        command,
        WindowsOperation::RestoreAcl,
        "icacls restore could not start",
        "icacls exact restore failed",
        should_continue,
    )
}

#[cfg(target_os = "windows")]
fn run_icacls(
    mut command: std::process::Command,
    operation: WindowsOperation,
    start_failure: &'static str,
    status_failure: &'static str,
    should_continue: &dyn Fn() -> bool,
) -> Result<(), WindowsError> {
    if !should_continue() {
        return Err(acl_error(operation, "ACL operation was cancelled before process creation"));
    }
    let mut child = command.spawn().map_err(|_| acl_error(operation, start_failure))?;
    loop {
        if !should_continue() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(acl_error(operation, "ACL operation was cancelled by its retained owner"));
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err(acl_error(operation, status_failure)),
            Ok(None) => std::thread::yield_now(),
            Err(_) => return Err(acl_error(operation, status_failure)),
        }
    }
}

#[cfg(target_os = "windows")]
fn icacls_command(operation: WindowsOperation) -> Result<std::process::Command, WindowsError> {
    crate::native::probe::system_acl_tool()
        .map(std::process::Command::new)
        .ok_or_else(|| acl_error(operation, "system ACL tool is unavailable"))
}

#[cfg(target_os = "windows")]
fn acquire_owner_lock(
    backup_root: &Path,
    should_continue: &dyn Fn() -> bool,
) -> Result<File, WindowsError> {
    let path = backup_root.join("acl-owner-v1.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&path)
        .map_err(|_| acl_error(WindowsOperation::InstallAcl, "ACL owner lock cannot be opened"))?;
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| identity_error("ACL owner lock cannot be inspected"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(identity_error("ACL owner lock is not a real file"));
    }
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if should_continue() => std::thread::yield_now(),
            Err(TryLockError::WouldBlock) => return Err(acl_error(
                WindowsOperation::InstallAcl,
                "ACL owner lock acquisition was cancelled",
            )),
            Err(_) => return Err(acl_error(
                WindowsOperation::InstallAcl,
                "ACL owner lock cannot be acquired",
            )),
        }
    }
}

#[cfg(target_os = "windows")]
fn reject_other_incomplete_transactions(
    root: &Path,
    current: Sha256Digest,
) -> Result<(), WindowsError> {
    let current_name = OsString::from(hex(current.as_bytes()));
    for entry in std::fs::read_dir(root)
        .map_err(|_| identity_error("ACL transaction directory cannot be enumerated"))?
    {
        let entry = entry.map_err(|_| identity_error("ACL transaction entry cannot be read"))?;
        if entry.file_name() == current_name { continue; }
        let metadata = entry.metadata()
            .map_err(|_| identity_error("ACL transaction entry cannot be inspected"))?;
        if !metadata.is_dir() {
            return Err(identity_error("ACL transaction root contains an unknown record"));
        }
        match parse_journal(&entry.path().join("journal-v1.bin")) {
            Ok(parsed) if parsed.complete => {}
            Ok(_) => return Err(identity_error(
                "another ACL transaction retains unresolved reversal ownership",
            )),
            Err(_) => return Err(identity_error(
                "another ACL transaction record is missing or indeterminate",
            )),
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn prepare_storage_root(path: &Path) -> Result<(), WindowsError> {
    std::fs::create_dir_all(path).map_err(|_| {
        acl_error(WindowsOperation::InstallAcl, "ACL backup root cannot be created")
    })?;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| identity_error("ACL backup root cannot be inspected"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(identity_error("ACL backup root is not a real directory"));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn verify_identity(path: &Path, expected: NativePathIdentity) -> Result<(), WindowsError> {
    if native_path_identity(path)? == expected { Ok(()) } else {
        Err(identity_error(
            "ACL recovery path no longer has its original native identity",
        ))
    }
}

#[cfg(target_os = "windows")]
fn group_ranges(plan: &AclPlan) -> Result<Vec<(usize, usize)>, WindowsError> {
    let mut ranges = Vec::new();
    ranges.try_reserve_exact(plan.entries.len()).map_err(|_| {
        acl_error(WindowsOperation::InstallAcl, "ACL group capacity is unavailable")
    })?;
    let mut index = 0;
    while index < plan.entries.len() {
        let mut end = index + 1;
        while end < plan.entries.len() && plan.entries[end].path == plan.entries[index].path {
            end += 1;
        }
        ranges.push((index, end));
        index = end;
    }
    Ok(ranges)
}

#[cfg(target_os = "windows")]
fn transaction_digest(
    process_id: ProcessId,
    preparation_digest: Sha256Digest,
    plan_digest: Sha256Digest,
    owner_operation_digest: Option<Sha256Digest>,
    service_owner_digest: Option<Sha256Digest>,
) -> Sha256Digest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PERITUS-WINDOWS-ACL-TRANSACTION-V1\0");
    bytes.extend_from_slice(process_id.as_bytes());
    bytes.extend_from_slice(preparation_digest.as_bytes());
    bytes.extend_from_slice(plan_digest.as_bytes());
    bytes.extend_from_slice(owner_operation_digest.unwrap_or(Sha256Digest::new([0; 32])).as_bytes());
    bytes.extend_from_slice(service_owner_digest.unwrap_or(Sha256Digest::new([0; 32])).as_bytes());
    peritus_codec::sha256(&bytes)
}

#[cfg(target_os = "windows")]
fn transaction_receipt(
    transaction_digest: Sha256Digest,
    plan_digest: Sha256Digest,
    group_count: u32,
    owner_operation_digest: Option<Sha256Digest>,
    service_owner_digest: Option<Sha256Digest>,
) -> Sha256Digest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PERITUS-WINDOWS-ACL-RECEIPT-V1\0");
    bytes.extend_from_slice(transaction_digest.as_bytes());
    bytes.extend_from_slice(plan_digest.as_bytes());
    bytes.extend_from_slice(&group_count.to_be_bytes());
    bytes.extend_from_slice(owner_operation_digest.unwrap_or(Sha256Digest::new([0; 32])).as_bytes());
    bytes.extend_from_slice(service_owner_digest.unwrap_or(Sha256Digest::new([0; 32])).as_bytes());
    peritus_codec::sha256(&bytes)
}

#[cfg(target_os = "windows")]
fn encode_header(header: JournalHeader) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(JOURNAL_HEADER_BYTES);
    bytes.extend_from_slice(JOURNAL_MAGIC);
    bytes.extend_from_slice(header.transaction_digest.as_bytes());
    bytes.extend_from_slice(header.receipt.as_bytes());
    bytes.extend_from_slice(header.plan_digest.as_bytes());
    bytes.extend_from_slice(header.owner_operation_digest.unwrap_or(Sha256Digest::new([0; 32])).as_bytes());
    bytes.extend_from_slice(header.service_owner_digest.unwrap_or(Sha256Digest::new([0; 32])).as_bytes());
    bytes.extend_from_slice(&header.group_count.to_be_bytes());
    let checksum = peritus_codec::sha256(&bytes);
    bytes.extend_from_slice(checksum.as_bytes());
    bytes
}

#[cfg(target_os = "windows")]
fn decode_header(bytes: &[u8]) -> Result<JournalHeader, WindowsError> {
    if bytes.len() != JOURNAL_HEADER_BYTES || bytes.get(..8) != Some(JOURNAL_MAGIC.as_slice()) {
        return Err(identity_error("ACL journal header framing is invalid"));
    }
    let checksum_start = JOURNAL_HEADER_BYTES - Sha256Digest::LENGTH;
    if peritus_codec::sha256(&bytes[..checksum_start]).as_bytes() != &bytes[checksum_start..] {
        return Err(identity_error("ACL journal header checksum mismatched"));
    }
    let digest = |offset: usize| Sha256Digest::new(
        bytes[offset..offset + Sha256Digest::LENGTH]
            .try_into()
            .expect("fixed checked journal digest"),
    );
    let owner = digest(104);
    let service = digest(136);
    let owner_operation_digest = (*owner.as_bytes() != [0; 32]).then_some(owner);
    let service_owner_digest = (*service.as_bytes() != [0; 32]).then_some(service);
    if owner_operation_digest.is_some() != service_owner_digest.is_some() {
        return Err(identity_error("ACL journal retained owner binding is incomplete"));
    }
    let header = JournalHeader {
        transaction_digest: digest(8),
        receipt: digest(40),
        plan_digest: digest(72),
        owner_operation_digest,
        service_owner_digest,
        group_count: u32::from_be_bytes(
            bytes[168..172].try_into().expect("fixed checked group count"),
        ),
    };
    if transaction_receipt(
        header.transaction_digest,
        header.plan_digest,
        header.group_count,
        header.owner_operation_digest,
        header.service_owner_digest,
    ) != header.receipt
    {
        return Err(identity_error("ACL journal receipt does not match its header"));
    }
    Ok(header)
}

#[cfg(target_os = "windows")]
fn event_index(event: &Event) -> Option<usize> {
    match event {
        Event::TargetPending(value) => Some(value.index),
        Event::TargetReady { index, .. }
        | Event::PreimageSaved(index)
        | Event::PreimageSavePending(index)
        | Event::MutationPending(index)
        | Event::Applied(index)
        | Event::ResetPending(index)
        | Event::ResetReady(index)
        | Event::RestorePending(index)
        | Event::Restored(index)
        | Event::BackupDeletePending(index)
        | Event::BackupDeleted(index)
        | Event::AnchorDeletePending(index)
        | Event::AnchorDeleted(index)
        | Event::ObligationComplete(index) => Some(*index),
        Event::TransactionComplete => None,
    }
}

#[cfg(target_os = "windows")]
fn push_simple(bytes: &mut Vec<u8>, kind: u8, index: usize) -> Result<(), WindowsError> {
    bytes.push(kind);
    push_index(bytes, index)
}

#[cfg(target_os = "windows")]
fn push_index(bytes: &mut Vec<u8>, value: usize) -> Result<(), WindowsError> {
    bytes.extend_from_slice(&u32::try_from(value)
        .map_err(|_| identity_error("ACL journal index is not representable"))?
        .to_be_bytes());
    Ok(())
}

#[cfg(target_os = "windows")]
fn push_identity(bytes: &mut Vec<u8>, identity: NativePathIdentity) {
    bytes.extend_from_slice(&identity.volume_serial().to_be_bytes());
    bytes.extend_from_slice(&identity.file_id().to_be_bytes());
}

#[cfg(target_os = "windows")]
fn push_path(bytes: &mut Vec<u8>, path: &WindowsPath) -> Result<(), WindowsError> {
    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    push_index(bytes, units.len())?;
    for unit in units { bytes.extend_from_slice(&unit.to_be_bytes()); }
    Ok(())
}

#[cfg(target_os = "windows")]
struct EventReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

#[cfg(target_os = "windows")]
impl<'a> EventReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self { Self { bytes, offset: 0 } }

    fn u8(&mut self) -> Result<u8, WindowsError> { Ok(self.take(1)?[0]) }

    fn boolean(&mut self) -> Result<bool, WindowsError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(identity_error("ACL journal boolean is invalid")),
        }
    }

    fn u32(&mut self) -> Result<u32, WindowsError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().expect("checked four-byte integer")))
    }

    fn u64(&mut self) -> Result<u64, WindowsError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().expect("checked eight-byte integer")))
    }

    fn index(&mut self) -> Result<usize, WindowsError> {
        usize::try_from(self.u32()?)
            .map_err(|_| identity_error("ACL journal index is not representable"))
    }

    fn identity(&mut self) -> Result<NativePathIdentity, WindowsError> {
        NativePathIdentity::new(self.u64()?, self.u64()?)
    }

    fn path(&mut self) -> Result<WindowsPath, WindowsError> {
        let count = self.index()?;
        if count == 0 || count >= 32_767 {
            return Err(identity_error("ACL journal path length is invalid"));
        }
        let raw = self.take(count.checked_mul(2)
            .ok_or_else(|| identity_error("ACL journal path length overflowed"))?)?;
        let mut units = Vec::new();
        units.try_reserve_exact(count)
            .map_err(|_| identity_error("ACL journal path allocation is unavailable"))?;
        for pair in raw.chunks_exact(2) {
            units.push(u16::from_be_bytes([pair[0], pair[1]]));
        }
        WindowsPath::from_os_str(OsString::from_wide(&units).as_os_str())
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], WindowsError> {
        let end = self.offset.checked_add(count)
            .ok_or_else(|| identity_error("ACL journal event length overflowed"))?;
        let value = self.bytes.get(self.offset..end)
            .ok_or_else(|| identity_error("ACL journal event is truncated"))?;
        self.offset = end;
        Ok(value)
    }

    fn finish(self) -> Result<(), WindowsError> {
        if self.offset == self.bytes.len() { Ok(()) } else {
            Err(identity_error("ACL journal event has trailing bytes"))
        }
    }
}

#[cfg(target_os = "windows")]
fn sync_directory(directory: &Path) -> Result<(), WindowsError> {
    use std::os::windows::fs::OpenOptionsExt as _;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| acl_error(
            WindowsOperation::InstallAcl,
            "ACL transaction directory cannot be synchronized",
        ))
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
        if access.contains(operation) { values.push(text); }
    }
    format!("({})", values.join(","))
}

#[cfg(target_os = "windows")]
fn acl_error(operation: WindowsOperation, detail: &'static str) -> WindowsError {
    WindowsError::new(WindowsErrorKind::Acl, operation, WindowsRecovery::RetryCleanup, detail)
}

#[cfg(target_os = "windows")]
fn identity_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::RecoveryIndeterminate,
        WindowsOperation::Recover,
        WindowsRecovery::Quarantine,
        detail,
    )
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

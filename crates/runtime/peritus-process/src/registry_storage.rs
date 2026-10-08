//! Protected registry filesystem layout and atomic record persistence.

use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read as _, Write},
    path::{Path, PathBuf},
};

use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    ErrorCode, ExecutionIdentity, ProcessError, ProcessOperation, RecoveryClass,
    consumption::store_cause,
    recovery::{claim::ConsumptionClaim, manifest::ExecutionManifest},
};

mod tombstone;
pub(crate) use tombstone::{load_canonical_tombstone, load_tombstone, persist_tombstone};
mod native_observation;
pub(crate) use native_observation::{
    persist_native_observation_page, validate_native_observation_frontier,
};
mod retained_owner;
pub(crate) use retained_owner::{
    RetainedOwnerTransaction, acquire_retained_owner_transaction,
    discard_unclaimed_retained_owner_request, load_retained_owner_request,
    load_unclaimed_retained_owner_request, restore_retained_consumptions,
    stage_retained_owner_request,
};
mod completion;
pub(crate) use completion::{
    load_owner_completion_observation, load_owner_completion_request,
    load_owner_completion_status, load_owner_completion_stream_page,
    persist_owner_completion,
};

const QUARANTINED_IDENTITY_MAGIC: &[u8] = b"PERITUS-PROCESS-QUARANTINED-IDENTITY-V1\0";

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StorageFaultPoint {
    StagingWrite,
    StagingSync,
    PreserveRename,
    PublishRename,
    DirectorySync,
    BackupDelete,
}

pub(crate) fn visit_registry_identities(
    directory: &Path,
    extension: &str,
    retained_owners: &Path,
    quarantine: &Path,
    quarantined_identities: &Path,
    quarantined: &mut Vec<PathBuf>,
    mut visit: impl FnMut(ProcessId, &mut Vec<PathBuf>) -> Result<(), ProcessError>,
) -> Result<(), ProcessError> {
    for entry in fs::read_dir(directory)
        .map_err(|error| store_cause("registry directory cannot be read", error))?
    {
        let entry = entry
            .map_err(|error| store_cause("registry entry cannot be inspected", error))?;
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some(extension) {
            continue;
        }
        let Some(process_id) = path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(process_id_from_hex)
        else {
            quarantine_path(
                &path,
                quarantine,
                quarantined_identities,
                quarantined,
            )?;
            continue;
        };
        let _transaction = acquire_retained_owner_transaction(retained_owners, process_id)?;
        visit(process_id, quarantined)?;
    }
    Ok(())
}

pub(crate) fn visit_quarantined_identities(
    directory: &Path,
    retained_owners: &Path,
    mut visit: impl FnMut(ProcessId) -> Result<(), ProcessError>,
) -> Result<(), ProcessError> {
    for entry in fs::read_dir(directory)
        .map_err(|error| store_cause("quarantined identity directory cannot be read", error))?
    {
        let entry = entry
            .map_err(|error| store_cause("quarantined identity cannot be inspected", error))?;
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("identity") {
            continue;
        }
        let process_id = path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(process_id_from_hex)
            .ok_or_else(|| store_error("quarantined identity has an invalid filename"))?;
        let _transaction = acquire_retained_owner_transaction(retained_owners, process_id)?;
        if !load_quarantined_identity(directory, process_id)? {
            return Err(store_error("quarantined identity disappeared during inspection"));
        }
        visit(process_id)?;
    }
    Ok(())
}

pub(crate) fn load_quarantine(quarantine: &Path) -> Result<Vec<PathBuf>, ProcessError> {
    let mut records = Vec::new();
    for entry in
        fs::read_dir(quarantine).map_err(|_| store_error("quarantine directory cannot be read"))?
    {
        let entry = entry.map_err(|_| store_error("quarantine entry cannot be inspected"))?;
        let file_type = entry
            .file_type()
            .map_err(|_| store_error("quarantine entry type cannot be inspected"))?;
        if !file_type.is_file() {
            return Err(store_error("quarantine contains a non-file entry"));
        }
        records.push(entry.path());
    }
    records.sort();
    Ok(records)
}

pub(crate) fn backfill_quarantined_identities(
    quarantine: &Path,
    quarantined_identities: &Path,
    retained_owners: &Path,
) -> Result<(), ProcessError> {
    for path in load_quarantine(quarantine)? {
        let Some(process_id) = quarantined_identity(&path) else {
            continue;
        };
        let _transaction = acquire_retained_owner_transaction(retained_owners, process_id)?;
        persist_quarantined_identity(quarantined_identities, process_id)?;
    }
    Ok(())
}

pub(crate) fn load_claim(
    directory: &Path,
    quarantine: &Path,
    quarantined_identities: &Path,
    quarantined: &mut Vec<PathBuf>,
    process_id: ProcessId,
) -> Result<Option<ConsumptionClaim>, ProcessError> {
    let path = directory.join(format!("{}.claim", hex(process_id.as_bytes())));
    let bytes = match read_canonical_file(&path, "process consumption claim cannot be read") {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Ok(None),
        Err(_) => {
            quarantine_path(
                &path,
                quarantine,
                quarantined_identities,
                quarantined,
            )?;
            return Ok(None);
        }
    };
    match ConsumptionClaim::decode(&bytes) {
        Ok(claim) if claim.process_id() == process_id => Ok(Some(claim)),
        Ok(_) | Err(_) => {
            quarantine_path(
                &path,
                quarantine,
                quarantined_identities,
                quarantined,
            )?;
            Ok(None)
        }
    }
}

pub(crate) fn load_manifest(
    directory: &Path,
    quarantine: &Path,
    quarantined_identities: &Path,
    quarantined: &mut Vec<PathBuf>,
    process_id: ProcessId,
) -> Result<Option<ExecutionManifest>, ProcessError> {
    let path = directory.join(format!("{}.manifest", hex(process_id.as_bytes())));
    let bytes = match read_canonical_file(&path, "process manifest cannot be read") {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Ok(None),
        Err(_) => {
            quarantine_path(
                &path,
                quarantine,
                quarantined_identities,
                quarantined,
            )?;
            return Ok(None);
        }
    };
    match ExecutionManifest::decode(&bytes) {
        Ok(manifest) if manifest.identity.process_id() == process_id => Ok(Some(manifest)),
        Ok(_) | Err(_) => {
            quarantine_path(
                &path,
                quarantine,
                quarantined_identities,
                quarantined,
            )?;
            Ok(None)
        }
    }
}

pub(crate) fn load_quarantined_identity(
    directory: &Path,
    process_id: ProcessId,
) -> Result<bool, ProcessError> {
    let path = directory.join(format!("{}.identity", hex(process_id.as_bytes())));
    let Some(bytes) = read_canonical_file(&path, "quarantined identity cannot be read")? else {
        return Ok(false);
    };
    if bytes != quarantined_identity_bytes(process_id) {
        return Err(store_error("quarantined identity binding is invalid"));
    }
    Ok(true)
}

pub(crate) fn persist_quarantined_identity(
    directory: &Path,
    process_id: ProcessId,
) -> Result<(), ProcessError> {
    let path = directory.join(format!("{}.identity", hex(process_id.as_bytes())));
    publish_immutable_file(
        directory,
        &path,
        &quarantined_identity_bytes(process_id),
        ExistingImmutable::RequireExact,
    )
}

pub(super) fn quarantine_path(
    path: &Path,
    quarantine: &Path,
    quarantined_identities: &Path,
    quarantined: &mut Vec<PathBuf>,
) -> Result<(), ProcessError> {
    if let Some(process_id) = canonical_identity(path) {
        persist_quarantined_identity(quarantined_identities, process_id)?;
    }
    let name = path.file_name().and_then(std::ffi::OsStr::to_str).unwrap_or("unknown");
    let mut sequence = 0_u64;
    let target = loop {
        let candidate = quarantine.join(format!("{name}.{sequence:016x}.quarantined"));
        if !candidate.exists() {
            break candidate;
        }
        sequence = sequence
            .checked_add(1)
            .ok_or_else(|| store_error("quarantine filename space is exhausted"))?;
    };
    fs::rename(path, &target)
        .map_err(|_| store_error("corrupt registry record cannot be quarantined"))?;
    sync_directory(quarantine)?;
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    quarantined.push(target);
    Ok(())
}

pub(crate) fn persist_claim(
    directory: &Path,
    identity: &ExecutionIdentity,
    action_digest: Sha256Digest,
    plan_digest: Sha256Digest,
) -> Result<ConsumptionClaim, ProcessError> {
    let claim = ConsumptionClaim::new(identity, action_digest, plan_digest);
    persist_exact_claim(directory, identity.process_id(), claim)
}

pub(crate) fn persist_retained_claim(
    directory: &Path,
    identity: &ExecutionIdentity,
    action_digest: Sha256Digest,
    plan_digest: Sha256Digest,
    binding: crate::recovery::claim::RetainedClaimBinding,
) -> Result<ConsumptionClaim, ProcessError> {
    let claim = ConsumptionClaim::new_retained(identity, action_digest, plan_digest, binding);
    persist_exact_claim(directory, identity.process_id(), claim)
}

fn persist_exact_claim(
    directory: &Path,
    process_id: ProcessId,
    claim: ConsumptionClaim,
) -> Result<ConsumptionClaim, ProcessError> {
    let path = directory.join(format!("{}.claim", hex(process_id.as_bytes())));
    publish_immutable_file(
        directory,
        &path,
        &claim.encode(),
        ExistingImmutable::RejectAsReused,
    )?;
    Ok(claim)
}

pub(crate) fn write_manifest(
    directory: &Path,
    manifest: &ExecutionManifest,
) -> Result<(), ProcessError> {
    let name = hex(manifest.identity.process_id().as_bytes());
    let target = directory.join(format!("{name}.manifest"));
    let staging = directory.join(format!("{name}.staging"));
    let backup = directory.join(format!("{name}.previous"));
    let bytes = manifest.encode()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&staging)
        .map_err(|_| store_error("manifest staging file cannot be opened"))?;
    #[cfg(test)]
    fault_control::check(StorageFaultPoint::StagingWrite)
        .map_err(|_| store_error("manifest staging file cannot be written"))?;
    file.write_all(&bytes).map_err(|_| store_error("manifest staging file cannot be written"))?;
    #[cfg(test)]
    fault_control::check(StorageFaultPoint::StagingSync)
        .map_err(|_| store_error("manifest staging file cannot be synchronized"))?;
    file.sync_all().map_err(|_| store_error("manifest staging file cannot be synchronized"))?;
    if target.exists() {
        let _ = fs::remove_file(&backup);
        #[cfg(test)]
        fault_control::check(StorageFaultPoint::PreserveRename)
            .map_err(|_| store_error("prior manifest cannot be preserved for replacement"))?;
        fs::rename(&target, &backup)
            .map_err(|_| store_error("prior manifest cannot be preserved for replacement"))?;
    }
    #[cfg(test)]
    let publish_fault = fault_control::check(StorageFaultPoint::PublishRename).is_err();
    #[cfg(not(test))]
    let publish_fault = false;
    if publish_fault || fs::rename(&staging, &target).is_err() {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(store_error("manifest replacement failed"));
    }
    #[cfg(test)]
    fault_control::check(StorageFaultPoint::DirectorySync)
        .map_err(|_| store_error("registry directory cannot be synchronized"))?;
    sync_directory(directory)?;
    if backup.exists() {
        #[cfg(test)]
        fault_control::check(StorageFaultPoint::BackupDelete)
            .map_err(|_| store_error("manifest backup cannot be removed"))?;
        fs::remove_file(&backup).map_err(|_| store_error("manifest backup cannot be removed"))?;
        sync_directory(directory)?;
    }
    Ok(())
}

pub(crate) fn retire_execution_record(
    _claims: &Path,
    manifests: &Path,
    spools: &Path,
    process_id: ProcessId,
) -> Result<(), ProcessError> {
    let name = hex(process_id.as_bytes());
    let manifest = manifests.join(format!("{name}.manifest"));
    let spool = spools.join(&name);

    // Retain the fixed-size one-use receipt so older binaries also refuse identity reuse.
    if spool.exists() {
        fs::remove_dir_all(&spool)
            .map_err(|_| store_error("retired process spool cannot be removed"))?;
        sync_directory(spools)?;
    }
    if manifest.exists() {
        remove_file_if_present(&manifest, "retired process manifest cannot be removed")?;
        sync_directory(manifests)?;
    }
    Ok(())
}

fn remove_file_if_present(path: &Path, detail: &'static str) -> Result<(), ProcessError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(_) => Err(store_error(detail)),
    }
}

pub(crate) fn preserve_consumption_claim(
    directory: &Path,
    claim: ConsumptionClaim,
) -> Result<(), ProcessError> {
    let path = directory.join(format!("{}.claim", hex(claim.process_id().as_bytes())));
    publish_immutable_file(
        directory,
        &path,
        &claim.encode(),
        ExistingImmutable::RequireExact,
    )
}

pub(crate) fn restore_backups(
    directory: &Path,
    retained_owners: &Path,
    quarantined_identities: &Path,
) -> Result<(), ProcessError> {
    for entry in
        fs::read_dir(directory).map_err(|_| store_error("manifest directory cannot be read"))?
    {
        let entry = entry.map_err(|_| store_error("manifest backup cannot be inspected"))?;
        let path = entry.path();
        let extension = path.extension().and_then(std::ffi::OsStr::to_str);
        if !matches!(extension, Some("previous" | "staging")) {
            continue;
        }
        let process_id = path
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(process_id_from_hex)
            .ok_or_else(|| store_error("manifest recovery file has an invalid name"))?;
        let _transaction = acquire_retained_owner_transaction(retained_owners, process_id)?;
        if load_quarantined_identity(quarantined_identities, process_id)? {
            fs::remove_file(path)
                .map_err(|_| store_error("quarantined manifest recovery file cannot be removed"))?;
            continue;
        }
        match extension {
            Some("previous") => restore_previous(&path, directory)?,
            Some("staging") => fs::remove_file(path)
                .map_err(|_| store_error("stale manifest staging cannot be removed"))?,
            _ => unreachable!("manifest recovery extension was checked above"),
        }
    }
    sync_directory(directory)
}

fn restore_previous(path: &Path, directory: &Path) -> Result<(), ProcessError> {
    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| store_error("manifest backup has an invalid name"))?;
    let target = directory.join(format!("{stem}.manifest"));
    if target.exists() {
        fs::remove_file(path).map_err(|_| store_error("stale manifest backup cannot be removed"))
    } else {
        fs::rename(path, target).map_err(|_| store_error("manifest backup cannot be restored"))
    }
}

#[derive(Clone, Copy)]
enum ExistingImmutable {
    RejectAsReused,
    RequireExact,
}

fn publish_immutable_file(
    directory: &Path,
    path: &Path,
    bytes: &[u8],
    existing: ExistingImmutable,
) -> Result<(), ProcessError> {
    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| store_cause("immutable registry record cannot be staged", error))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| {
            store_cause("immutable registry record cannot be synchronized", error)
        })?;
    match temporary.persist_noclobber(path) {
        Ok(_) => sync_directory(directory),
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => match existing {
            ExistingImmutable::RejectAsReused => Err(reused()),
            ExistingImmutable::RequireExact => {
                let actual = read_canonical_file(path, "immutable registry record cannot be reread")?
                    .ok_or_else(|| store_error("immutable registry record disappeared"))?;
                if actual != bytes {
                    return Err(store_error("immutable registry record conflicts"));
                }
                File::open(path)
                    .and_then(|file| file.sync_all())
                    .map_err(|error| {
                        store_cause("immutable registry record cannot be resynchronized", error)
                    })?;
                sync_directory(directory)
            }
        },
        Err(error) => Err(store_cause(
            "immutable registry record cannot be published",
            error.error,
        )),
    }
}

fn read_canonical_file(
    path: &Path,
    detail: &'static str,
) -> Result<Option<Vec<u8>>, ProcessError> {
    let path_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(store_cause(detail, error)),
    };
    if !path_metadata.file_type().is_file() || path_metadata.file_type().is_symlink() {
        return Err(store_error(detail));
    }
    let file = File::open(path).map_err(|error| store_cause(detail, error))?;
    let metadata = file.metadata().map_err(|error| store_cause(detail, error))?;
    if !metadata.file_type().is_file() {
        return Err(store_error(detail));
    }
    let length = usize::try_from(metadata.len()).map_err(|error| store_cause(detail, error))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|error| store_cause(detail, error))?;
    file.take(metadata.len().saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| store_cause(detail, error))?;
    if bytes.len() != length {
        return Err(store_error(detail));
    }
    Ok(Some(bytes))
}

fn quarantined_identity_bytes(process_id: ProcessId) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(QUARANTINED_IDENTITY_MAGIC.len() + 16);
    bytes.extend_from_slice(QUARANTINED_IDENTITY_MAGIC);
    bytes.extend_from_slice(process_id.as_bytes());
    bytes
}

fn canonical_identity(path: &Path) -> Option<ProcessId> {
    path.file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .and_then(process_id_from_hex)
}

pub(crate) fn quarantined_identity(path: &Path) -> Option<ProcessId> {
    let name = path.file_name()?.to_str()?;
    for suffix in [".claim", ".manifest", ".tombstone"] {
        let Some(end) = name.find(suffix) else {
            continue;
        };
        let prefix = &name[..end];
        if let Some(process_id) = prefix.rsplit('-').next().and_then(process_id_from_hex) {
            return Some(process_id);
        }
    }
    None
}

pub(crate) fn process_id_from_hex(value: &str) -> Option<ProcessId> {
    if value.len() != 32 {
        return None;
    }
    let mut bytes = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let digit = |byte| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        };
        bytes[index] = digit(pair[0])?
            .checked_mul(16)?
            .checked_add(digit(pair[1])?)?;
    }
    ProcessId::new(bytes).ok()
}

pub(crate) fn create_checked_directory(root: &Path, directory: &Path) -> Result<(), ProcessError> {
    fs::create_dir_all(directory)
        .map_err(|_| store_error("registry directory cannot be created"))?;
    let metadata = fs::symlink_metadata(directory)
        .map_err(|_| store_error("registry directory cannot be inspected"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(store_error("registry path is not a real directory"));
    }
    let canonical = fs::canonicalize(directory)
        .map_err(|_| store_error("registry directory cannot be canonicalized"))?;
    if !canonical.starts_with(root) {
        return Err(store_error("registry directory escaped its protected root"));
    }
    Ok(())
}

fn sync_directory(directory: &Path) -> Result<(), ProcessError> {
    sync_directory_os(directory)
        .map_err(|_| store_error("registry directory cannot be synchronized"))
}

#[cfg(not(windows))]
fn sync_directory_os(directory: &Path) -> std::io::Result<()> {
    File::open(directory)?.sync_all()
}

#[cfg(windows)]
fn sync_directory_os(directory: &Path) -> std::io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(directory)?
        .sync_all()
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use core::fmt::Write as _;
        write!(&mut result, "{byte:02x}").expect("writing to String is infallible");
    }
    result
}

const fn reused() -> ProcessError {
    ProcessError::new(
        ErrorCode::ReceiptReused,
        ProcessOperation::Authorize,
        RecoveryClass::Reauthorize,
        "action/process authority was already durably consumed",
    )
}

const fn store_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Persistence,
        ProcessOperation::Persist,
        RecoveryClass::ReopenAndReconcile,
        detail,
    )
}

#[cfg(test)]
pub(crate) mod fault_control;

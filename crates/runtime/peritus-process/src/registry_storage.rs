//! Protected registry filesystem layout and atomic record persistence.

use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    ErrorCode, ExecutionIdentity, ProcessError, ProcessOperation, RecoveryClass,
    recovery::{claim::ConsumptionClaim, manifest::ExecutionManifest},
};

pub(crate) fn load_claims(
    claims: &Path,
    quarantine: &Path,
    decoded: &mut BTreeMap<ProcessId, ConsumptionClaim>,
    quarantined: &mut Vec<PathBuf>,
) -> Result<(), ProcessError> {
    for entry in fs::read_dir(claims).map_err(|_| store_error("claim directory cannot be read"))? {
        let entry = entry.map_err(|_| store_error("claim entry cannot be inspected"))?;
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("claim") {
            continue;
        }
        let claim = fs::read(&path)
            .map_err(|_| store_error("process consumption claim cannot be read"))
            .and_then(|bytes| ConsumptionClaim::decode(&bytes));
        match claim {
            Ok(claim) => {
                let process_id = claim.process_id();
                if path.file_stem().and_then(std::ffi::OsStr::to_str)
                    != Some(hex(process_id.as_bytes()).as_str())
                    || decoded.insert(process_id, claim).is_some()
                {
                    quarantine_path(&path, quarantine, quarantined)?;
                }
            }
            Err(_) => quarantine_path(&path, quarantine, quarantined)?,
        }
    }
    Ok(())
}

pub(crate) fn load_manifests(
    manifests: &Path,
    quarantine: &Path,
    decoded: &mut BTreeMap<ProcessId, ExecutionManifest>,
    quarantined: &mut Vec<PathBuf>,
) -> Result<(), ProcessError> {
    for entry in
        fs::read_dir(manifests).map_err(|_| store_error("manifest directory cannot be read"))?
    {
        let entry = entry.map_err(|_| store_error("manifest entry cannot be inspected"))?;
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("manifest") {
            continue;
        }
        let manifest = fs::read(&path)
            .map_err(|_| store_error("process manifest cannot be read"))
            .and_then(|bytes| ExecutionManifest::decode(&bytes));
        match manifest {
            Ok(manifest) => {
                let process_id = manifest.identity.process_id();
                if path.file_stem().and_then(std::ffi::OsStr::to_str)
                    != Some(hex(process_id.as_bytes()).as_str())
                    || decoded.insert(process_id, manifest).is_some()
                {
                    quarantine_path(&path, quarantine, quarantined)?;
                }
            }
            Err(_) => quarantine_path(&path, quarantine, quarantined)?,
        }
    }
    Ok(())
}

fn quarantine_path(
    path: &Path,
    quarantine: &Path,
    quarantined: &mut Vec<PathBuf>,
) -> Result<(), ProcessError> {
    let sequence = quarantined.len();
    let name = path.file_name().and_then(std::ffi::OsStr::to_str).unwrap_or("unknown");
    let target = quarantine.join(format!("{sequence:08}-{name}"));
    fs::rename(path, &target)
        .map_err(|_| store_error("corrupt registry record cannot be quarantined"))?;
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
    let path = directory.join(format!("{}.claim", hex(identity.process_id().as_bytes())));
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => return Err(reused()),
        Err(_) => return Err(store_error("exclusive process consumption claim cannot be created")),
    };
    let bytes = claim.encode();
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| store_error("process consumption claim cannot be synchronized"))?;
    sync_directory(directory)?;
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
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| store_error("manifest staging file cannot be synchronized"))?;
    if target.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(&target, &backup)
            .map_err(|_| store_error("prior manifest cannot be preserved for replacement"))?;
    }
    if fs::rename(&staging, &target).is_err() {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(store_error("manifest replacement failed"));
    }
    sync_directory(directory)?;
    if backup.exists() {
        fs::remove_file(&backup).map_err(|_| store_error("manifest backup cannot be removed"))?;
        sync_directory(directory)?;
    }
    Ok(())
}

pub(crate) fn restore_backups(directory: &Path) -> Result<(), ProcessError> {
    for entry in
        fs::read_dir(directory).map_err(|_| store_error("manifest directory cannot be read"))?
    {
        let entry = entry.map_err(|_| store_error("manifest backup cannot be inspected"))?;
        let path = entry.path();
        match path.extension().and_then(std::ffi::OsStr::to_str) {
            Some("previous") => restore_previous(&path, directory)?,
            Some("staging") => fs::remove_file(path)
                .map_err(|_| store_error("stale manifest staging cannot be removed"))?,
            _ => {}
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
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| store_error("registry directory cannot be synchronized"))
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

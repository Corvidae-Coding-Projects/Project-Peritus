//! Pre-consumption retained-owner request staging and claim-linked recovery.

use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read as _, Write as _},
    path::Path,
};

use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    ProcessError,
    consumption::{store_cause, store_error},
    recovery::{claim::ConsumptionClaim, manifest::ExecutionManifest},
};

use super::{
    hex, load_canonical_tombstone, load_claim, load_manifest, load_quarantined_identity,
    persist_quarantined_identity, process_id_from_hex, quarantine_path, sync_directory,
    write_manifest,
};

const REQUEST_SUFFIX: &str = "request";
const AUTHORIZED_SUFFIX: &str = "authorized";

pub(crate) struct RetainedOwnerTransaction {
    process_id: ProcessId,
    _lock: File,
}

impl RetainedOwnerTransaction {
    pub(crate) fn process_id(&self) -> ProcessId {
        self.process_id
    }
}

pub(crate) fn acquire_retained_owner_transaction(
    directory: &Path,
    process_id: ProcessId,
) -> Result<RetainedOwnerTransaction, ProcessError> {
    let path = directory.join(format!("{}.transaction", hex(process_id.as_bytes())));
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| store_cause("retained owner transaction cannot be opened", error))?;
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| store_cause("retained owner transaction cannot be inspected", error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(store_error("retained owner transaction is not a regular file"));
    }
    lock.lock()
        .map_err(|error| store_cause("retained owner transaction cannot be locked", error))?;
    if !lock
        .metadata()
        .map_err(|error| store_cause("retained owner transaction cannot be inspected", error))?
        .file_type()
        .is_file()
    {
        return Err(store_error("retained owner transaction is not a regular file"));
    }
    Ok(RetainedOwnerTransaction {
        process_id,
        _lock: lock,
    })
}

pub(crate) fn stage_retained_owner_request(
    transaction: &RetainedOwnerTransaction,
    directory: &Path,
    request: &[u8],
    request_digest: Sha256Digest,
    manifest: &ExecutionManifest,
) -> Result<(), ProcessError> {
    let process_id = transaction.process_id();
    if peritus_codec::sha256(request) != request_digest
        || manifest.identity.process_id() != process_id
    {
        return Err(store_error("retained owner staging binding is inconsistent"));
    }
    let name = hex(process_id.as_bytes());
    persist_immutable(&directory.join(format!("{name}.{REQUEST_SUFFIX}")), request)?;
    persist_immutable(
        &directory.join(format!("{name}.{AUTHORIZED_SUFFIX}")),
        &manifest.encode()?,
    )?;
    sync_directory(directory)
}

pub(crate) fn restore_retained_consumptions(
    directory: &Path,
    claims: &Path,
    manifests: &Path,
    tombstones: &Path,
    quarantine: &Path,
    quarantined_identities: &Path,
) -> Result<(), ProcessError> {
    let mut quarantined = Vec::new();
    for entry in fs::read_dir(directory)
        .map_err(|error| store_cause("retained owner directory cannot be read", error))?
    {
        let entry = entry
            .map_err(|error| store_cause("retained owner entry cannot be inspected", error))?;
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some(AUTHORIZED_SUFFIX) {
            continue;
        }
        let Some(name) = path.file_stem().and_then(std::ffi::OsStr::to_str) else {
            continue;
        };
        let Some(process_id) = process_id_from_hex(name) else {
            continue;
        };
        if restore_retained_consumption(
            directory,
            claims,
            manifests,
            tombstones,
            quarantine,
            quarantined_identities,
            &mut quarantined,
            process_id,
        )
        .is_err()
        {
            quarantine_retained_owner_stage(
                directory,
                quarantine,
                quarantined_identities,
                &mut quarantined,
                process_id,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn restore_retained_consumption(
    directory: &Path,
    claims: &Path,
    manifests: &Path,
    tombstones: &Path,
    quarantine: &Path,
    quarantined_identities: &Path,
    quarantined: &mut Vec<std::path::PathBuf>,
    process_id: ProcessId,
) -> Result<(), ProcessError> {
    let _transaction = acquire_retained_owner_transaction(directory, process_id)?;
    if load_quarantined_identity(quarantined_identities, process_id)? {
        return Ok(());
    }
    let Some(claim) = load_claim(
        claims,
        quarantine,
        quarantined_identities,
        quarantined,
        process_id,
    )? else {
        return Ok(());
    };
    let Some(owner) = claim.retained_owner() else {
        return Ok(());
    };
    if claim.process_id() != process_id {
        return Err(store_error("retained owner claim identity differs"));
    }
    if let Some((terminal_claim, terminal_manifest)) = load_canonical_tombstone(
        tombstones,
        quarantine,
        quarantined_identities,
        quarantined,
        process_id,
    )? {
        // Retirement publishes the immutable claim+terminal tombstone before deleting the active
        // manifest. The retained proposal must never recreate its Authorized predecessor.
        if terminal_claim != claim || !terminal_claim.matches_manifest(&terminal_manifest) {
            return Err(store_error(
                "retained owner tombstone differs from its consumption claim",
            ));
        }
        return Ok(());
    }
    if load_quarantined_identity(quarantined_identities, process_id)? {
        return Ok(());
    }
    let name = hex(process_id.as_bytes());
    let request = read_regular_file(
        &directory.join(format!("{name}.{REQUEST_SUFFIX}")),
        "retained owner request cannot be read",
    )
    .map_err(|error| store_cause("retained owner request cannot be read", error))?;
    if peritus_codec::sha256(&request) != owner.request_digest() {
        return Err(store_error("retained owner request differs from its consumption claim"));
    }
    let manifest = read_regular_file(
        &directory.join(format!("{name}.{AUTHORIZED_SUFFIX}")),
        "staged retained owner manifest cannot be read",
    )
    .map_err(|error| store_cause("staged retained owner manifest cannot be read", error))
    .and_then(|bytes| ExecutionManifest::decode(&bytes))?;
    if !claim.matches_manifest(&manifest) {
        return Err(store_error("staged retained owner manifest differs from its claim"));
    }
    match load_manifest(
        manifests,
        quarantine,
        quarantined_identities,
        quarantined,
        process_id,
    )? {
        Some(retained)
            if claim.matches_manifest(&retained)
                && retained.has_same_authorization(&manifest) => {}
        Some(_) => {
            return Err(store_error(
                "retained owner manifest differs from staged authorization",
            ));
        }
        None if load_quarantined_identity(quarantined_identities, process_id)? => {}
        None => write_manifest(manifests, &manifest)?,
    }
    Ok(())
}

fn quarantine_retained_owner_stage(
    directory: &Path,
    quarantine: &Path,
    quarantined_identities: &Path,
    quarantined: &mut Vec<std::path::PathBuf>,
    process_id: ProcessId,
) -> Result<(), ProcessError> {
    persist_quarantined_identity(quarantined_identities, process_id)?;
    let name = hex(process_id.as_bytes());
    for suffix in [REQUEST_SUFFIX, AUTHORIZED_SUFFIX] {
        let path = directory.join(format!("{name}.{suffix}"));
        match fs::symlink_metadata(&path) {
            Ok(_) => quarantine_path(&path, quarantine, quarantined_identities, quarantined)?,
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(store_cause(
                    "retained owner quarantine candidate cannot be inspected",
                    error,
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn load_retained_owner_request(
    directory: &Path,
    claims: &Path,
    manifests: &Path,
    process_id: ProcessId,
) -> Result<Option<(Sha256Digest, Sha256Digest, Vec<u8>, crate::LifecyclePhase)>, ProcessError> {
    let name = hex(process_id.as_bytes());
    let claim_path = claims.join(format!("{name}.claim"));
    let claim = match read_regular_file(&claim_path, "retained owner claim cannot be read") {
        Ok(bytes) => ConsumptionClaim::decode(&bytes)?,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(store_cause("retained owner claim cannot be read", error)),
    };
    let Some(owner) = claim.retained_owner() else {
        return Ok(None);
    };
    if claim.process_id() != process_id {
        return Err(store_error("retained owner claim identity differs"));
    }
    let request = read_regular_file(
        &directory.join(format!("{name}.{REQUEST_SUFFIX}")),
        "retained owner request cannot be read",
    )
    .map_err(|error| store_cause("retained owner request cannot be read", error))?;
    if peritus_codec::sha256(&request) != owner.request_digest() {
        return Err(store_error("retained owner request differs from its consumption claim"));
    }
    let staged = read_regular_file(
        &directory.join(format!("{name}.{AUTHORIZED_SUFFIX}")),
        "staged retained owner manifest cannot be read",
    )
    .map_err(|error| store_cause("staged retained owner manifest cannot be read", error))
    .and_then(|bytes| ExecutionManifest::decode(&bytes))?;
    let retained = read_regular_file(
        &manifests.join(format!("{name}.manifest")),
        "retained owner manifest cannot be read",
    )
    .map_err(|error| store_cause("retained owner manifest cannot be read", error))
    .and_then(|bytes| ExecutionManifest::decode(&bytes))?;
    if !claim.matches_manifest(&staged)
        || !claim.matches_manifest(&retained)
        || !retained.has_same_authorization(&staged)
    {
        return Err(store_error("retained owner authorization records differ"));
    }
    Ok(Some((
        owner.operation_digest(),
        owner.request_digest(),
        request,
        retained.phase,
    )))
}

pub(crate) fn load_unclaimed_retained_owner_request(
    transaction: &RetainedOwnerTransaction,
    directory: &Path,
    claims: &Path,
    manifests: &Path,
    tombstones: &Path,
    quarantined_identities: &Path,
    expected: &ExecutionManifest,
) -> Result<Option<Vec<u8>>, ProcessError> {
    let process_id = transaction.process_id();
    let name = hex(process_id.as_bytes());
    if expected.identity.process_id() != process_id {
        return Err(store_error("unclaimed retained owner expectation differs"));
    }
    if load_quarantined_identity(quarantined_identities, process_id)?
        || claims.join(format!("{name}.claim")).exists()
        || manifests.join(format!("{name}.manifest")).exists()
        || tombstones.join(format!("{name}.tombstone")).exists()
    {
        return Ok(None);
    }
    let request_path = directory.join(format!("{name}.{REQUEST_SUFFIX}"));
    let authorized_path = directory.join(format!("{name}.{AUTHORIZED_SUFFIX}"));
    let request = match read_regular_file(
        &request_path,
        "unclaimed retained owner request cannot be read",
    ) {
        Ok(request) => request,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            remove_unclaimed_stage(&authorized_path)?;
            sync_directory(directory)?;
            return Ok(None);
        }
        Err(error) => {
            return Err(store_cause(
                "unclaimed retained owner request cannot be read",
                error,
            ));
        }
    };
    let retained = match crate::RetainedOwnerRequest::decode(request.clone()) {
        Ok(retained) if retained.binding().process_id() == process_id => retained,
        Ok(_) | Err(_) => {
            remove_unclaimed_stage(&request_path)?;
            remove_unclaimed_stage(&authorized_path)?;
            sync_directory(directory)?;
            return Ok(None);
        }
    };
    match read_regular_file(
        &authorized_path,
        "unclaimed retained owner authorization cannot be read",
    ) {
        Ok(bytes) => {
            let authorized = ExecutionManifest::decode(&bytes);
            let exact = authorized.as_ref().is_ok_and(|manifest| {
                manifest.has_same_authorization(expected)
                    && manifest.identity.action_id()
                        == retained.execution_plan().identity().action_id()
                    && manifest.action_digest == retained.binding().action_digest()
                    && manifest.plan_digest == retained.execution_plan().digest()
            });
            if !exact {
                remove_unclaimed_stage(&authorized_path)?;
                sync_directory(directory)?;
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(store_cause(
                "unclaimed retained owner authorization cannot be read",
                error,
            ));
        }
    }
    Ok(Some(request))
}

pub(crate) fn discard_unclaimed_retained_owner_request(
    transaction: &RetainedOwnerTransaction,
    directory: &Path,
    claims: &Path,
    manifests: &Path,
    tombstones: &Path,
    quarantined_identities: &Path,
) -> Result<(), ProcessError> {
    let name = hex(transaction.process_id().as_bytes());
    if load_quarantined_identity(quarantined_identities, transaction.process_id())?
        || claims.join(format!("{name}.claim")).exists()
        || manifests.join(format!("{name}.manifest")).exists()
        || tombstones.join(format!("{name}.tombstone")).exists()
    {
        return Err(store_error(
            "consumed retained owner staging cannot be discarded",
        ));
    }
    remove_unclaimed_stage(&directory.join(format!("{name}.{REQUEST_SUFFIX}")))?;
    remove_unclaimed_stage(&directory.join(format!("{name}.{AUTHORIZED_SUFFIX}")))?;
    sync_directory(directory)
}

fn persist_immutable(path: &Path, bytes: &[u8]) -> Result<(), ProcessError> {
    let directory = path
        .parent()
        .ok_or_else(|| store_error("retained owner staging has no protected parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| store_cause("retained owner staging cannot be created", error))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| store_cause("retained owner staging cannot be synchronized", error))?;
    match temporary.persist_noclobber(path) {
        Ok(_) => sync_directory(directory),
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {
            let actual = read_regular_file(path, "retained owner staging cannot be reread")
                .map_err(|error| store_cause("retained owner staging cannot be reread", error))?;
            if actual != bytes {
                return Err(store_error("retained owner staging conflicts with its identity"));
            }
            File::open(path)
                .and_then(|file| file.sync_all())
                .map_err(|error| {
                    store_cause("retained owner staging cannot be resynchronized", error)
                })?;
            sync_directory(directory)
        }
        Err(error) => Err(store_cause(
            "retained owner staging cannot be published",
            error.error,
        )),
    }
}

fn remove_unclaimed_stage(path: &Path) -> Result<(), ProcessError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(store_cause(
            "unclaimed retained owner staging cannot be removed",
            error,
        )),
    }
}

fn read_regular_file(path: &Path, detail: &'static str) -> std::io::Result<Vec<u8>> {
    let path_metadata = fs::symlink_metadata(path)?;
    if !path_metadata.file_type().is_file() || path_metadata.file_type().is_symlink() {
        return Err(std::io::Error::other(detail));
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() {
        return Err(std::io::Error::other(detail));
    }
    let length = usize::try_from(metadata.len()).map_err(|_| std::io::Error::other(detail))?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|_| std::io::Error::other(detail))?;
    file.take(metadata.len().saturating_add(1)).read_to_end(&mut bytes)?;
    if bytes.len() != length {
        return Err(std::io::Error::other(detail));
    }
    Ok(bytes)
}

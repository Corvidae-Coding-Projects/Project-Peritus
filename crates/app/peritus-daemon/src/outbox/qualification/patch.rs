//! Real patch transaction recovery on both sides of durable application.

use std::fs;
use std::path::{Path, PathBuf};

use peritus_patch::{
    FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchPlan, PatchSet, WorkspacePath,
    apply_patch,
};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};

use crate::instance::InstanceGuard;
use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{
    acquire_instance, digest_hex, identifier, qualification_error, verify_empty_journal_for_store,
};

const QUALIFICATION_DIRECTORY: &str = "patch-crash-qualification-v1";
const TARGET: &str = "delivery.txt";
const FINAL_BYTES: &[u8] = b"Peritus patch transaction committed exactly once.\n";

/// Checkpoint holding a checked patch plan only in the killed process.
pub struct PatchBeforeCheckpoint {
    patch_sha256: String,
    target_sha256: String,
    _instance: InstanceGuard,
    _unsubmitted: PatchPlan,
}

impl PatchBeforeCheckpoint {
    pub(crate) fn patch_sha256(&self) -> &str {
        &self.patch_sha256
    }

    pub(crate) fn target_sha256(&self) -> &str {
        &self.target_sha256
    }
}

/// Durable production patch receipt observed before caller acknowledgement.
pub struct PatchAfterCheckpoint {
    patch_sha256: String,
    target_sha256: String,
    manifest_sha256: String,
    _instance: InstanceGuard,
}

impl PatchAfterCheckpoint {
    pub(crate) fn patch_sha256(&self) -> &str {
        &self.patch_sha256
    }

    pub(crate) fn target_sha256(&self) -> &str {
        &self.target_sha256
    }

    pub(crate) fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
}

/// Exact workspace facts recovered by a fresh daemon process.
pub struct PatchCrashQualification {
    patch_sha256: String,
    target_sha256: Option<String>,
    target_files: u64,
    pending_transactions: u64,
}

impl PatchCrashQualification {
    pub(crate) fn patch_sha256(&self) -> &str {
        &self.patch_sha256
    }

    pub(crate) fn target_sha256(&self) -> Option<&str> {
        self.target_sha256.as_deref()
    }

    pub(crate) const fn target_files(&self) -> u64 {
        self.target_files
    }

    pub(crate) const fn pending_transactions(&self) -> u64 {
        self.pending_transactions
    }

    pub(crate) const fn journal_verified(&self) -> bool {
        true
    }
}

/// Prepares the exact production plan without creating transaction state or changing the target.
pub fn stage_patch_before_crash(
    config: &DaemonConfig,
) -> Result<PatchBeforeCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let instance = acquire_instance(config, store_id)?;
    verify_empty_journal_for_store(config, store_id)?;
    let paths = QualificationPaths::prepare(config)?;
    let plan = plan(config)?;
    require_clean(&paths)?;
    Ok(PatchBeforeCheckpoint {
        patch_sha256: plan.identity().to_hex(),
        target_sha256: digest_hex(peritus_codec::sha256(FINAL_BYTES)),
        _instance: instance,
        _unsubmitted: plan,
    })
}

/// Applies a checked production patch and returns its durable receipt before acknowledgement.
pub fn stage_patch_after_crash(config: &DaemonConfig) -> Result<PatchAfterCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let instance = acquire_instance(config, store_id)?;
    verify_empty_journal_for_store(config, store_id)?;
    let paths = QualificationPaths::prepare(config)?;
    let plan = plan(config)?;
    require_clean(&paths)?;
    let applied = apply_patch(&paths.workspace, &paths.transactions, &plan).map_err(patch_error)?;
    if applied.identity() != plan.identity() || applied.cleanup_pending() {
        return Err(qualification_error(
            "applied patch receipt differs or cleanup remains pending",
        ));
    }
    let target_sha256 = target_digest(&paths)?.ok_or_else(|| {
        qualification_error("applied patch did not leave its exact regular target")
    })?;
    Ok(PatchAfterCheckpoint {
        patch_sha256: applied.identity().to_hex(),
        target_sha256,
        manifest_sha256: digest_hex(applied.manifest_digest()),
        _instance: instance,
    })
}

/// Proves the killed pre-commit plan left no target or transaction state.
pub fn recover_patch_before_crash(
    config: &DaemonConfig,
) -> Result<PatchCrashQualification, DaemonError> {
    recover(config, false)
}

/// Proves the committed patch postimage survived while transaction metadata was cleaned.
pub fn recover_patch_after_crash(
    config: &DaemonConfig,
) -> Result<PatchCrashQualification, DaemonError> {
    recover(config, true)
}

fn recover(config: &DaemonConfig, committed: bool) -> Result<PatchCrashQualification, DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let paths = QualificationPaths::prepare(config)?;
    let plan = plan(config)?;
    let target_sha256 = target_digest(&paths)?;
    let target_files = u64::from(target_sha256.is_some());
    let pending_transactions = entry_count(&paths.transactions)?;
    let expected_digest = digest_hex(peritus_codec::sha256(FINAL_BYTES));
    if pending_transactions != 0
        || target_files != u64::from(committed)
        || target_sha256.as_deref() != committed.then_some(expected_digest.as_str())
        || !verify_empty_journal_for_store(config, store_id)?
    {
        return Err(qualification_error("recovered patch state differs from the commit boundary"));
    }
    Ok(PatchCrashQualification {
        patch_sha256: plan.identity().to_hex(),
        target_sha256,
        target_files,
        pending_transactions,
    })
}

fn plan(config: &DaemonConfig) -> Result<PatchPlan, DaemonError> {
    let store_id = config.store_identity()?;
    let workspace_id = WorkspaceId::new(identifier(b"peritus/h1/patch-workspace/v1\0", store_id))
        .map_err(|_| qualification_error("derive patch workspace identity"))?;
    let final_file =
        FinalFile::new(FINAL_BYTES.to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
            .map_err(patch_error)?;
    let operation =
        PatchOperation::create(WorkspacePath::new(TARGET).map_err(patch_error)?, final_file);
    PatchSet::new(workspace_id, Generation::first(), RevisionNumber::first(), vec![operation])
        .and_then(|patch| patch.plan(workspace_id, Generation::first(), RevisionNumber::first()))
        .map_err(patch_error)
}

fn require_clean(paths: &QualificationPaths) -> Result<(), DaemonError> {
    if target_digest(paths)?.is_some() || entry_count(&paths.transactions)? != 0 {
        return Err(qualification_error("patch qualification roots are not empty"));
    }
    Ok(())
}

fn target_digest(paths: &QualificationPaths) -> Result<Option<String>, DaemonError> {
    let target = paths.workspace.join(TARGET);
    match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_file() => {
            let bytes = fs::read(target).map_err(io_error)?;
            Ok(Some(digest_hex(peritus_codec::sha256(&bytes))))
        }
        Ok(_) => Err(qualification_error("patch qualification target is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error(error)),
    }
}

fn entry_count(path: &Path) -> Result<u64, DaemonError> {
    fs::read_dir(path).map_err(io_error)?.try_fold(0_u64, |count, entry| {
        entry.map_err(io_error)?;
        count.checked_add(1).ok_or_else(|| qualification_error("patch entry count overflowed"))
    })
}

struct QualificationPaths {
    workspace: PathBuf,
    transactions: PathBuf,
}

impl QualificationPaths {
    fn prepare(config: &DaemonConfig) -> Result<Self, DaemonError> {
        let root = config.paths().state_root().join(QUALIFICATION_DIRECTORY);
        let workspace = root.join("workspace");
        let transactions = root.join("transactions");
        fs::create_dir_all(&workspace).map_err(io_error)?;
        fs::create_dir_all(&transactions).map_err(io_error)?;
        Ok(Self { workspace, transactions })
    }
}

fn patch_error(error: peritus_patch::PatchError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify patch transaction recovery",
        error.to_string(),
        error,
    )
}

fn io_error(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "inspect patch qualification state",
        error.to_string(),
        error,
    )
}

//! Private fixed-layout path derivation and directory synchronization.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{ArtifactDigest, ArtifactStoreError, ErrorCode, RecoveryClass, StoreOperation};

pub struct StorePaths {
    root: PathBuf,
    objects_sha256: PathBuf,
    temporary: PathBuf,
    quarantine_sha256: PathBuf,
    database: PathBuf,
}

impl StorePaths {
    pub(crate) fn initialize(
        root: &Path,
        configured_database: Option<&Path>,
    ) -> Result<Self, ArtifactStoreError> {
        fs::create_dir_all(root).map_err(|error| io(StoreOperation::Initialize, error))?;
        let root =
            fs::canonicalize(root).map_err(|error| io(StoreOperation::Canonicalize, error))?;
        if !fs::metadata(&root).map_err(|error| io(StoreOperation::Initialize, error))?.is_dir() {
            return Err(ArtifactStoreError::message(
                ErrorCode::InvalidConfiguration,
                RecoveryClass::CorrectRequest,
                "store root is not a directory",
            ));
        }
        let objects = fixed_directory(&root, "objects")?;
        let objects_sha256 = fixed_directory(&objects, "sha256")?;
        let temporary = fixed_directory(&root, "temporary")?;
        let quarantine = fixed_directory(&root, "quarantine")?;
        let quarantine_sha256 = fixed_directory(&quarantine, "sha256")?;
        let database =
            configured_database.map_or_else(|| root.join("metadata.sqlite3"), Path::to_path_buf);
        match fs::symlink_metadata(&database) {
            Ok(metadata) if !metadata.file_type().is_file() => return Err(layout_escape()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(StoreOperation::Initialize, error)),
        }
        sync_directory(&root)?;
        Ok(Self { root, objects_sha256, temporary, quarantine_sha256, database })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn database(&self) -> &Path {
        &self.database
    }
    pub(crate) fn temporary(&self) -> &Path {
        &self.temporary
    }
    pub(crate) fn quarantine_root(&self) -> &Path {
        &self.quarantine_sha256
    }
    pub(crate) fn objects_root(&self) -> &Path {
        &self.objects_sha256
    }

    pub(crate) fn object(&self, digest: ArtifactDigest) -> PathBuf {
        digest_path(&self.objects_sha256, digest)
    }

    pub(crate) fn quarantine(&self, digest: ArtifactDigest) -> PathBuf {
        digest_path(&self.quarantine_sha256, digest)
    }

    pub(crate) fn ensure_object_parent(
        &self,
        digest: ArtifactDigest,
    ) -> Result<PathBuf, ArtifactStoreError> {
        ensure_digest_parent(&self.objects_sha256, digest)
    }

    pub(crate) fn ensure_quarantine_parent(
        &self,
        digest: ArtifactDigest,
    ) -> Result<PathBuf, ArtifactStoreError> {
        ensure_digest_parent(&self.quarantine_sha256, digest)
    }
}

fn digest_path(base: &Path, digest: ArtifactDigest) -> PathBuf {
    let hex = digest.to_hex();
    base.join(&hex[..2]).join(hex)
}

fn ensure_digest_parent(
    base: &Path,
    digest: ArtifactDigest,
) -> Result<PathBuf, ArtifactStoreError> {
    let hex = digest.to_hex();
    let parent = base.join(&hex[..2]);
    if !parent.exists() {
        fs::create_dir(&parent).map_err(|error| io(StoreOperation::Initialize, error))?;
        sync_directory(base)?;
    }
    let canonical =
        fs::canonicalize(&parent).map_err(|error| io(StoreOperation::Canonicalize, error))?;
    if canonical.parent() != Some(base) {
        return Err(layout_escape());
    }
    Ok(canonical)
}

fn fixed_directory(parent: &Path, name: &str) -> Result<PathBuf, ArtifactStoreError> {
    let path = parent.join(name);
    fs::create_dir_all(&path).map_err(|error| io(StoreOperation::Initialize, error))?;
    let canonical =
        fs::canonicalize(&path).map_err(|error| io(StoreOperation::Canonicalize, error))?;
    if canonical.parent() != Some(parent) {
        return Err(layout_escape());
    }
    Ok(canonical)
}

const fn layout_escape() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::InvalidConfiguration,
        RecoveryClass::TerminalIntegrity,
        "fixed store directory resolves outside its expected parent",
    )
}

pub fn sync_directory(path: &Path) -> Result<(), ArtifactStoreError> {
    #[cfg(unix)]
    {
        let directory =
            fs::File::open(path).map_err(|error| io(StoreOperation::Synchronize, error))?;
        directory.sync_all().map_err(|error| io(StoreOperation::Synchronize, error))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

pub fn io(operation: StoreOperation, error: std::io::Error) -> ArtifactStoreError {
    ArtifactStoreError::io(operation, error)
}

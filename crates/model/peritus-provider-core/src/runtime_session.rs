//! Exclusive, durable task/role/provider namespaces for official native runtimes.

use crate::ProviderCoreError;
use peritus_model_protocol::ModelRequest;
use sha2::{Digest as _, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

/// Retains native runtime files for one host-bound task, role, profile revision, and model.
///
/// This owns storage and exclusion only. It never grants model continuation or tool authority.
pub struct RuntimeSession {
    root: Option<PathBuf>,
    _owner: Option<File>,
}

/// One private native invocation's artifacts, retained when the host supplied a namespace.
pub enum RuntimeTurnDirectory {
    /// An unbound provider request has no host task to resume.
    Temporary(tempfile::TempDir),
    /// A bound host task retains evidence across process failure and host restart.
    Persistent(PathBuf),
}

impl RuntimeTurnDirectory {
    /// Returns the owned artifact directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::Temporary(directory) => directory.path(),
            Self::Persistent(path) => path,
        }
    }
}

impl RuntimeSession {
    /// Acquires one exclusive native session namespace from host-owned request metadata.
    ///
    /// # Errors
    /// Rejects relative namespaces, failed storage, or an already owned lineage.
    pub fn open(request: &ModelRequest) -> Result<Self, ProviderCoreError> {
        let Some(directory) = request.local_session_directory() else {
            return Ok(Self { root: None, _owner: None });
        };
        if !directory.is_absolute() {
            return Err(error("native session namespace must be absolute"));
        }
        let mut route = request.profile_id().as_bytes().to_vec();
        route.extend_from_slice(&request.profile_revision().to_be_bytes());
        route.extend_from_slice(request.model().as_str().as_bytes());
        let mut name = String::new();
        for byte in Sha256::digest(route) {
            use std::fmt::Write as _;
            write!(name, "{byte:02x}").map_err(|_| error("cannot encode native route"))?;
        }
        let root = directory.join(name);
        fs::create_dir_all(&root).map_err(|_| error("cannot create native session namespace"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .map_err(|_| error("cannot protect native session namespace"))?;
        }
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("owner.lock"))
            .map_err(|_| error("cannot open native session owner"))?;
        owner
            .try_lock()
            .map_err(|_| error("native session already has an owner or cannot be locked"))?;
        Ok(Self { root: Some(root), _owner: Some(owner) })
    }

    /// Returns the stable isolated working directory, if this request has a host lineage.
    #[must_use]
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Creates an invocation directory without deleting bound task artifacts on drop.
    ///
    /// # Errors
    /// Returns a redacted storage failure.
    pub fn turn_directory(&self) -> Result<RuntimeTurnDirectory, ProviderCoreError> {
        match &self.root {
            Some(root) => Ok(RuntimeTurnDirectory::Persistent(
                tempfile::Builder::new()
                    .prefix("turn-")
                    .tempdir_in(root)
                    .map_err(|_| error("cannot create retained native turn"))?
                    .keep(),
            )),
            None => Ok(RuntimeTurnDirectory::Temporary(
                tempfile::tempdir().map_err(|_| error("cannot create native turn"))?,
            )),
        }
    }
}

const fn error(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::configuration("native_runtime_session", detail)
}

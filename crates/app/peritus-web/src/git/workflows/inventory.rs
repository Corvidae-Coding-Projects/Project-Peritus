//! Progressive, snapshot-bound Git inventory pages with direct durable receipts.

mod materialize;
mod page;

use crate::{
    error::{Result, problem, uncertain},
    git::effects::{self, OwnerBinding},
    state::{App, hex, id, save},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(super) const SCHEMA_VERSION: u16 = 3;
pub(super) const PAGE_RECORDS: usize = 256;
pub(super) const PAGE_BYTES: usize = 256 * 1_024;
pub(super) const OWNER_FLAG: &str = "--peritus-git-inventory-owner";

pub(in crate::git) struct InventoryPage {
    pub(in crate::git) branches: Vec<Value>,
    pub(in crate::git) remotes: Vec<Value>,
    pub(in crate::git) summary: String,
    pub(in crate::git) cursor: Option<String>,
    pub(in crate::git) snapshot: String,
}

#[derive(Clone)]
pub(super) struct InventoryStore {
    pub(super) root: PathBuf,
    pub(super) workspace: String,
    pub(super) identity: String,
    pub(super) repository: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MaterializationRequest {
    pub(super) schema_version: u16,
    pub(super) workspace: String,
    pub(super) store: String,
    pub(super) repository: PathBuf,
    pub(super) snapshot: String,
    pub(super) job_name: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MaterializationPhase {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MaterializationState {
    pub(super) schema_version: u16,
    pub(super) workspace: String,
    pub(super) store: String,
    pub(super) repository: PathBuf,
    pub(super) snapshot: String,
    pub(super) owner: OwnerBinding,
    pub(super) phase: MaterializationPhase,
    pub(super) pages: u64,
    pub(super) bytes: u64,
    pub(super) digest: Option<String>,
    pub(super) error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PageReceipt {
    pub(super) schema_version: u16,
    pub(super) snapshot: String,
    pub(super) index: u64,
    pub(super) offset: u64,
    pub(super) bytes: u64,
    pub(super) records: usize,
    pub(super) digest: String,
}

pub(crate) async fn inventory(
    app: &App,
    root: &Path,
    cursor: Option<&str>,
    snapshot: Option<&str>,
) -> Result<InventoryPage> {
    if cursor.is_some() != snapshot.is_some() {
        return Err(problem("Git inventory cursors require their original materialization"));
    }
    let store = InventoryStore::open(app, root)?;
    match (cursor, snapshot) {
        (None, None) => {
            let (snapshot, mut child) = materialize::launch(&store).await?;
            let result = page::read(&store, &snapshot, 0, Some(&mut child)).await;
            if result.is_ok() {
                // The sidecar owns the materialization after its first durable page. Dropping the
                // gateway runtime must not turn a successful progressive response into a kill.
                child.disarm();
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
            }
            result
        }
        (Some(cursor), Some(snapshot)) => {
            let index = cursor
                .parse::<u64>()
                .map_err(|_| problem("The Git inventory cursor is malformed"))?;
            page::read(&store, snapshot, index, None).await
        }
        _ => Err(problem("Git inventory cursors require their original materialization")),
    }
}

pub(crate) fn owner_argument() -> Option<PathBuf> {
    let mut args = std::env::args_os();
    let _program = args.next()?;
    if args.next()?.to_str()? != OWNER_FLAG {
        return None;
    }
    let path = PathBuf::from(args.next()?);
    args.next().is_none().then_some(path)
}

pub(crate) fn run_owner(request_path: &Path) -> Result<()> {
    materialize::run(request_path)
}

impl InventoryStore {
    fn open(app: &App, repository: &Path) -> Result<Self> {
        let workspace = app.snapshot()?.identity;
        let state_file = app.options.state_file.canonicalize()?;
        let repository = repository.canonicalize()?;
        let mut owner = Sha256::new();
        owner.update(b"peritus-web-git-inventory-store-v3\0");
        owner.update(workspace.as_bytes());
        owner.update(b"\0");
        owner.update(state_file.to_string_lossy().as_bytes());
        let identity = hex(&owner.finalize());
        let repository_identity = hex(&Sha256::digest(repository.to_string_lossy().as_bytes()));
        let root = state_file
            .parent()
            .ok_or_else(|| problem("The gateway state file has no parent directory"))?
            .join("git-inventories")
            .join(&identity)
            .join(repository_identity);
        Ok(Self { root, workspace, identity, repository })
    }

    pub(super) fn from_request(path: &Path, request: &MaterializationRequest) -> Result<Self> {
        let snapshot_directory = path
            .parent()
            .ok_or_else(|| problem("The Git inventory request has no snapshot directory"))?;
        let snapshots = snapshot_directory
            .parent()
            .ok_or_else(|| problem("The Git inventory request has no snapshots directory"))?;
        let root = snapshots
            .parent()
            .ok_or_else(|| problem("The Git inventory request has no store directory"))?
            .to_owned();
        let repository_identity = hex(&Sha256::digest(
            request.repository.to_string_lossy().as_bytes(),
        ));
        if snapshots.file_name().and_then(|name| name.to_str()) != Some("snapshots")
            || root.file_name().and_then(|name| name.to_str())
                != Some(repository_identity.as_str())
            || root
                .parent()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                != Some(request.store.as_str())
            || root
                .parent()
                .and_then(Path::parent)
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str())
                != Some("git-inventories")
        {
            return Err(uncertain(
                "The Git inventory request is outside its repository-bound store",
            ));
        }
        let store = Self {
            root,
            workspace: request.workspace.clone(),
            identity: request.store.clone(),
            repository: request.repository.clone(),
        };
        store.validate_request(request, snapshot_directory)?;
        Ok(store)
    }

    pub(super) fn create_request(&self) -> Result<(MaterializationRequest, PathBuf)> {
        std::fs::create_dir_all(self.root.join("snapshots"))?;
        loop {
            let snapshot = id()?;
            let directory = self.root.join("snapshots").join(&snapshot);
            match std::fs::create_dir(&directory) {
                Ok(()) => {
                    std::fs::create_dir(directory.join("pages"))?;
                    let request = MaterializationRequest {
                        schema_version: SCHEMA_VERSION,
                        workspace: self.workspace.clone(),
                        store: self.identity.clone(),
                        repository: self.repository.clone(),
                        job_name: format!("Local\\PeritusGitInventory-{}-{snapshot}", self.identity),
                        snapshot,
                    };
                    let path = directory.join("request.json");
                    save(&path, &serde_json::to_vec(&request)?)?;
                    return Ok((request, path));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub(super) fn validate_request(
        &self,
        request: &MaterializationRequest,
        directory: &Path,
    ) -> Result<()> {
        let expected = self.root.join("snapshots").join(&request.snapshot);
        if request.schema_version != SCHEMA_VERSION
            || request.workspace != self.workspace
            || request.store != self.identity
            || request.repository != self.repository
            || request.snapshot.is_empty()
            || request.job_name
                != format!("Local\\PeritusGitInventory-{}-{}", self.identity, request.snapshot)
            || directory != expected
        {
            return Err(uncertain("The Git inventory request does not match its owning store"));
        }
        Ok(())
    }

    pub(super) fn directory(&self, snapshot: &str) -> Result<PathBuf> {
        if snapshot.is_empty()
            || !snapshot.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(problem("The Git inventory snapshot identity is malformed"));
        }
        Ok(self.root.join("snapshots").join(snapshot))
    }
}

pub(super) fn validate_state(
    request: &MaterializationRequest,
    state: &MaterializationState,
) -> Result<()> {
    if state.schema_version != SCHEMA_VERSION
        || state.workspace != request.workspace
        || state.store != request.store
        || state.repository != request.repository
        || state.snapshot != request.snapshot
        || (state.phase == MaterializationPhase::Completed) != state.digest.is_some()
        || (state.phase == MaterializationPhase::Failed) != state.error.is_some()
    {
        return Err(uncertain("The Git inventory state does not match its request"));
    }
    effects::validate_detached_owner(&state.owner, &request.job_name)
}

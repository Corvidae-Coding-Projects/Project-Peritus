//! Durable, snapshot-bound directory indexes with opaque keyset cursors.

mod materialize;
mod page;

use crate::{
    error::{Result, problem, uncertain},
    git::effects::{self, OwnerBinding},
    state::{hex, id, save},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

const SCHEMA_VERSION: u16 = 1;
const PAGE_SIZE: u64 = 250;
const MATERIALIZATION_BATCH: usize = 256;
const OWNER_FLAG: &str = "--peritus-directory-index-owner";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    name: String,
    path: String,
    directory: bool,
    directory_identity: Option<String>,
    symlink: bool,
    bytes: u64,
}

#[derive(Clone)]
struct ListingStore {
    root: PathBuf,
    workspace: String,
    identity: String,
    state_file: PathBuf,
    project_root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MaterializationRequest {
    schema_version: u16,
    workspace: String,
    store: String,
    state_file: PathBuf,
    project_root: PathBuf,
    directory: PathBuf,
    directory_identity: String,
    relative: String,
    filter: String,
    materialization: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum MaterializationPhase {
    Pending,
    Indexing,
    Ordering,
    Completed,
    Failed,
}

impl MaterializationPhase {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Indexing => "indexing",
            Self::Ordering => "ordering",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MaterializationState {
    schema_version: u16,
    workspace: String,
    store: String,
    materialization: String,
    directory_identity: String,
    filter: String,
    owner: Option<OwnerBinding>,
    phase: MaterializationPhase,
    scanned: u64,
    matched: u64,
    digest: Option<String>,
    error: Option<String>,
}

impl MaterializationState {
    fn pending(request: &MaterializationRequest) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            workspace: request.workspace.clone(),
            store: request.store.clone(),
            materialization: request.materialization.clone(),
            directory_identity: request.directory_identity.clone(),
            filter: request.filter.clone(),
            owner: None,
            phase: MaterializationPhase::Pending,
            scanned: 0,
            matched: 0,
            digest: None,
            error: None,
        }
    }
}

/// Reads or starts one bounded directory page. Materialization work belongs to a durable owner
/// process; an indexing response contains a cursor that may be polled without restarting work.
#[expect(
    clippy::too_many_arguments,
    reason = "the HTTP observation binds every project, directory, query and cursor identity"
)]
pub fn list_page(
    state_file: &Path,
    workspace: &str,
    root: &Path,
    relative: &str,
    expected_directory: &str,
    offset: u64,
    filter: &str,
    cursor: &str,
    expected_snapshot: &str,
) -> Result<serde_json::Value> {
    page::list(
        state_file,
        workspace,
        root,
        relative,
        expected_directory,
        offset,
        filter,
        cursor,
        expected_snapshot,
    )
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

impl ListingStore {
    fn open(state_file: &Path, workspace: &str, project_root: &Path) -> Result<Self> {
        if workspace.is_empty() {
            return Err(problem("The directory index has no gateway workspace identity"));
        }
        let state_file = state_file.canonicalize()?;
        let project_root = project_root.canonicalize()?;
        if !project_root.is_dir() {
            return Err(problem("The directory index project root is unavailable"));
        }
        let mut digest = Sha256::new();
        digest.update(b"peritus-web-directory-index-store-v1\0");
        digest.update(workspace.as_bytes());
        digest.update(b"\0");
        update_path_digest(&mut digest, &state_file);
        digest.update(b"\0");
        update_path_digest(&mut digest, &project_root);
        let identity = hex(&digest.finalize());
        let parent = state_file
            .parent()
            .ok_or_else(|| problem("The gateway state file has no parent directory"))?;
        let root = parent.join("directory-indexes").join(&identity);
        Ok(Self {
            root,
            workspace: workspace.to_owned(),
            identity,
            state_file,
            project_root,
        })
    }

    fn from_request(path: &Path, request: &MaterializationRequest) -> Result<Self> {
        let store = Self::open(
            &request.state_file,
            &request.workspace,
            &request.project_root,
        )?;
        let directory = path
            .parent()
            .ok_or_else(|| problem("The directory-index request has no materialization directory"))?;
        if path.file_name().and_then(|name| name.to_str()) != Some("request.json")
            || request.schema_version != SCHEMA_VERSION
            || request.store != store.identity
            || request.materialization.is_empty()
            || directory != store.materialization_directory(&request.materialization)?
        {
            return Err(uncertain(
                "The directory-index request does not match its owning store",
            ));
        }
        store.validate_request(request)?;
        Ok(store)
    }

    fn create_request(
        &self,
        relative: &str,
        expected_directory: &str,
        filter: &str,
    ) -> Result<(MaterializationRequest, PathBuf)> {
        let directory = super::resolve(&self.project_root, relative)?;
        if !directory.is_dir() {
            return Err(problem("Choose a directory to browse"));
        }
        let directory_identity = path_identity(&directory);
        if !expected_directory.is_empty() && expected_directory != directory_identity {
            return Err(problem(
                "The requested directory changed identity. Refresh the explorer.",
            ));
        }
        std::fs::create_dir_all(self.root.join("materializations"))?;
        loop {
            let materialization = id()?;
            let directory_path = self.root.join("materializations").join(&materialization);
            match std::fs::create_dir(&directory_path) {
                Ok(()) => {
                    let request = MaterializationRequest {
                        schema_version: SCHEMA_VERSION,
                        workspace: self.workspace.clone(),
                        store: self.identity.clone(),
                        state_file: self.state_file.clone(),
                        project_root: self.project_root.clone(),
                        directory,
                        directory_identity,
                        relative: relative.to_owned(),
                        filter: filter.to_owned(),
                        materialization,
                    };
                    let path = directory_path.join("request.json");
                    save(&path, &serde_json::to_vec(&request)?)?;
                    save_state(&directory_path, &MaterializationState::pending(&request))?;
                    return Ok((request, path));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn request(&self, materialization: &str) -> Result<MaterializationRequest> {
        let path = self.materialization_directory(materialization)?.join("request.json");
        let request: MaterializationRequest = serde_json::from_slice(&std::fs::read(&path)?)?;
        self.validate_request(&request)?;
        Ok(request)
    }

    fn validate_request(&self, request: &MaterializationRequest) -> Result<()> {
        if request.schema_version != SCHEMA_VERSION
            || request.workspace != self.workspace
            || request.store != self.identity
            || request.state_file != self.state_file
            || request.project_root != self.project_root
            || request.materialization.is_empty()
            || request.directory_identity != path_identity(&request.directory)
            || !request.directory.starts_with(&request.project_root)
        {
            return Err(uncertain(
                "The directory-index request differs from its durable source binding",
            ));
        }
        Ok(())
    }

    fn materialization_directory(&self, materialization: &str) -> Result<PathBuf> {
        validate_hex_identity(materialization, "directory materialization")?;
        Ok(self.root.join("materializations").join(materialization))
    }

    fn state(&self, materialization: &str) -> Result<MaterializationState> {
        let directory = self.materialization_directory(materialization)?;
        serde_json::from_slice(&std::fs::read(directory.join("state.json"))?).map_err(Into::into)
    }

    fn owner(&self, materialization: &str) -> Result<File> {
        let path = self.materialization_directory(materialization)?.join("owner.lock");
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(Into::into)
    }

    fn activation(&self, materialization: &str) -> Result<File> {
        let path = self.materialization_directory(materialization)?.join("activation.lock");
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(Into::into)
    }
}

fn validate_state(
    request: &MaterializationRequest,
    state: &MaterializationState,
) -> Result<()> {
    let terminal = matches!(
        state.phase,
        MaterializationPhase::Completed | MaterializationPhase::Failed
    );
    if state.schema_version != SCHEMA_VERSION
        || state.workspace != request.workspace
        || state.store != request.store
        || state.materialization != request.materialization
        || state.directory_identity != request.directory_identity
        || state.filter != request.filter
        || state.matched > state.scanned
        || (state.phase != MaterializationPhase::Pending && state.owner.is_none())
        || (state.phase == MaterializationPhase::Completed) != state.digest.is_some()
        || (state.phase == MaterializationPhase::Failed) != state.error.is_some()
        || (!terminal && (state.digest.is_some() || state.error.is_some()))
    {
        return Err(uncertain(
            "The directory-index state differs from its durable source binding",
        ));
    }
    if let Some(digest) = &state.digest {
        validate_digest(digest, "directory snapshot")?;
    }
    if let Some(owner) = &state.owner {
        effects::validate_detached_launcher(owner)?;
    }
    Ok(())
}

fn save_state(directory: &Path, state: &MaterializationState) -> Result<()> {
    save(&directory.join("state.json"), &serde_json::to_vec(state)?)
}

fn validate_hex_identity(value: &str, subject: &str) -> Result<()> {
    if value.len() != 32
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(problem(format!("The {subject} identity is malformed")));
    }
    Ok(())
}

fn validate_digest(value: &str, subject: &str) -> Result<()> {
    if value.len() != 64
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(problem(format!("The {subject} digest is malformed")));
    }
    Ok(())
}

fn path_identity(path: &Path) -> String {
    let mut digest = Sha256::new();
    update_path_digest(&mut digest, path);
    hex(&digest.finalize())
}

fn update_path_digest(digest: &mut Sha256, path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        digest.update(path.as_os_str().as_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        for unit in path.as_os_str().encode_wide() {
            digest.update(unit.to_le_bytes());
        }
    }
    #[cfg(not(any(unix, windows)))]
    digest.update(path.as_os_str().to_string_lossy().as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtering_covers_the_whole_directory_and_later_pages_require_the_same_snapshot() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..=PAGE_SIZE {
            std::fs::write(root.path().join(format!("entry-{index:03}.txt")), "entry").unwrap();
        }
        std::fs::write(root.path().join("zzzz-needle.txt"), "needle").unwrap();

        let filtered = list_page(root.path(), "", 0, "needle", "").unwrap();
        assert_eq!(filtered["total"], 1);
        assert_eq!(filtered["entries"][0]["name"], "zzzz-needle.txt");

        let first = list_page(root.path(), "", 0, "", "").unwrap();
        let snapshot = first["snapshot"].as_str().unwrap();
        assert_eq!(first["next"], PAGE_SIZE);
        std::fs::write(root.path().join("entry-new.txt"), "new").unwrap();
        assert!(list_page(root.path(), "", PAGE_SIZE, "", snapshot).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn canonical_identities_distinguish_legitimate_aliases_from_ancestor_cycles() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("real")).unwrap();
        std::os::unix::fs::symlink("real", root.path().join("alias")).unwrap();
        std::os::unix::fs::symlink("..", root.path().join("real/back")).unwrap();

        let root_page = list_page(root.path(), "", 0, "", "").unwrap();
        let root_identity = root_page["directory"].as_str().unwrap();
        let entries = root_page["entries"].as_array().unwrap();
        let real = entries.iter().find(|entry| entry["name"] == "real").unwrap();
        let alias = entries.iter().find(|entry| entry["name"] == "alias").unwrap();
        assert_eq!(real["directoryIdentity"], alias["directoryIdentity"]);
        assert_ne!(real["directoryIdentity"], root_identity);

        let real_page = list_page(root.path(), "real", 0, "", "").unwrap();
        let back = real_page["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == "back")
            .unwrap();
        assert_eq!(back["directoryIdentity"], root_identity);
    }
}

//! Immutable-generation product state and atomic local publication.

use std::{
    fs::{self, File, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

use peritus_product_state::ProductState;

use crate::LauncherError;

pub struct ProductStateStore {
    root: PathBuf,
}

impl ProductStateStore {
    pub fn open(root: PathBuf) -> Result<Self, LauncherError> {
        fs::create_dir_all(&root).map_err(|error| {
            LauncherError::filesystem("create product-state directory", &root, error)
        })?;
        protect_directory(&root)?;
        Ok(Self { root })
    }

    pub fn load_or_initialize(&self) -> Result<ProductState, LauncherError> {
        if let Some(mut state) = self.load_latest()? {
            if state.migrate_legacy_workspace_storage()? {
                self.commit(&state)?;
            }
            return Ok(state);
        }
        let state = ProductState::new(crate::identity::generate()?);
        self.commit(&state)?;
        Ok(state)
    }

    pub fn commit(&self, state: &ProductState) -> Result<(), LauncherError> {
        let bytes = state.canonical_json()?;
        let final_path = self.generation_path(state.generation());
        if final_path.exists() {
            let existing = fs::read(&final_path).map_err(|error| {
                LauncherError::filesystem("read product-state generation", &final_path, error)
            })?;
            if existing == bytes {
                return Ok(());
            }
            return Err(LauncherError::PlatformPaths(format!(
                "product-state generation {} already exists with different content",
                state.generation()
            )));
        }
        publish_new(&self.root.join("state.pending"), &final_path, &bytes)?;
        Ok(())
    }

    fn load_latest(&self) -> Result<Option<ProductState>, LauncherError> {
        let entries = fs::read_dir(&self.root).map_err(|error| {
            LauncherError::filesystem("list product-state generations", &self.root, error)
        })?;
        let mut generations = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    eprintln!(
                        "peritus launcher: skipped an unreadable product-state directory entry: {error}"
                    );
                    continue;
                }
            };
            if let Some(generation) = parse_generation(&entry.file_name().to_string_lossy()) {
                generations.push((generation, entry.path()));
            }
        }
        generations.sort_unstable_by_key(|(generation, _)| std::cmp::Reverse(*generation));
        if generations.is_empty() {
            return Ok(None);
        }
        let mut rejected = Vec::new();
        for (generation, path) in generations {
            let parsed = fs::read(&path)
                .map_err(|error| error.to_string())
                .and_then(|bytes| {
                    ProductState::parse_json(&bytes).map_err(|error| error.to_string())
                })
                .and_then(|state| {
                    let payload_generation = state.generation();
                    if payload_generation == generation {
                        Ok(state)
                    } else {
                        Err(format!(
                            "filename generation {generation} does not match payload generation {payload_generation}"
                        ))
                    }
                });
            match parsed {
                Ok(state) => {
                    if !rejected.is_empty() {
                        eprintln!(
                            "peritus launcher: recovered product state from generation {generation} after skipping {} newer invalid generation(s)",
                            rejected.len()
                        );
                    }
                    return Ok(Some(state));
                }
                Err(detail) => {
                    eprintln!(
                        "peritus launcher: skipped invalid product-state generation {}: {detail}",
                        path.display()
                    );
                    rejected.push(path);
                }
            }
        }
        Err(LauncherError::PlatformPaths(format!(
            "no readable valid product-state generation remains under {}",
            self.root.display()
        )))
    }

    fn generation_path(&self, generation: u64) -> PathBuf {
        self.root.join(format!("state-{generation:020}.json"))
    }
}

pub fn publish_new(
    pending_path: &Path,
    final_path: &Path,
    bytes: &[u8],
) -> Result<(), LauncherError> {
    if pending_path.exists() {
        fs::remove_file(pending_path).map_err(|error| {
            LauncherError::filesystem("remove interrupted publication", pending_path, error)
        })?;
    }
    let mut pending =
        OpenOptions::new().create_new(true).write(true).open(pending_path).map_err(|error| {
            LauncherError::filesystem("create pending publication", pending_path, error)
        })?;
    protect_file(&pending, pending_path)?;
    pending.write_all(bytes).and_then(|()| pending.sync_all()).map_err(|error| {
        LauncherError::filesystem("write pending publication", pending_path, error)
    })?;
    drop(pending);
    fs::rename(pending_path, final_path)
        .map_err(|error| LauncherError::filesystem("publish durable file", final_path, error))?;
    sync_parent(final_path)?;
    Ok(())
}

pub fn read_exact_or_publish(path: &Path, bytes: &[u8]) -> Result<Vec<u8>, LauncherError> {
    match fs::read(path) {
        Ok(existing) => Ok(existing),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let pending = path.with_extension("pending");
            publish_new(&pending, path, bytes)?;
            Ok(bytes.to_vec())
        }
        Err(error) => Err(LauncherError::filesystem("read durable file", path, error)),
    }
}

/// Replaces one application-owned mutable recovery file and synchronizes it before returning.
pub fn replace_recovery_file(path: &Path, bytes: &[u8]) -> Result<(), LauncherError> {
    let mut file = OpenOptions::new()
        .create(false)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| LauncherError::filesystem("open recovery file", path, error))?;
    protect_file(&file, path)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| LauncherError::filesystem("replace recovery file", path, error))?;
    sync_parent(path)
}

fn parse_generation(name: &str) -> Option<u64> {
    name.strip_prefix("state-")?.strip_suffix(".json")?.parse().ok()
}

#[cfg(unix)]
fn protect_directory(path: &Path) -> Result<(), LauncherError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| LauncherError::filesystem("protect product-state directory", path, error))
}

#[cfg(windows)]
#[allow(
    clippy::unnecessary_wraps,
    reason = "keeps the platform implementations behind one fallible directory-protection contract"
)]
const fn protect_directory(_path: &Path) -> Result<(), LauncherError> {
    Ok(())
}

#[cfg(unix)]
pub fn protect_file(file: &File, path: &Path) -> Result<(), LauncherError> {
    use std::os::unix::fs::PermissionsExt as _;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| LauncherError::filesystem("protect durable file", path, error))
}

#[cfg(windows)]
#[allow(
    clippy::unnecessary_wraps,
    reason = "keeps the platform implementations behind one fallible file-protection contract"
)]
pub const fn protect_file(_file: &File, _path: &Path) -> Result<(), LauncherError> {
    Ok(())
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), LauncherError> {
    let parent = path.parent().ok_or_else(|| {
        LauncherError::PlatformPaths("durable publication has no parent directory".to_owned())
    })?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| LauncherError::filesystem("synchronize durable directory", parent, error))
}

#[cfg(windows)]
#[allow(
    clippy::unnecessary_wraps,
    reason = "keeps the platform implementations behind one fallible durable-sync contract"
)]
const fn sync_parent(_path: &Path) -> Result<(), LauncherError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_invalid_generation_falls_back_to_last_valid_state() {
        let root = tempfile::tempdir().expect("state root");
        let store = ProductStateStore::open(root.path().to_owned()).expect("store");
        let expected = ProductState::new(crate::identity::generate().expect("identity"));
        store.commit(&expected).expect("valid generation");
        fs::write(root.path().join("state-18000000000000000000.json"), b"{broken")
            .expect("newer corrupt generation");

        let recovered = store.load_or_initialize().expect("fallback state");

        assert_eq!(recovered.generation(), expected.generation());
        assert_eq!(recovered.identity(), expected.identity());
    }
}

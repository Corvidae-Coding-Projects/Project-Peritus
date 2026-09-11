//! Exact-file in-place comparison evidence. No Git and no recursive directory inventory.

use crate::{ProductRunnerError, ProductRunnerErrorKind, file_metadata};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    io::{BufRead as _, Read as _, Write as _},
    path::{Path, PathBuf},
};

const MAX_FILES: usize = 256;
const MAX_JOURNAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PREVIEW_BYTES: usize = 16 * 1024;

#[cfg(test)]
mod tests;

/// Durable scope descriptor, with a journal location supplied by the daemon-owned trace.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedBaseline {
    root: PathBuf,
    journal: PathBuf,
    protected: Vec<PathBuf>,
    revision: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    version: u8,
    scope: ScopedBaseline,
    path: String,
    before: FileStamp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FileStamp {
    kind: String,
    digest: Option<[u8; 32]>,
    permissions: Option<u32>,
    bytes: u64,
    preview: String,
}

impl ScopedBaseline {
    pub const fn new(
        root: PathBuf,
        journal: PathBuf,
        protected: Vec<PathBuf>,
        revision: u64,
    ) -> Self {
        Self { root, journal, protected, revision }
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn paths(&self) -> Result<Vec<PathBuf>, ProductRunnerError> {
        Ok(self.load()?.into_keys().map(PathBuf::from).collect())
    }

    pub(crate) fn progress_checkpoint(
        &self,
        root: &Path,
    ) -> Result<crate::progress::WorkspaceCheckpoint, ProductRunnerError> {
        crate::progress::WorkspaceCheckpoint::scoped(root, self.changed_paths(root)?)
    }

    pub fn changed_paths(&self, root: &Path) -> Result<Vec<PathBuf>, ProductRunnerError> {
        self.check_root(root)?;
        self.load()?
            .into_iter()
            .filter_map(|(path, before)| match self.stamp(&path) {
                Ok(current) if current == before => None,
                Ok(_) => Some(Ok(PathBuf::from(path))),
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    pub fn enroll(&self, path: &str) -> Result<(), ProductRunnerError> {
        let entries = self.load()?;
        if entries.contains_key(path) {
            return Ok(());
        }
        if entries.len() >= MAX_FILES {
            return Err(failure("in-place scope exceeds 256 exact files"));
        }
        let entry = Entry {
            version: 1,
            scope: self.clone(),
            path: path.to_owned(),
            before: self.stamp(path)?,
        };
        let mut bytes = serde_json::to_vec(&entry).map_err(|error| failure(error.to_string()))?;
        bytes.push(b'\n');
        let size = self.journal_size()?;
        if size.unwrap_or(0).saturating_add(bytes.len() as u64) > MAX_JOURNAL_BYTES {
            return Err(failure("in-place evidence journal exceeds its bound"));
        }
        let mut options = fs::OpenOptions::new();
        options.append(true);
        if size.is_none() {
            options.create_new(true);
        }
        let mut file = options.open(&self.journal).map_err(|error| failure(error.to_string()))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| failure(error.to_string()))?;
        #[cfg(unix)]
        if size.is_none() {
            let parent = self.journal.parent().ok_or_else(|| failure("journal has no parent"))?;
            fs::File::open(parent)
                .and_then(|file| file.sync_all())
                .map_err(|error| failure(error.to_string()))?;
        }
        Ok(())
    }

    pub fn diff(&self, root: &Path) -> Result<String, ProductRunnerError> {
        self.check_root(root)?;
        let mut diff = String::from(
            "In-place task-file comparison; not a whole-folder inventory or a Git candidate.\n",
        );
        for (path, before) in self.load()? {
            let after = self.stamp(&path)?;
            if before == after {
                continue;
            }
            let _ = writeln!(
                diff,
                "\n--- before/{path}\n+++ current/{path}\nkind: {} -> {}; permissions: {:?} -> {:?}; bytes: {} -> {}",
                before.kind,
                after.kind,
                before.permissions,
                after.permissions,
                before.bytes,
                after.bytes
            );
            for line in before.preview.lines() {
                let _ = writeln!(diff, "-{line}");
            }
            for line in after.preview.lines() {
                let _ = writeln!(diff, "+{line}");
            }
            if before.bytes > MAX_PREVIEW_BYTES as u64 || after.bytes > MAX_PREVIEW_BYTES as u64 {
                diff.push_str("[bounded preview; exact full-file digests used for freshness]\n");
            }
            if diff.len() > 512 * 1024 {
                return Ok(crate::bundle::limit_text(&diff, 512 * 1024));
            }
        }
        Ok(diff)
    }

    fn check_root(&self, root: &Path) -> Result<(), ProductRunnerError> {
        if root != self.root {
            return Err(failure("in-place evidence belongs to another directory"));
        }
        Ok(())
    }

    fn journal_size(&self) -> Result<Option<u64>, ProductRunnerError> {
        match fs::symlink_metadata(&self.journal) {
            Ok(meta)
                if meta.is_file()
                    && !meta.file_type().is_symlink()
                    && meta.len() <= MAX_JOURNAL_BYTES =>
            {
                Ok(Some(meta.len()))
            }
            Ok(_) => Err(failure("in-place evidence is not a bounded regular journal")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(failure(error.to_string())),
        }
    }

    fn load(&self) -> Result<BTreeMap<String, FileStamp>, ProductRunnerError> {
        let mut entries = BTreeMap::new();
        if self.journal_size()?.is_none() {
            return Ok(entries);
        }
        let file = fs::File::open(&self.journal).map_err(|error| failure(error.to_string()))?;
        let mut reader = std::io::BufReader::new(file);
        loop {
            let mut bytes = Vec::new();
            let count = reader
                .by_ref()
                .take(512 * 1024 + 1)
                .read_until(b'\n', &mut bytes)
                .map_err(|error| failure(error.to_string()))?;
            if count == 0 {
                break;
            }
            if count > 512 * 1024 || bytes.last() != Some(&b'\n') {
                return Err(failure(
                    "in-place evidence has a torn or oversized record; effects remain unverified",
                ));
            }
            let entry: Entry =
                serde_json::from_slice(&bytes).map_err(|error| failure(error.to_string()))?;
            self.checked_path(&entry.path)?;
            if entry.version != 1
                || entry.scope != *self
                || entries.len() >= MAX_FILES
                || entries.insert(entry.path, entry.before).is_some()
            {
                return Err(failure(
                    "in-place evidence has an invalid scope, version or duplicate path",
                ));
            }
        }
        Ok(entries)
    }

    fn checked_path(&self, relative: &str) -> Result<PathBuf, ProductRunnerError> {
        if relative.is_empty() || relative == "." {
            return Err(failure(
                "in-place evidence requires an exact file or empty-directory path",
            ));
        }
        crate::developer_tools::checked_protected_file(&self.root, relative, "", &self.protected)
            .map_err(|error| failure(error.to_string()))
    }

    fn stamp(&self, relative: &str) -> Result<FileStamp, ProductRunnerError> {
        let path = self.checked_path(relative)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(FileStamp {
                    kind: "absent".to_owned(),
                    digest: None,
                    permissions: None,
                    bytes: 0,
                    preview: String::new(),
                });
            }
            Err(error) => return Err(failure(error.to_string())),
        };
        let permissions = Some(file_metadata::permission_fingerprint(&metadata));
        if metadata.is_dir() {
            // An exact empty-directory removal can be tracked, but a directory declaration
            // cannot stand in for the files a command creates below it.
            if fs::read_dir(&path).map_err(|error| failure(error.to_string()))?.next().is_some() {
                return Err(failure("declare exact task files, not a nonempty directory"));
            }
            return Ok(FileStamp {
                kind: "directory".to_owned(),
                digest: None,
                permissions,
                bytes: 0,
                preview: String::new(),
            });
        }
        if !metadata.is_file() {
            return Err(failure("in-place evidence target is not a regular file"));
        }
        let mut file = fs::File::open(path).map_err(|error| failure(error.to_string()))?;
        let mut hasher = Sha256::new();
        let mut preview = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let count = file.read(&mut buffer).map_err(|error| failure(error.to_string()))?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            let remaining = MAX_PREVIEW_BYTES.saturating_sub(preview.len());
            preview.extend_from_slice(&buffer[..count.min(remaining)]);
        }
        Ok(FileStamp {
            kind: "file".to_owned(),
            digest: Some(hasher.finalize().into()),
            permissions,
            bytes: metadata.len(),
            preview: String::from_utf8(preview).unwrap_or_else(|_| "[binary content]".to_owned()),
        })
    }
}

pub fn definition() -> Result<peritus_model_protocol::ToolDefinition, ProductRunnerError> {
    crate::developer_tools::in_place_definition()
}

fn failure(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, "track in-place task files", detail)
}

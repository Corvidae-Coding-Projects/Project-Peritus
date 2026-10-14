//! In-place comparison evidence for explicit files and bounded declared subtrees. No Git.

use crate::{ProductRunnerError, ProductRunnerErrorKind, file_metadata};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    io::{BufRead as _, Read as _, Write as _},
    path::{Path, PathBuf},
};

const MAX_JOURNAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PREVIEW_BYTES: usize = 16 * 1024;
const MAX_SUBTREE_ENTRIES: usize = 4096;
const MAX_SUBTREE_DEPTH: usize = 64;

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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    descendants: BTreeMap<String, FileStamp>,
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
    /// Binds task evidence to a workspace, durable journal, protected paths and revision.
    /// The journal must live outside the declared task subtrees.
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

    /// Returns enrolled targets and currently discovered admissible descendants.
    ///
    /// # Errors
    /// Rejects invalid journals or inaccessible targets.
    pub fn paths(&self) -> Result<Vec<PathBuf>, ProductRunnerError> {
        Ok(self.comparison()?.into_keys().map(PathBuf::from).collect())
    }

    /// Exact predeclared file targets eligible for automatic before-image checkpoints.
    /// New descendants discovered after a command are comparison evidence, not before-images.
    pub(crate) fn checkpoint_paths(&self) -> Result<Vec<PathBuf>, ProductRunnerError> {
        self.load()?
            .into_iter()
            .filter_map(|(path, before)| match self.stamp(&path) {
                Ok(current)
                    if matches!(before.kind.as_str(), "file" | "absent")
                        && matches!(current.kind.as_str(), "file" | "absent") =>
                {
                    match self.observed_path(&path) {
                        Ok(Some(_)) => Some(Ok(PathBuf::from(path))),
                        Ok(None) => None,
                        Err(error) => Some(Err(error)),
                    }
                }
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    pub(crate) fn progress_checkpoint(
        &self,
        root: &Path,
    ) -> Result<crate::progress::WorkspaceCheckpoint, ProductRunnerError> {
        crate::progress::WorkspaceCheckpoint::scoped(root, self.changed_paths(root)?)
    }

    /// Compares exact file content, permissions and path kinds against durable baselines.
    ///
    /// # Errors
    /// Rejects a different workspace, invalid evidence or unreadable targets.
    pub fn changed_paths(&self, root: &Path) -> Result<Vec<PathBuf>, ProductRunnerError> {
        self.check_root(root)?;
        self.comparison()?
            .into_iter()
            .filter_map(|(path, before)| match self.stamp(&path) {
                Ok(current) if current == before => None,
                Ok(_) => Some(Ok(PathBuf::from(path))),
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    /// Durably enrolls a file or bounded directory before an effect, preserving earlier stamps.
    /// Protected paths, links and conventional build caches are excluded from subtree discovery.
    ///
    /// # Errors
    /// Rejects invalid/protected paths, oversized evidence or filesystem failures before effects.
    pub fn enroll(&self, path: &str) -> Result<(), ProductRunnerError> {
        let entries = self.comparison()?;
        if entries.contains_key(path) {
            return Ok(());
        }
        self.checked_path(path)?;
        let before = self.stamp(path)?;
        let descendants = if before.kind == "directory" {
            self.subtree_paths(path, true)?
                .into_iter()
                .map(|path| self.stamp(&path).map(|stamp| (path, stamp)))
                .collect::<Result<BTreeMap<_, _>, _>>()?
        } else {
            BTreeMap::new()
        };
        let entry =
            Entry { version: 2, scope: self.clone(), path: path.to_owned(), before, descendants };
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

    /// Renders a bounded comparison preview; exact full-file digests determine changes.
    ///
    /// # Errors
    /// Rejects a different workspace, invalid evidence or unreadable targets.
    pub fn diff(&self, root: &Path) -> Result<String, ProductRunnerError> {
        self.check_root(root)?;
        let mut diff = String::from(
            "In-place task-file comparison; not a whole-folder inventory or a Git candidate.\n",
        );
        for (path, before) in self.comparison()? {
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
        let mut declarations = BTreeSet::new();
        if self.journal_size()?.is_none() {
            return Ok(entries);
        }
        let file = fs::File::open(&self.journal).map_err(|error| failure(error.to_string()))?;
        let mut reader = std::io::BufReader::new(file);
        loop {
            let mut bytes = Vec::new();
            let count = reader
                .by_ref()
                .take(MAX_JOURNAL_BYTES + 1)
                .read_until(b'\n', &mut bytes)
                .map_err(|error| failure(error.to_string()))?;
            if count == 0 {
                break;
            }
            if count as u64 > MAX_JOURNAL_BYTES || bytes.last() != Some(&b'\n') {
                return Err(failure(
                    "in-place evidence has a torn or oversized record; effects remain unverified",
                ));
            }
            let entry: Entry =
                serde_json::from_slice(&bytes).map_err(|error| failure(error.to_string()))?;
            self.observed_path(&entry.path)?;
            if !matches!(entry.version, 1 | 2)
                || entry.scope != *self
                || !declarations.insert(entry.path.clone())
                || (entry.version == 1 && !entry.descendants.is_empty())
            {
                return Err(failure(
                    "in-place evidence has an invalid scope, version or duplicate path",
                ));
            }
            for (path, before) in entry.descendants {
                if path == entry.path || !Path::new(&path).starts_with(&entry.path) {
                    return Err(failure("directory evidence escaped its declared subtree"));
                }
                self.observed_path(&path)?;
                // Earlier exact-file enrollment always wins over a later parent declaration.
                entries.entry(path).or_insert(before);
            }
            entries.entry(entry.path).or_insert(entry.before);
        }
        Ok(entries)
    }

    fn checked_path(&self, relative: &str) -> Result<PathBuf, ProductRunnerError> {
        if relative.is_empty() || relative == "." {
            return Err(failure("in-place evidence requires a file or a scoped directory path"));
        }
        crate::developer_tools::checked_protected_file(&self.root, relative, "", &self.protected)
            .map_err(|error| failure(error.to_string()))
    }

    fn stamp(&self, relative: &str) -> Result<FileStamp, ProductRunnerError> {
        let Some(path) = self.observed_path(relative)? else { return Ok(absent()) };
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
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
            // Directory contents belong to individual descendant stamps. Population is a
            // normal lifecycle transition, including journals written by older releases.
            return Ok(FileStamp {
                kind: "directory".to_owned(),
                digest: None,
                permissions,
                bytes: 0,
                preview: String::new(),
            });
        }
        if !metadata.is_file() {
            // Runtime endpoints (sockets/pipes) and later link replacement are observable
            // effects, not file bodies. Do not open, read through, or restore them as files.
            let mut digest = Sha256::new();
            if metadata.file_type().is_symlink() {
                digest.update(
                    fs::read_link(&path)
                        .map_err(|error| failure(error.to_string()))?
                        .as_os_str()
                        .as_encoded_bytes(),
                );
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt as _;
                for value in [
                    metadata.dev(),
                    metadata.ino(),
                    metadata.rdev(),
                    metadata.mtime() as u64,
                    metadata.mtime_nsec() as u64,
                    metadata.ctime() as u64,
                    metadata.ctime_nsec() as u64,
                ] {
                    digest.update(value.to_le_bytes());
                }
            }
            return Ok(FileStamp {
                kind: opaque_kind(&metadata).to_owned(),
                digest: Some(digest.finalize().into()),
                permissions,
                bytes: 0,
                preview: "[metadata only; target contents not read]".to_owned(),
            });
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

    fn comparison(&self) -> Result<BTreeMap<String, FileStamp>, ProductRunnerError> {
        let mut before = self.load()?;
        let mut roots = Vec::<String>::new();
        for path in before.keys() {
            if self
                .observed_path(path)?
                .is_some_and(|path| fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()))
                && !roots.iter().any(|root| Path::new(path).starts_with(root))
            {
                roots.push(path.clone());
            }
        }
        for root in roots {
            for path in self.subtree_paths(&root, false)? {
                // The original declaration captured every admissible descendant. A newly
                // observed child was therefore absent, never enrolled at its postimage.
                before.entry(path).or_insert_with(absent);
            }
        }
        Ok(before)
    }

    fn subtree_paths(
        &self,
        relative: &str,
        admission: bool,
    ) -> Result<Vec<String>, ProductRunnerError> {
        let mut entries = Vec::new();
        let mut pending = vec![(relative.to_owned(), 0_usize)];
        while let Some((directory, depth)) = pending.pop() {
            if admission && depth >= MAX_SUBTREE_DEPTH {
                return Err(failure("declared directory is too deep; declare narrower task paths"));
            }
            let children = fs::read_dir(self.checked_path(&directory)?)
                .map_err(|error| failure(error.to_string()))?;
            for child in children {
                let child = child.map_err(|error| failure(error.to_string()))?;
                let path = child.path();
                let relative =
                    path.strip_prefix(&self.root).map_err(|error| failure(error.to_string()))?;
                // A directory declaration does not authorize private data, links, Git metadata
                // or build caches. Do not follow or fingerprint them through this inventory.
                if crate::developer_tools::ignored(relative) {
                    continue;
                }
                let Some(relative) = relative.to_str() else { continue };
                if self.checked_path(relative).is_err() {
                    continue;
                }
                let metadata =
                    fs::symlink_metadata(&path).map_err(|error| failure(error.to_string()))?;
                if !metadata.is_file() && !metadata.is_dir() {
                    continue;
                }
                if admission && entries.len() >= MAX_SUBTREE_ENTRIES {
                    return Err(failure(
                        "declared directory is too broad; declare narrower task paths",
                    ));
                }
                entries.push(relative.to_owned());
                if metadata.is_dir() {
                    pending.push((relative.to_owned(), depth + 1));
                }
            }
        }
        Ok(entries)
    }

    fn observed_path(&self, relative: &str) -> Result<Option<PathBuf>, ProductRunnerError> {
        if relative.is_empty() || relative == "." {
            return Err(failure("in-place evidence requires a scoped path"));
        }
        crate::developer_tools::checked_protected_observation(&self.root, relative, &self.protected)
            .map_err(|error| failure(error.to_string()))
    }
}

fn opaque_kind(metadata: &fs::Metadata) -> &'static str {
    if metadata.file_type().is_symlink() {
        return "symlink";
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt as _;
        let kind = metadata.file_type();
        if kind.is_socket() {
            return "socket";
        }
        if kind.is_fifo() {
            return "pipe";
        }
    }
    "non-regular"
}

fn absent() -> FileStamp {
    FileStamp {
        kind: "absent".to_owned(),
        digest: None,
        permissions: None,
        bytes: 0,
        preview: String::new(),
    }
}

pub fn definition() -> Result<peritus_model_protocol::ToolDefinition, ProductRunnerError> {
    crate::developer_tools::in_place_definition()
}

fn failure(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, "track in-place task files", detail)
}

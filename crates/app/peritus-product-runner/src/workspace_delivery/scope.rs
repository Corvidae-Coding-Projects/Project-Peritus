//! In-place comparison evidence for explicit files and declared subtrees. No Git.

#[path = "scope_store.rs"]
mod store;

use crate::progress::{CheckpointEntry, ContentKind, WorkspaceCheckpoint};
use crate::{ProductRunnerError, ProductRunnerErrorKind, file_metadata};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

const MAX_PREVIEW_BYTES: usize = 16 * 1024;

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

#[derive(Clone, Debug, Deserialize, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct FileStamp {
    kind: String,
    digest: Option<[u8; 32]>,
    permissions: Option<u32>,
    bytes: u64,
    preview: String,
}

// Previews are a presentation projection, including for journals from older releases.
// Candidate identity depends on exact bytes, type and permissions, never preview encoding.
impl PartialEq for FileStamp {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.digest == other.digest
            && self.permissions == other.permissions
            && self.bytes == other.bytes
    }
}

struct FileObservation {
    stamp: FileStamp,
    checkpoint: CheckpointEntry,
}

impl FileObservation {
    fn new(relative: &str, stamp: FileStamp, kind: ContentKind, digest: Option<[u8; 32]>) -> Self {
        let checkpoint =
            CheckpointEntry::observed(PathBuf::from(relative), digest, stamp.permissions, kind);
        Self { stamp, checkpoint }
    }

    fn absent(relative: &str) -> Self {
        Self::new(relative, absent(), ContentKind::Other, None)
    }
}

pub(crate) struct ScopedCandidateObservation {
    pub(crate) has_changes: bool,
    pub(crate) content: peritus_types::Sha256Digest,
    pub(crate) repository: peritus_types::Sha256Digest,
}

struct EnrolledPaths {
    entries: BTreeMap<String, FileStamp>,
    declarations: BTreeSet<String>,
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
    /// Directory descendants are comparison evidence, not an implicit request to capture live
    /// service data for rewind. Reading or editing a descendant explicitly enrolls that file.
    pub(crate) fn checkpoint_paths(&self) -> Result<Vec<PathBuf>, ProductRunnerError> {
        let enrolled = self.load()?;
        let EnrolledPaths { entries, declarations } = enrolled.as_ref();
        entries
            .iter()
            .filter(|(path, _)| declarations.contains(*path))
            .filter_map(|(path, before)| match self.observed_kind(&path) {
                Ok(current)
                    if matches!(before.kind.as_str(), "file" | "absent")
                        && matches!(current.as_str(), "file" | "absent") =>
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
    ) -> Result<WorkspaceCheckpoint, ProductRunnerError> {
        WorkspaceCheckpoint::scoped(root, self.changed_paths(root)?)
    }

    /// Derives both identity axes from the same observations of the enrolled path set.
    /// Running services may change their files between observations; their next observation
    /// invalidates old evidence without requiring the entire directory to stop changing.
    pub(crate) fn observe_candidate(
        &self,
        root: &Path,
    ) -> Result<ScopedCandidateObservation, ProductRunnerError> {
        self.observe_candidate_mode(root, false)
    }

    pub(crate) fn observe_candidate_fresh(
        &self,
        root: &Path,
    ) -> Result<ScopedCandidateObservation, ProductRunnerError> {
        self.observe_candidate_mode(root, true)
    }

    fn observe_candidate_mode(
        &self,
        root: &Path,
        fresh: bool,
    ) -> Result<ScopedCandidateObservation, ProductRunnerError> {
        self.check_root(root)?;
        let mut content = Vec::new();
        let mut repository = Vec::new();
        for (path, before) in self.comparison()? {
            let current = self.observe_mode(&path, fresh)?;
            if current.stamp != before {
                content.push(current.checkpoint.clone());
            }
            repository.push(current.checkpoint);
        }
        Ok(ScopedCandidateObservation {
            has_changes: !content.is_empty(),
            content: WorkspaceCheckpoint::scoped_entries(content).digest(),
            repository: WorkspaceCheckpoint::scoped_entries(repository).digest(),
        })
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
        let full = self.checked_path(path)?;
        let relative =
            full.strip_prefix(&self.root).map_err(|_| failure("scope target escaped workspace"))?;
        let relative = relative.to_str().ok_or_else(|| failure("scope path is not UTF-8"))?;
        self.enroll_index(relative)
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
            for line in text_preview(&before.preview).lines() {
                let _ = writeln!(diff, "-{line}");
            }
            for line in text_preview(&after.preview).lines() {
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
            Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(Some(meta.len())),
            Ok(_) => Err(failure("in-place evidence is not a regular journal")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(failure(error.to_string())),
        }
    }

    fn load(&self) -> Result<std::sync::Arc<EnrolledPaths>, ProductRunnerError> {
        self.load_index()
    }

    fn checked_path(&self, relative: &str) -> Result<PathBuf, ProductRunnerError> {
        if relative.is_empty() || relative == "." {
            return Err(failure("in-place evidence requires a file or a scoped directory path"));
        }
        let path = crate::developer_tools::checked_protected_file(
            &self.root,
            relative,
            "",
            &self.protected,
        )
        .map_err(|error| failure(error.to_string()))?;
        if self.private_evidence(&path) {
            return Err(failure("scope evidence is outside task authority"));
        }
        Ok(path)
    }

    fn stamp(&self, relative: &str) -> Result<FileStamp, ProductRunnerError> {
        self.observe(relative).map(|observed| observed.stamp)
    }

    fn observed_kind(&self, relative: &str) -> Result<String, ProductRunnerError> {
        let Some(path) = self.observed_path(relative)? else { return Ok("absent".to_owned()) };
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_file() => Ok("file".to_owned()),
            Ok(meta) if meta.is_dir() => Ok("directory".to_owned()),
            Ok(_) => Ok("other".to_owned()),
            Err(error) if missing(&error) => Ok("absent".to_owned()),
            Err(error) => Err(failure(error.to_string())),
        }
    }

    fn observe(&self, relative: &str) -> Result<FileObservation, ProductRunnerError> {
        self.observe_mode(relative, false)
    }

    fn observe_mode(
        &self,
        relative: &str,
        fresh: bool,
    ) -> Result<FileObservation, ProductRunnerError> {
        let Some(path) = self.observed_path(relative)? else {
            return Ok(FileObservation::absent(relative));
        };
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(FileObservation::absent(relative));
            }
            Err(error) => return Err(failure(error.to_string())),
        };
        let permissions = Some(file_metadata::permission_fingerprint(&metadata));
        if metadata.is_dir() {
            // Directory contents belong to individual descendant stamps. Population is a
            // normal lifecycle transition, including journals written by older releases.
            let stamp = FileStamp {
                kind: "directory".to_owned(),
                digest: None,
                permissions,
                bytes: 0,
                preview: String::new(),
            };
            return Ok(FileObservation::new(
                relative,
                stamp,
                ContentKind::Other,
                Some(Sha256::digest(b"non-file").into()),
            ));
        }
        if !metadata.is_file() {
            // Runtime endpoints (sockets/pipes) and later link replacement are observable
            // effects, not file bodies. Do not open, read through, or restore them as files.
            let mut digest = Sha256::new();
            let (kind, checkpoint_digest) = if metadata.file_type().is_symlink() {
                let target = fs::read_link(&path).map_err(|error| failure(error.to_string()))?;
                let target = target.as_os_str().as_encoded_bytes();
                digest.update(target);
                (ContentKind::Symlink, Sha256::digest(target).into())
            } else {
                (ContentKind::Other, Sha256::digest(b"non-file").into())
            };
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
            let stamp = FileStamp {
                kind: opaque_kind(&metadata).to_owned(),
                digest: Some(digest.finalize().into()),
                permissions,
                bytes: 0,
                preview: "[metadata only; target contents not read]".to_owned(),
            };
            return Ok(FileObservation::new(relative, stamp, kind, Some(checkpoint_digest)));
        }
        let content = match crate::file_observation::read(&path, fresh) {
            Ok(content) => content,
            Err(error) if missing(&error) => return Ok(FileObservation::absent(relative)),
            Err(error) => return Err(failure(error.to_string())),
        };
        let digest = content.digest;
        let stamp = FileStamp {
            kind: "file".to_owned(),
            digest: Some(digest),
            permissions,
            bytes: content.bytes,
            preview: String::from_utf8_lossy(&content.preview).into_owned(),
        };
        Ok(FileObservation::new(relative, stamp, ContentKind::File, Some(digest)))
    }

    fn comparison(&self) -> Result<BTreeMap<String, FileStamp>, ProductRunnerError> {
        let mut before = self.load()?.entries.clone();
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
        self.visit_subtree(relative, admission, |path| {
            entries.push(path.to_owned());
            Ok(())
        })?;
        Ok(entries)
    }

    fn visit_subtree(
        &self,
        relative: &str,
        admission: bool,
        mut visit: impl FnMut(&str) -> Result<(), ProductRunnerError>,
    ) -> Result<(), ProductRunnerError> {
        let mut pending = vec![relative.to_owned()];
        while let Some(directory) = pending.pop() {
            let path = if admission {
                self.checked_path(&directory)?
            } else {
                let Some(path) = self.observed_path(&directory)? else { continue };
                match fs::symlink_metadata(&path) {
                    Ok(metadata) if metadata.is_dir() => path,
                    Ok(_) => continue,
                    Err(error) if missing(&error) => continue,
                    Err(error) => return Err(failure(error.to_string())),
                }
            };
            let children = match fs::read_dir(path) {
                Ok(children) => children,
                Err(error) if !admission && missing(&error) => continue,
                Err(error) => return Err(failure(error.to_string())),
            };
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
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(error) if !admission && missing(&error) => continue,
                    Err(error) => return Err(failure(error.to_string())),
                };
                if !metadata.is_file() && !metadata.is_dir() {
                    continue;
                }
                visit(relative)?;
                if metadata.is_dir() {
                    pending.push(relative.to_owned());
                }
            }
        }
        Ok(())
    }

    fn observed_path(&self, relative: &str) -> Result<Option<PathBuf>, ProductRunnerError> {
        if relative.is_empty() || relative == "." {
            return Err(failure("in-place evidence requires a scoped path"));
        }
        let path = crate::developer_tools::checked_protected_observation(
            &self.root,
            relative,
            &self.protected,
        )
        .map_err(|error| failure(error.to_string()))?;
        Ok(path.filter(|path| !self.private_evidence(path)))
    }
}

fn text_preview(value: &str) -> &str {
    // A zero-filled database page is valid UTF-8, but not model text. Keep durable stamps
    // and full-file digests unchanged, including previews captured by older versions.
    if value.contains('\0') { "[binary content; NUL-bearing preview omitted]" } else { value }
}

fn missing(error: &std::io::Error) -> bool {
    matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory)
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

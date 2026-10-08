//! Exact-file in-place comparison evidence. No Git and no recursive directory inventory.

use crate::{
    ProductRunnerError, ProductRunnerErrorKind, WorkspaceMutationKind,
    control::{CheckpointFileMode, CheckpointFileVersion, WorkspaceMutationBaseline},
    file_metadata,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    fmt::Write as _,
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

const MAX_PREVIEW_BYTES: usize = 16 * 1024;
const MAX_DIFF_PREVIEW_BYTES: usize = 480 * 1024;
const EVIDENCE_PAGE_BYTES: usize = 16 * 1024;
const EVIDENCE_MARKER: &str = "peritus_scope_evidence_v1";

#[cfg(test)]
mod tests;
mod storage;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preimage: Option<Preimage>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Preimage {
    sha256: [u8; 32],
    bytes: u64,
}

#[derive(Clone, Eq, PartialEq)]
struct BaselineEntry {
    before: FileStamp,
    preimage: Option<Preimage>,
    complete: bool,
}

pub(crate) struct EvidencePage {
    pub(crate) handle: String,
    pub(crate) sha256: String,
    pub(crate) bytes: u64,
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) content: Vec<u8>,
    pub(crate) next: Option<u64>,
}

pub(crate) struct EvidenceContent {
    pub(crate) path: PathBuf,
    pub(crate) sha256: [u8; 32],
    pub(crate) bytes: u64,
    pub(crate) permissions: u32,
}

pub(crate) struct UnavailableEvidenceContent {
    pub(crate) sha256: [u8; 32],
    pub(crate) bytes: u64,
    pub(crate) permissions: u32,
}

pub(crate) struct EvidenceEntry {
    pub(crate) path: PathBuf,
    pub(crate) before: Option<EvidenceContent>,
    pub(crate) before_unavailable: Option<UnavailableEvidenceContent>,
    pub(crate) after: Option<EvidenceContent>,
    pub(crate) before_kind: String,
    pub(crate) after_kind: String,
    pub(crate) before_permissions: Option<u32>,
    pub(crate) after_permissions: Option<u32>,
    pub(crate) legacy_preimage_missing: bool,
}

pub(crate) struct EvidenceSnapshot {
    pub(crate) entries: Vec<EvidenceEntry>,
    pub(crate) content_digest: peritus_types::Sha256Digest,
    pub(crate) repository_digest: peritus_types::Sha256Digest,
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
        Ok(storage::snapshot(self)?.entries.into_keys().map(PathBuf::from).collect())
    }

    pub(crate) fn path_count(&self) -> Result<usize, ProductRunnerError> {
        storage::path_count(self)
    }

    pub(crate) fn mutation_baseline(
        &self,
        relative: &str,
    ) -> Result<(WorkspaceMutationKind, WorkspaceMutationBaseline), ProductRunnerError> {
        self.checked_path(relative)?;
        let baseline = storage::load_one(self, relative)?
            .ok_or_else(|| failure("command path is not enrolled in the in-place scope"))?;
        self.mutation_baseline_entry(&baseline)
    }

    pub(crate) fn command_paths(&self) -> Result<Vec<PathBuf>, ProductRunnerError> {
        let first = storage::snapshot(self)?;
        let mut paths = Vec::with_capacity(first.entries.len());
        for (relative, baseline) in &first.entries {
            if self.checked_path(relative)?.is_dir() {
                return Err(failure("command scope paths must be regular files or absent"));
            }
            self.mutation_baseline_entry(baseline)?;
            paths.push(PathBuf::from(relative));
        }
        if !storage::frontier_matches(self, first.frontier)? {
            return Err(failure(
                "in-place command scope changed during baseline validation",
            ));
        }
        Ok(paths)
    }

    fn mutation_baseline_entry(
        &self,
        baseline: &BaselineEntry,
    ) -> Result<(WorkspaceMutationKind, WorkspaceMutationBaseline), ProductRunnerError> {
        let before = &baseline.before;
        match before.kind.as_str() {
            "absent"
                if before.digest.is_none()
                    && before.permissions.is_none()
                    && before.bytes == 0
                    && baseline.preimage.is_none() =>
            {
                WorkspaceMutationBaseline::new(CheckpointFileVersion::Absent, None)
                    .map(|baseline| (WorkspaceMutationKind::File, baseline))
                    .map_err(|_| failure("in-place absent baseline is invalid"))
            }
            "directory"
                if before.digest.is_none()
                    && before.bytes == 0
                    && baseline.preimage.is_none() =>
            {
                let permissions = u16::try_from(
                    before
                        .permissions
                        .ok_or_else(|| failure("in-place directory baseline has no permissions"))?,
                )
                .map_err(|_| failure("in-place directory permissions are out of range"))?;
                let mode = peritus_patch::DirectoryMode::new(permissions)
                    .map_err(|error| failure(error.to_string()))?;
                WorkspaceMutationBaseline::new(
                    CheckpointFileVersion::empty_directory(mode),
                    None,
                )
                .map(|baseline| (WorkspaceMutationKind::EmptyDirectory, baseline))
                .map_err(|_| failure("in-place directory baseline is invalid"))
            }
            "file" => {
                if !baseline.complete {
                    return Err(failure(
                        "legacy in-place file baseline has no complete retained preimage",
                    ));
                }
                let digest = before
                    .digest
                    .ok_or_else(|| failure("in-place file baseline has no content digest"))?;
                let permissions = before
                    .permissions
                    .ok_or_else(|| failure("in-place file baseline has no permissions"))?;
                let preimage = baseline
                    .preimage
                    .as_ref()
                    .ok_or_else(|| failure("in-place file baseline has no retained preimage"))?;
                let file = self.open_preimage(preimage)?;
                let mode = checkpoint_file_mode(permissions);
                let version = CheckpointFileVersion::present(
                    peritus_types::Sha256Digest::new(digest),
                    before.bytes,
                    mode,
                );
                let snapshot = peritus_patch::SnapshotFile::new(
                    file,
                    peritus_types::Sha256Digest::new(digest),
                    before.bytes,
                    patch_file_mode(mode),
                );
                WorkspaceMutationBaseline::new(version, Some(snapshot))
                    .map(|baseline| (WorkspaceMutationKind::File, baseline))
                    .map_err(|_| failure("retained in-place file baseline is invalid"))
            }
            _ => Err(failure("in-place baseline has an unsupported object identity")),
        }
    }

    pub(crate) fn progress_checkpoint(
        &self,
        root: &Path,
    ) -> Result<crate::progress::WorkspaceCheckpoint, ProductRunnerError> {
        crate::progress::WorkspaceCheckpoint::scoped(root, self.changed_paths(root)?)
    }

    pub fn changed_paths(&self, root: &Path) -> Result<Vec<PathBuf>, ProductRunnerError> {
        self.check_root(root)?;
        let snapshot = storage::snapshot(self)?;
        let changed = snapshot
            .entries
            .into_iter()
            .filter_map(|(path, baseline)| match self.stamp(&path) {
                Ok(current) if current == baseline.before => None,
                Ok(_) => Some(Ok(PathBuf::from(path))),
                Err(error) => Some(Err(error)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !storage::frontier_matches(self, snapshot.frontier)? {
            return Err(failure(
                "in-place scope changed while changed paths were observed",
            ));
        }
        Ok(changed)
    }

    pub fn enroll(&self, path: &str) -> Result<(), ProductRunnerError> {
        self.checked_path(path)?;
        if storage::load_one(self, path)?.is_some() {
            return Ok(());
        }
        let before = self.stamp(path)?;
        let preimage = self.retain_preimage(path, &before)?;
        let entry = Entry {
            version: 2,
            scope: self.clone(),
            path: path.to_owned(),
            before,
            preimage,
        };
        storage::enroll(self, &entry)
    }

    pub fn diff(&self, root: &Path) -> Result<String, ProductRunnerError> {
        self.check_root(root)?;
        let snapshot = storage::snapshot(self)?;
        let binding = storage::binding(self)?;
        let mut diff = storage::artifact_writer(self)?;
        writeln!(
            diff,
            "In-place task-file comparison; not a whole-folder inventory or a Git candidate."
        )
        .map_err(|error| failure(error.to_string()))?;
        let mut observed = Vec::with_capacity(snapshot.entries.len());
        for (path, baseline) in snapshot.entries {
            let before = &baseline.before;
            let after = self.stamp(&path)?;
            observed.push((path.clone(), after.clone()));
            if *before == after {
                continue;
            }
            writeln!(
                diff,
                "\n--- before/{path}\n+++ current/{path}\nkind: {} -> {}; permissions: {:?} -> {:?}; bytes: {} -> {}",
                before.kind,
                after.kind,
                before.permissions,
                after.permissions,
                before.bytes,
                after.bytes
            )
            .map_err(|error| failure(error.to_string()))?;
            for line in before.preview.lines() {
                writeln!(diff, "-{line}").map_err(|error| failure(error.to_string()))?;
            }
            for line in after.preview.lines() {
                writeln!(diff, "+{line}").map_err(|error| failure(error.to_string()))?;
            }
            if let Some(preimage) = baseline.preimage.as_ref() {
                let artifact = retain_artifact(
                    storage::artifact_writer(self)?,
                    self.open_preimage(preimage)?,
                    preimage.sha256,
                    preimage.bytes,
                )?;
                write_artifact_reference(&mut diff, "before", binding, artifact)?;
            } else if before.kind == "file" {
                writeln!(
                    diff,
                    "before_exact: unavailable legacy preimage sha256={} bytes={}",
                    before.digest.map_or_else(|| "missing".to_owned(), |digest| hex(&digest)),
                    before.bytes,
                )
                .map_err(|error| failure(error.to_string()))?;
            }
            if let Some(digest) = after.digest {
                let artifact = retain_artifact(
                    storage::artifact_writer(self)?,
                    self.open_current(&path, digest, after.bytes)?,
                    digest,
                    after.bytes,
                )?;
                write_artifact_reference(&mut diff, "current", binding, artifact)?;
            }
        }
        let artifact = diff.finish()?;
        for (path, expected) in observed {
            if self.stamp(&path)? != expected {
                return Err(failure(
                    "in-place file changed while its replayable comparison was published",
                ));
            }
        }
        if !storage::frontier_matches(self, snapshot.frontier)? {
            return Err(failure(
                "in-place scope changed while its replayable comparison was published",
            ));
        }
        let mut page = storage::read_artifact_page(
            self,
            artifact.sha256,
            artifact.bytes,
            0,
            MAX_DIFF_PREVIEW_BYTES,
        )?;
        while let Err(error) = std::str::from_utf8(&page) {
            if error.error_len().is_some() {
                return Err(failure("generated in-place comparison is not UTF-8"));
            }
            page.truncate(error.valid_up_to());
        }
        let consumed = page.len() as u64;
        let next = (consumed < artifact.bytes).then_some(consumed);
        let handle = evidence_handle(binding, artifact.sha256, "utf8");
        let mut preview = format!(
            "{EVIDENCE_MARKER} handle={handle} size={} sha256={} offset=0 next={}\n",
            artifact.bytes,
            hex(&artifact.sha256),
            next.map_or_else(|| "null".to_owned(), |offset| offset.to_string()),
        );
        preview.push_str(
            "This comparison is a physical preview of a complete retained UTF-8 artifact. Use workspace_scope_evidence_read with the exact handle, digest, size and next offset until next is null. Per-file handles in that artifact retrieve exact before/current bytes from offset zero.\n\n",
        );
        preview.push_str(
            std::str::from_utf8(&page)
                .map_err(|_| failure("generated in-place comparison is not UTF-8"))?,
        );
        Ok(preview)
    }

    pub(crate) fn evidence_snapshot(
        &self,
        root: &Path,
    ) -> Result<EvidenceSnapshot, ProductRunnerError> {
        self.check_root(root)?;
        let snapshot = storage::snapshot(self)?;
        let mut entries = Vec::new();
        let mut content = Sha256::new();
        checkpoint_bytes(&mut content, b"in-place-task-files-v1");
        let mut repository = Sha256::new();
        checkpoint_bytes(&mut repository, b"in-place-task-files-v1");
        for (path, baseline) in snapshot.entries {
            let after = self.stamp(&path)?;
            checkpoint_entry(&mut repository, &path, &after);
            if baseline.before == after {
                continue;
            }
            let legacy_preimage_missing = !baseline.complete
                && baseline.before.digest.is_some()
                && baseline.preimage.is_none();
            let before = baseline.preimage.as_ref().map(|preimage| EvidenceContent {
                path: self.preimage_path(&preimage.sha256),
                sha256: preimage.sha256,
                bytes: preimage.bytes,
                permissions: baseline.before.permissions.unwrap_or_default(),
            });
            let before_unavailable = if legacy_preimage_missing {
                if baseline.before.kind != "file" {
                    return Err(failure(
                        "legacy in-place preimage identity does not describe a regular file",
                    ));
                }
                Some(UnavailableEvidenceContent {
                    sha256: baseline.before.digest.ok_or_else(|| {
                        failure("legacy in-place file preimage has no content digest")
                    })?,
                    bytes: baseline.before.bytes,
                    permissions: baseline.before.permissions.ok_or_else(|| {
                        failure("legacy in-place file preimage has no permission identity")
                    })?,
                })
            } else {
                None
            };
            let current = after.digest.map(|sha256| EvidenceContent {
                path: root.join(&path),
                sha256,
                bytes: after.bytes,
                permissions: after.permissions.unwrap_or_default(),
            });
            checkpoint_entry(&mut content, &path, &after);
            entries.push(EvidenceEntry {
                path: PathBuf::from(path),
                before,
                before_unavailable,
                after: current,
                before_kind: baseline.before.kind.clone(),
                after_kind: after.kind.clone(),
                before_permissions: baseline.before.permissions,
                after_permissions: after.permissions,
                legacy_preimage_missing,
            });
        }
        let evidence = EvidenceSnapshot {
            entries,
            content_digest: peritus_types::Sha256Digest::new(content.finalize().into()),
            repository_digest: peritus_types::Sha256Digest::new(repository.finalize().into()),
        };
        if !storage::frontier_matches(self, snapshot.frontier)? {
            return Err(failure(
                "in-place scope changed while candidate evidence was captured",
            ));
        }
        Ok(evidence)
    }

    pub(crate) fn evidence_page(
        &self,
        handle: &str,
        digest: &str,
        bytes: u64,
        offset: u64,
    ) -> Result<EvidencePage, ProductRunnerError> {
        let digest = parse_hex(digest)
            .ok_or_else(|| failure("in-place evidence digest is not canonical lowercase SHA-256"))?;
        let binding = storage::binding(self)?;
        let utf8 = handle == evidence_handle(binding, digest, "utf8");
        if !utf8 && handle != evidence_handle(binding, digest, "bytes") {
            return Err(failure("in-place evidence handle conflicts with its retained scope"));
        }
        let mut content = storage::read_artifact_page(
            self,
            digest,
            bytes,
            offset,
            EVIDENCE_PAGE_BYTES,
        )?;
        if utf8 {
            while let Err(error) = std::str::from_utf8(&content) {
                if error.error_len().is_some() {
                    return Err(failure("retained UTF-8 scope evidence is corrupt"));
                }
                content.truncate(error.valid_up_to());
            }
            if content.is_empty() && offset < bytes {
                return Err(failure("UTF-8 scope evidence page cannot advance"));
            }
        }
        let end = offset
            .checked_add(content.len() as u64)
            .ok_or_else(|| failure("in-place evidence page offset overflow"))?;
        Ok(EvidencePage {
            handle: handle.to_owned(),
            sha256: hex(&digest),
            bytes,
            start: offset,
            end,
            content,
            next: (end < bytes).then_some(end),
        })
    }

    fn check_root(&self, root: &Path) -> Result<(), ProductRunnerError> {
        if root != self.root {
            return Err(failure("in-place evidence belongs to another directory"));
        }
        Ok(())
    }

    fn retain_preimage(
        &self,
        relative: &str,
        stamp: &FileStamp,
    ) -> Result<Option<Preimage>, ProductRunnerError> {
        let Some(expected) = stamp.digest else { return Ok(None) };
        self.checked_path(relative)?;
        let directory = self.preimage_directory();
        fs::create_dir_all(&directory).map_err(|error| failure(error.to_string()))?;
        let metadata = fs::symlink_metadata(&directory)
            .map_err(|error| failure(error.to_string()))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(failure("in-place preimage store is not an owned directory"));
        }
        #[cfg(unix)]
        {
            let parent = directory.parent().ok_or_else(|| failure("preimage store has no parent"))?;
            fs::File::open(parent)
                .and_then(|file| file.sync_all())
                .map_err(|error| failure(error.to_string()))?;
        }
        let mut source = self.open_current(relative, expected, stamp.bytes)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|error| failure(error.to_string()))?;
        let mut hasher = Sha256::new();
        let mut bytes = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = source.read(&mut buffer).map_err(|error| failure(error.to_string()))?;
            if count == 0 { break; }
            hasher.update(&buffer[..count]);
            temporary.write_all(&buffer[..count]).map_err(|error| failure(error.to_string()))?;
            bytes = bytes.checked_add(count as u64)
                .ok_or_else(|| failure("in-place preimage length overflow"))?;
        }
        let digest: [u8; 32] = hasher.finalize().into();
        if digest != expected || bytes != stamp.bytes {
            return Err(failure(
                "in-place file changed while its complete preimage was retained; retry enrollment",
            ));
        }
        temporary.as_file().sync_all().map_err(|error| failure(error.to_string()))?;
        let destination = self.preimage_path(&digest);
        match temporary.persist_noclobber(&destination) {
            Ok(_) => {
                fs::File::open(&directory)
                    .and_then(|file| file.sync_all())
                    .map_err(|error| failure(error.to_string()))?;
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(&destination)
                    .map_err(|error| failure(error.to_string()))?;
                if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != bytes {
                    return Err(failure("retained in-place preimage conflicts with its digest"));
                }
                let mut existing = fs::File::open(&destination)
                    .map_err(|error| failure(error.to_string()))?;
                let mut hasher = Sha256::new();
                let mut buffer = [0_u8; 64 * 1024];
                loop {
                    let count = existing.read(&mut buffer)
                        .map_err(|error| failure(error.to_string()))?;
                    if count == 0 { break; }
                    hasher.update(&buffer[..count]);
                }
                let retained: [u8; 32] = hasher.finalize().into();
                if retained != digest {
                    return Err(failure("retained in-place preimage digest is corrupt"));
                }
            }
            Err(error) => return Err(failure(error.error.to_string())),
        }
        Ok(Some(Preimage { sha256: digest, bytes }))
    }

    fn preimage_directory(&self) -> PathBuf {
        self.journal.with_extension("preimages")
    }

    fn open_preimage(&self, preimage: &Preimage) -> Result<fs::File, ProductRunnerError> {
        let directory = self.preimage_directory();
        let directory_metadata =
            fs::symlink_metadata(&directory).map_err(|error| failure(error.to_string()))?;
        if !directory_metadata.is_dir() || directory_metadata.file_type().is_symlink() {
            return Err(failure("in-place preimage store is not an owned directory"));
        }
        let path = self.preimage_path(&preimage.sha256);
        let before = fs::symlink_metadata(&path).map_err(|error| failure(error.to_string()))?;
        if !before.is_file()
            || before.file_type().is_symlink()
            || before.len() != preimage.bytes
        {
            return Err(failure("retained in-place preimage has an invalid file identity"));
        }
        let file = fs::File::open(path).map_err(|error| failure(error.to_string()))?;
        let opened = file.metadata().map_err(|error| failure(error.to_string()))?;
        if !same_open_file(&before, &opened) {
            return Err(failure("retained in-place preimage changed while it was opened"));
        }
        Ok(file)
    }

    fn open_current(
        &self,
        relative: &str,
        _digest: [u8; 32],
        bytes: u64,
    ) -> Result<fs::File, ProductRunnerError> {
        let path = self.checked_path(relative)?;
        let before = fs::symlink_metadata(&path).map_err(|error| failure(error.to_string()))?;
        if !before.is_file() || before.file_type().is_symlink() || before.len() != bytes {
            return Err(failure("current in-place evidence has an invalid file identity"));
        }
        let file = fs::File::open(&path).map_err(|error| failure(error.to_string()))?;
        let opened = file.metadata().map_err(|error| failure(error.to_string()))?;
        let after = fs::symlink_metadata(&path).map_err(|error| failure(error.to_string()))?;
        if !same_open_file(&before, &opened) || !same_open_file(&after, &opened) {
            return Err(failure("current in-place evidence changed while it was opened"));
        }
        Ok(file)
    }

    fn preimage_path(&self, digest: &[u8; 32]) -> PathBuf {
        use std::fmt::Write as _;
        let name = digest.iter().fold(String::with_capacity(64), |mut name, byte| {
            let _ = write!(name, "{byte:02x}");
            name
        });
        self.preimage_directory().join(name)
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
        let mut file = fs::File::open(&path).map_err(|error| failure(error.to_string()))?;
        let opened = file.metadata().map_err(|error| failure(error.to_string()))?;
        let current = fs::symlink_metadata(&path).map_err(|error| failure(error.to_string()))?;
        if !same_open_file(&metadata, &opened)
            || !same_open_file(&current, &opened)
            || current.file_type().is_symlink()
        {
            return Err(failure("in-place evidence target changed while it was opened"));
        }
        let mut hasher = Sha256::new();
        let mut preview = Vec::new();
        let mut bytes = 0_u64;
        let mut buffer = [0; 8192];
        loop {
            let count = file.read(&mut buffer).map_err(|error| failure(error.to_string()))?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            bytes = bytes
                .checked_add(count as u64)
                .ok_or_else(|| failure("in-place evidence file length overflow"))?;
            let remaining = MAX_PREVIEW_BYTES.saturating_sub(preview.len());
            preview.extend_from_slice(&buffer[..count.min(remaining)]);
        }
        let after = fs::symlink_metadata(&path).map_err(|error| failure(error.to_string()))?;
        if !same_open_file(&metadata, &after) || bytes != metadata.len() {
            return Err(failure("in-place evidence target changed while it was read"));
        }
        Ok(FileStamp {
            kind: "file".to_owned(),
            digest: Some(hasher.finalize().into()),
            permissions,
            bytes,
            preview: String::from_utf8(preview).unwrap_or_else(|_| "[binary content]".to_owned()),
        })
    }
}

fn retain_artifact(
    mut writer: storage::ArtifactWriter,
    mut source: fs::File,
    digest: [u8; 32],
    bytes: u64,
) -> Result<storage::Artifact, ProductRunnerError> {
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = source.read(&mut buffer).map_err(|error| failure(error.to_string()))?;
        if count == 0 {
            break;
        }
        writer
            .write_all(&buffer[..count])
            .map_err(|error| failure(error.to_string()))?;
    }
    writer.finish_expected(digest, bytes)
}

fn write_artifact_reference(
    writer: &mut storage::ArtifactWriter,
    side: &str,
    binding: [u8; 32],
    artifact: storage::Artifact,
) -> Result<(), ProductRunnerError> {
    writeln!(
        writer,
        "{side}_exact: handle={} sha256={} size={} offset=0",
        evidence_handle(binding, artifact.sha256, "bytes"),
        hex(&artifact.sha256),
        artifact.bytes,
    )
    .map_err(|error| failure(error.to_string()))
}

fn evidence_handle(binding: [u8; 32], digest: [u8; 32], encoding: &str) -> String {
    format!("scope:{}:artifact:{}:{encoding}", hex(&binding), hex(&digest))
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().fold(String::with_capacity(64), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

fn parse_hex(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut output = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = hex_value(pair[0])?.checked_mul(16)?.checked_add(hex_value(pair[1])?)?;
    }
    Some(output)
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn checkpoint_entry(hasher: &mut Sha256, path: &str, stamp: &FileStamp) {
    checkpoint_bytes(hasher, Path::new(path).to_string_lossy().as_bytes());
    match (stamp.kind.as_str(), stamp.digest) {
        ("file", Some(digest)) => {
            hasher.update([1]);
            hasher.update(digest);
        }
        ("directory", None) => {
            hasher.update([4]);
            hasher.update(Sha256::digest(b"non-file"));
        }
        _ => hasher.update([0]),
    }
    match stamp.permissions {
        Some(permissions) => {
            hasher.update([1]);
            hasher.update(permissions.to_le_bytes());
        }
        None => hasher.update([0]),
    }
}

fn checkpoint_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

fn valid_preimage(entry: &Entry) -> bool {
    match (entry.version, entry.before.digest, entry.preimage.as_ref()) {
        (1, _, None) => true,
        (2, Some(digest), Some(preimage)) => {
            preimage.sha256 == digest && preimage.bytes == entry.before.bytes
        }
        (2, None, None) => true,
        _ => false,
    }
}

#[cfg(unix)]
fn checkpoint_file_mode(permissions: u32) -> CheckpointFileMode {
    if permissions & 0o111 == 0 {
        CheckpointFileMode::Regular
    } else {
        CheckpointFileMode::Executable
    }
}

#[cfg(not(unix))]
const fn checkpoint_file_mode(_permissions: u32) -> CheckpointFileMode {
    CheckpointFileMode::Regular
}

const fn patch_file_mode(mode: CheckpointFileMode) -> peritus_patch::FileMode {
    match mode {
        CheckpointFileMode::Regular => peritus_patch::FileMode::Regular,
        CheckpointFileMode::Executable => peritus_patch::FileMode::Executable,
    }
}

fn same_open_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    let same = left.len() == right.len()
        && left.modified().ok() == right.modified().ok()
        && left.permissions() == right.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        same && left.dev() == right.dev()
            && left.ino() == right.ino()
            && left.ctime() == right.ctime()
            && left.ctime_nsec() == right.ctime_nsec()
            && left.mode() == right.mode()
    }
    #[cfg(not(unix))]
    {
        same
    }
}

pub fn definitions() -> Result<Vec<peritus_model_protocol::ToolDefinition>, ProductRunnerError> {
    crate::developer_tools::in_place_definitions()
}

fn failure(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, "track in-place task files", detail)
}

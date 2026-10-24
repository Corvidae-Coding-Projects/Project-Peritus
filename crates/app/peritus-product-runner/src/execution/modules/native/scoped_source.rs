//! Host-owned immutable source capture and range evidence for native modules.

use super::ProductRunInput;
use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_patch::WorkspacePath;
use peritus_plugin_sdk::{
    ModuleHostOutcome, ScopedSourceAuthority, ScopedSourceEntry, ScopedSourceHandle,
    ScopedSourceManifestPage, ScopedSourceManifestRequest, ScopedSourceReadRequest,
    ScopedSourceReadResult, ScopedSourceRequest, ScopedSourceRevocation, ScopedSourceRevokeRequest,
};
use peritus_types::{EventId, Sha256Digest};
use peritus_workspace::{FolderIdentity, FolderInspection};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

const SCHEMA: u16 = 1;
const MAX_MANIFEST_ENTRIES: u16 = 512;
const MAX_RANGE_BYTES: u64 = 1024 * 1024;
const ARTIFACT_LIMIT: u64 = i64::MAX as u64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct BrokerIndex {
    schema: u16,
    next_revision: u64,
    scopes: BTreeMap<String, ScopeRecord>,
}

impl Default for BrokerIndex {
    fn default() -> Self {
        Self { schema: SCHEMA, next_revision: 1, scopes: BTreeMap::new() }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ScopeRecord {
    schema: u16,
    handle: String,
    scope_revision: String,
    snapshot_digest: String,
    manifest_artifact: String,
    display_name: String,
    file_count: u64,
    total_bytes: u64,
    exclusions: Vec<String>,
    revoked: bool,
    revoked_by: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceManifest {
    schema: u16,
    source_label: String,
    exclusions: Vec<String>,
    entries: Vec<ManifestEntry>,
    directories: Vec<DirectoryEntry>,
    total_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ManifestEntry {
    path: String,
    digest: String,
    bytes: u64,
    executable: bool,
    version: VersionStamp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct DirectoryEntry {
    path: String,
    version: VersionStamp,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct VersionStamp {
    bytes: u64,
    modified_nanos: String,
    identity: Option<String>,
    changed: Option<String>,
    mode: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Inventory {
    files: Vec<(String, VersionStamp)>,
    directories: Vec<DirectoryEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ReadReceipt {
    schema: u16,
    handle: String,
    scope_revision: String,
    snapshot_digest: String,
    path: String,
    source_digest: String,
    range_start: u64,
    range_end: u64,
    range_artifact: String,
}

pub(super) struct MaterializedSourceFile {
    pub(super) snapshot_digest: String,
    pub(super) path: PathBuf,
    pub(super) digest: String,
}

pub(super) struct Broker<'a> {
    input: &'a ProductRunInput,
    state: &'a Path,
    module_digest: &'a str,
    revision: u64,
}

impl<'a> Broker<'a> {
    pub(super) fn new(input: &'a ProductRunInput, state: &'a Path, module_digest: &'a str) -> Self {
        Self { input, state, module_digest, revision: input.conversation.revision() }
    }

    pub(super) fn dispatch(&self, operation: &str, input: serde_json::Value) -> ModuleHostOutcome {
        let result = match operation {
            "capture" => decode(input).and_then(|request| self.capture(request)),
            "manifest" => decode(input).and_then(|request| self.manifest(request)),
            "read" => decode(input).and_then(|request| self.read(request)),
            "revoke" => decode(input).and_then(|request| self.revoke(request)),
            _ => {
                return ModuleHostOutcome::Unsupported {
                    detail: format!(
                        "scoped-source operation {operation} is unavailable in contract version 1"
                    ),
                };
            }
        };
        match result {
            Ok(output) => ModuleHostOutcome::Success { output },
            Err(BrokerError::Rejected { code, detail }) => {
                ModuleHostOutcome::Rejected { code, detail }
            }
            Err(BrokerError::Failed { code, detail, retryable }) => {
                ModuleHostOutcome::Failed { code, detail, retryable_with_new_action: retryable }
            }
        }
    }

    pub(super) fn materialize_file(
        &self,
        authority: &ScopedSourceAuthority,
        source_path: &str,
        destination: &Path,
    ) -> Result<MaterializedSourceFile, BrokerError> {
        validate_relative_path(source_path)?;
        let index = self.load_index()?;
        let scope = self.require_scope(&index, authority)?;
        let manifest = self.load_manifest(scope)?;
        let entry = manifest
            .entries
            .binary_search_by(|entry| entry.path.as_str().cmp(source_path))
            .ok()
            .map(|index| &manifest.entries[index])
            .ok_or_else(|| {
                rejected(
                    "source_not_in_snapshot",
                    "the requested target is not an included regular file in this snapshot",
                )
            })?;
        let parent = destination.parent().ok_or_else(|| {
            failed("job_materialization", "target destination has no parent", false)
        })?;
        fs::create_dir_all(parent).map_err(|_| {
            failed("job_materialization", "target directory could not be created", true)
        })?;
        let store = self.artifacts()?;
        let digest = parse_digest(&entry.digest)?;
        let mut reader = store.open_read(digest).map_err(|_| {
            failed(
                "artifact_integrity",
                "the immutable target artifact is absent, quarantined, or corrupt",
                false,
            )
        })?;
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o400);
        }
        let mut output = options
            .open(destination)
            .map_err(|_| failed("job_materialization", "target file could not be created", true))?;
        let mut observed = 0_u64;
        let mut hasher = Sha256::new();
        while let Some(chunk) = reader.read_chunk(64 * 1024).map_err(|_| {
            failed("artifact_integrity", "target artifact verification failed", false)
        })? {
            output.write_all(chunk.bytes()).map_err(|_| {
                failed("job_materialization", "target bytes could not be written", true)
            })?;
            observed = observed
                .checked_add(u64::try_from(chunk.bytes().len()).map_err(|_| overflow())?)
                .ok_or_else(overflow)?;
            hasher.update(chunk.bytes());
        }
        output.sync_all().map_err(|_| {
            failed("job_materialization", "target bytes could not be synchronized", true)
        })?;
        if observed != entry.bytes || encode_hex(&hasher.finalize()) != entry.digest {
            return Err(failed(
                "artifact_integrity",
                "materialized target differs from its immutable source identity",
                false,
            ));
        }
        Ok(MaterializedSourceFile {
            snapshot_digest: scope.snapshot_digest.clone(),
            path: destination.to_path_buf(),
            digest: entry.digest.clone(),
        })
    }

    pub(super) fn validate_authority(
        &self,
        authority: &ScopedSourceAuthority,
    ) -> Result<String, BrokerError> {
        let index = self.load_index()?;
        let scope = self.require_scope(&index, authority)?;
        Ok(scope.snapshot_digest.clone())
    }

    fn capture(&self, request: ScopedSourceRequest) -> Result<serde_json::Value, BrokerError> {
        if request.schema != SCHEMA
            || !request.immutable_snapshot
            || request.purpose.trim().is_empty()
            || request.purpose.len() > 1024
            || request.purpose.chars().any(char::is_control)
        {
            return Err(rejected(
                "invalid_source_request",
                "source capture requires schema 1, an immutable snapshot, and a plain purpose",
            ));
        }
        let source_root = self.source_root()?;
        let exclusions = self.effective_exclusions(request.exclusions, &source_root)?;
        let root_identity = FolderIdentity::observe(&source_root).map_err(|_| {
            failed("source_unavailable", "source root identity is unavailable", true)
        })?;
        let inspection = FolderInspection::open(&root_identity).map_err(|_| {
            failed(
                "source_unavailable",
                "source root cannot be opened without following links",
                true,
            )
        })?;
        let mut index = self.load_index()?;
        let previous = match request.refresh.as_ref() {
            Some(authority) => Some(self.require_scope(&index, authority)?.clone()),
            None => None,
        };
        let previous_manifest =
            previous.as_ref().map(|scope| self.load_manifest(scope)).transpose()?;
        let before = inventory(&source_root, &exclusions, || self.cancelled())?;
        let store = self.artifacts()?;
        let temporary_root = self.recover_temporary_root()?;
        let prior_entries = previous_manifest
            .as_ref()
            .map(|manifest| {
                manifest
                    .entries
                    .iter()
                    .map(|entry| (entry.path.as_str(), entry))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let mut entries = Vec::with_capacity(before.files.len());
        let mut reused_files = 0_u64;
        let mut reused_bytes = 0_u64;
        let mut total_bytes = 0_u64;
        for (path, version) in &before.files {
            if self.cancelled() {
                return Err(failed("source_cancelled", "source capture was cancelled", true));
            }
            let prior = prior_entries.get(path.as_str()).copied();
            let (digest, reused) =
                if let Some(prior) = prior.filter(|entry| entry.version == *version) {
                    let digest = parse_digest(&prior.digest)?;
                    store.verify(digest).map_err(|_| {
                        failed(
                            "artifact_integrity",
                            "an unchanged source artifact failed digest verification",
                            false,
                        )
                    })?;
                    (prior.digest.clone(), true)
                } else {
                    (self.capture_file(&inspection, path, &temporary_root, &store)?, false)
                };
            if reused {
                reused_files = reused_files.checked_add(1).ok_or_else(overflow)?;
                reused_bytes = reused_bytes.checked_add(version.bytes).ok_or_else(overflow)?;
            }
            total_bytes = total_bytes.checked_add(version.bytes).ok_or_else(overflow)?;
            entries.push(ManifestEntry {
                path: path.clone(),
                digest,
                bytes: version.bytes,
                executable: version.mode.is_some_and(|mode| mode & 0o111 != 0),
                version: version.clone(),
            });
        }
        let after = inventory(&source_root, &exclusions, || self.cancelled())?;
        if before != after {
            return Err(failed(
                "mixed_source_revision",
                "source entries changed during capture; no source handle was published",
                true,
            ));
        }
        if self.cancelled() {
            return Err(failed(
                "source_cancelled",
                "source capture authority changed before publication",
                true,
            ));
        }
        let display_name = source_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("selected-workspace")
            .to_owned();
        let manifest = SourceManifest {
            schema: SCHEMA,
            source_label: display_name.clone(),
            exclusions: exclusions.clone(),
            entries,
            directories: before.directories,
            total_bytes,
        };
        let manifest_bytes = serde_json::to_vec(&manifest).map_err(json_failure)?;
        let manifest_digest = sha256_hex(&manifest_bytes);
        self.publish_artifact(
            &store,
            &manifest_bytes,
            "application/vnd.peritus.scoped-source-manifest.v1+json",
        )?;
        let prior_by_path = previous_manifest
            .as_ref()
            .map(|manifest| {
                manifest
                    .entries
                    .iter()
                    .map(|entry| (entry.path.as_str(), entry.digest.as_str()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let current_by_path = manifest
            .entries
            .iter()
            .map(|entry| (entry.path.as_str(), entry.digest.as_str()))
            .collect::<BTreeMap<_, _>>();
        let changed_paths = current_by_path
            .iter()
            .filter(|(path, digest)| prior_by_path.get(**path).is_some_and(|old| old != *digest))
            .map(|(path, _)| (*path).to_owned())
            .collect::<Vec<_>>();
        let removed_paths = prior_by_path
            .keys()
            .filter(|path| !current_by_path.contains_key(**path))
            .map(|path| (*path).to_owned())
            .collect::<Vec<_>>();
        let revision = index.next_revision;
        index.next_revision = revision.checked_add(1).ok_or_else(overflow)?;
        let scope_revision = format!("scope-{revision}-{:.16}", manifest_digest);
        let handle = sha256_hex(
            format!(
                "scoped-source-v1\0{}\0{}\0{}\0{}",
                self.module_digest, revision, manifest_digest, scope_revision
            )
            .as_bytes(),
        );
        let record = ScopeRecord {
            schema: SCHEMA,
            handle: handle.clone(),
            scope_revision: scope_revision.clone(),
            snapshot_digest: manifest_digest.clone(),
            manifest_artifact: manifest_digest.clone(),
            display_name: display_name.clone(),
            file_count: u64::try_from(manifest.entries.len()).map_err(|_| overflow())?,
            total_bytes,
            exclusions: exclusions.clone(),
            revoked: false,
            revoked_by: None,
        };
        if let Some(previous) = previous {
            let retained = index.scopes.get_mut(&previous.handle).ok_or_else(|| {
                failed("scope_state", "refreshed scope disappeared before publication", true)
            })?;
            retained.revoked = true;
            retained.revoked_by = Some(handle.clone());
        }
        index.scopes.insert(handle.clone(), record);
        self.save_index(&index)?;
        serde_json::to_value(ScopedSourceHandle {
            schema: SCHEMA,
            handle,
            scope_revision,
            snapshot_digest: manifest_digest.clone(),
            display_name,
            manifest_artifact: manifest_digest,
            file_count: u64::try_from(manifest.entries.len()).map_err(|_| overflow())?,
            total_bytes,
            reused_files,
            reused_bytes,
            changed_paths,
            removed_paths,
            exclusions,
        })
        .map_err(json_failure)
    }

    fn manifest(
        &self,
        request: ScopedSourceManifestRequest,
    ) -> Result<serde_json::Value, BrokerError> {
        if request.schema != SCHEMA || request.maximum_entries == 0 {
            return Err(rejected(
                "invalid_manifest_request",
                "manifest paging requires schema 1 and a nonzero page size",
            ));
        }
        if let Some(after) = request.after.as_deref() {
            validate_relative_path(after)?;
        }
        let index = self.load_index()?;
        let scope = self.require_scope(&index, &request.authority)?;
        let manifest = self.load_manifest(scope)?;
        let maximum = usize::from(request.maximum_entries.min(MAX_MANIFEST_ENTRIES));
        let start = request.after.as_ref().map_or(0, |after| {
            manifest.entries.partition_point(|entry| entry.path.as_str() <= after.as_str())
        });
        let end = start.saturating_add(maximum).min(manifest.entries.len());
        let entries = manifest.entries[start..end]
            .iter()
            .map(|entry| ScopedSourceEntry {
                path: entry.path.clone(),
                digest: entry.digest.clone(),
                bytes: entry.bytes,
                executable: entry.executable,
            })
            .collect();
        let next_after = (end < manifest.entries.len())
            .then(|| manifest.entries[end.saturating_sub(1)].path.clone());
        serde_json::to_value(ScopedSourceManifestPage {
            schema: SCHEMA,
            snapshot_digest: scope.snapshot_digest.clone(),
            manifest_artifact: scope.manifest_artifact.clone(),
            entries,
            next_after,
            exclusions: scope.exclusions.clone(),
        })
        .map_err(json_failure)
    }

    fn read(&self, request: ScopedSourceReadRequest) -> Result<serde_json::Value, BrokerError> {
        if request.schema != SCHEMA
            || request.maximum_bytes == 0
            || request.maximum_bytes > MAX_RANGE_BYTES
        {
            return Err(rejected(
                "invalid_read_request",
                "source reads require schema 1 and 1..=1048576 returned bytes",
            ));
        }
        validate_relative_path(&request.path)?;
        let index = self.load_index()?;
        let scope = self.require_scope(&index, &request.authority)?;
        if excluded(&request.path, &scope.exclusions) {
            return Err(rejected(
                "excluded_source",
                "the requested source path is excluded from this scope revision",
            ));
        }
        let manifest = self.load_manifest(scope)?;
        let entry = manifest
            .entries
            .binary_search_by(|entry| entry.path.as_str().cmp(&request.path))
            .ok()
            .map(|index| &manifest.entries[index])
            .ok_or_else(|| {
                rejected(
                    "source_not_in_snapshot",
                    "the requested path is not an included regular file in this snapshot",
                )
            })?;
        if request.offset > entry.bytes {
            return Err(rejected(
                "source_range",
                "the requested source offset exceeds the immutable file length",
            ));
        }
        let end = request
            .offset
            .checked_add(request.maximum_bytes)
            .ok_or_else(overflow)?
            .min(entry.bytes);
        let store = self.artifacts()?;
        let source_digest = parse_digest(&entry.digest)?;
        let mut reader = store.open_read(source_digest).map_err(|_| {
            failed(
                "artifact_integrity",
                "the immutable source artifact is absent, quarantined, or corrupt; preserve this campaign for diagnosis and begin a fresh campaign before reading this handle again",
                false,
            )
        })?;
        let mut selected = Vec::with_capacity(
            usize::try_from(end.saturating_sub(request.offset)).map_err(|_| overflow())?,
        );
        while let Some(chunk) = reader.read_chunk(64 * 1024).map_err(|_| {
            failed("artifact_integrity", "source artifact verification failed during read", false)
        })? {
            let chunk_start = chunk.offset();
            let chunk_end = chunk_start
                .checked_add(u64::try_from(chunk.bytes().len()).map_err(|_| overflow())?)
                .ok_or_else(overflow)?;
            if chunk_end <= request.offset || chunk_start >= end {
                continue;
            }
            let start_in_chunk = usize::try_from(request.offset.saturating_sub(chunk_start))
                .map_err(|_| overflow())?;
            let end_in_chunk =
                usize::try_from(end.min(chunk_end) - chunk_start).map_err(|_| overflow())?;
            selected.extend_from_slice(&chunk.bytes()[start_in_chunk..end_in_chunk]);
        }
        let range_artifact =
            self.publish_artifact(&store, &selected, "application/vnd.peritus.source-range.v1")?;
        let receipt = ReadReceipt {
            schema: SCHEMA,
            handle: scope.handle.clone(),
            scope_revision: scope.scope_revision.clone(),
            snapshot_digest: scope.snapshot_digest.clone(),
            path: request.path.clone(),
            source_digest: entry.digest.clone(),
            range_start: request.offset,
            range_end: end,
            range_artifact: range_artifact.clone(),
        };
        let receipt_bytes = serde_json::to_vec(&receipt).map_err(json_failure)?;
        let receipt_id = sha256_hex(&receipt_bytes);
        let receipt_path = self.root().join("reads").join(format!("{receipt_id}.json"));
        let reused = match read_json::<ReadReceipt>(&receipt_path) {
            Ok(existing) if existing == receipt => true,
            Ok(_) => {
                return Err(failed(
                    "receipt_integrity",
                    "a retained source-read receipt disagrees with its content identity",
                    false,
                ));
            }
            Err(ReadError::Absent) => {
                publish_json(&receipt_path, &receipt)?;
                false
            }
            Err(ReadError::Invalid) => {
                return Err(failed(
                    "receipt_integrity",
                    "a retained source-read receipt is damaged",
                    false,
                ));
            }
        };
        serde_json::to_value(ScopedSourceReadResult {
            schema: SCHEMA,
            snapshot_digest: scope.snapshot_digest.clone(),
            path: request.path,
            source_digest: entry.digest.clone(),
            source_bytes: entry.bytes,
            range_start: request.offset,
            range_end: end,
            range_artifact,
            bytes: selected,
            reused,
        })
        .map_err(json_failure)
    }

    fn revoke(&self, request: ScopedSourceRevokeRequest) -> Result<serde_json::Value, BrokerError> {
        if request.schema != SCHEMA {
            return Err(rejected("invalid_revoke_request", "source revocation requires schema 1"));
        }
        let mut index = self.load_index()?;
        let scope = index.scopes.get_mut(&request.authority.handle).ok_or_else(|| {
            rejected("unknown_source_handle", "the source handle is not retained by this module")
        })?;
        if scope.scope_revision != request.authority.scope_revision {
            return Err(rejected(
                "scope_revision_mismatch",
                "the source handle belongs to a different authority revision",
            ));
        }
        let newly_revoked = !scope.revoked;
        scope.revoked = true;
        if newly_revoked {
            self.save_index(&index)?;
        }
        serde_json::to_value(ScopedSourceRevocation {
            schema: SCHEMA,
            handle: request.authority.handle,
            scope_revision: request.authority.scope_revision,
            newly_revoked,
        })
        .map_err(json_failure)
    }

    fn capture_file(
        &self,
        inspection: &FolderInspection,
        path: &str,
        temporary_root: &Path,
        store: &ArtifactStore,
    ) -> Result<String, BrokerError> {
        let workspace_path = WorkspacePath::new(path.to_owned()).map_err(|_| {
            rejected("unsafe_source_path", "source inventory produced a noncanonical path")
        })?;
        let mut temporary = tempfile::NamedTempFile::new_in(temporary_root).map_err(|_| {
            failed("artifact_io", "source capture temporary file could not be created", true)
        })?;
        let mut write_error = false;
        let observed = inspection
            .scan_file_chunks(
                &workspace_path,
                || self.cancelled(),
                |_, bytes| {
                    if !write_error && temporary.write_all(bytes).is_err() {
                        write_error = true;
                    }
                },
            )
            .map_err(|_| {
                failed(
                    "mixed_source_revision",
                    "source identity or bytes changed during no-follow capture",
                    true,
                )
            })?
            .ok_or_else(|| failed("source_cancelled", "source capture was cancelled", true))?;
        if write_error {
            return Err(failed("artifact_io", "source bytes could not be staged", true));
        }
        temporary
            .as_file_mut()
            .flush()
            .map_err(|_| failed("artifact_io", "staged source bytes could not be flushed", true))?;
        temporary.as_file().sync_all().map_err(|_| {
            failed("artifact_io", "staged source bytes could not be synchronized", true)
        })?;
        let (bytes, digest) = observed;
        let artifact = ArtifactDigest::from_sha256(digest);
        let request = WriteRequest::new(
            artifact,
            bytes,
            bytes.max(1),
            MediaType::new("application/octet-stream").map_err(|_| overflow())?,
            EncryptionMetadata::unencrypted(),
            event_id(artifact),
        );
        let mut writer = store.begin_write(request).map_err(|_| {
            failed("artifact_io", "content-addressed source write could not begin", true)
        })?;
        let mut file = temporary.reopen().map_err(|_| {
            failed("artifact_io", "staged source bytes could not be reopened", true)
        })?;
        let mut chunk = vec![0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut chunk).map_err(|_| {
                failed("artifact_io", "staged source bytes could not be read", true)
            })?;
            if count == 0 {
                break;
            }
            writer.write_chunk(&chunk[..count]).map_err(|_| {
                failed("artifact_integrity", "content-addressed source write failed", false)
            })?;
        }
        writer.finalize().map_err(|_| {
            failed(
                "artifact_integrity",
                "content-addressed source publication failed integrity checks; preserve this campaign for diagnosis and begin a fresh campaign rather than retrying this handle",
                false,
            )
        })?;
        Ok(artifact.to_hex())
    }

    fn publish_artifact(
        &self,
        store: &ArtifactStore,
        bytes: &[u8],
        media_type: &str,
    ) -> Result<String, BrokerError> {
        let digest = ArtifactDigest::new(Sha256::digest(bytes).into());
        let size = u64::try_from(bytes.len()).map_err(|_| overflow())?;
        let request = WriteRequest::new(
            digest,
            size,
            size.max(1),
            MediaType::new(media_type).map_err(|_| overflow())?,
            EncryptionMetadata::unencrypted(),
            event_id(digest),
        );
        let mut writer = store.begin_write(request).map_err(|_| {
            failed("artifact_io", "content-addressed artifact write could not begin", true)
        })?;
        writer.write_chunk(bytes).map_err(|_| {
            failed("artifact_integrity", "content-addressed artifact write failed", false)
        })?;
        writer.finalize().map_err(|_| {
            failed(
                "artifact_integrity",
                "content-addressed artifact publication failed integrity checks; preserve this campaign for diagnosis and begin a fresh campaign rather than retrying this handle",
                false,
            )
        })?;
        Ok(digest.to_hex())
    }

    fn source_root(&self) -> Result<PathBuf, BrokerError> {
        let metadata = fs::symlink_metadata(&self.input.source_root).map_err(|_| {
            failed("source_unavailable", "selected source root is unavailable", true)
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(rejected(
                "unsafe_source_root",
                "selected source root must be a no-follow directory",
            ));
        }
        let source = fs::canonicalize(&self.input.source_root).map_err(|_| {
            failed("source_unavailable", "selected source root cannot be resolved", true)
        })?;
        fs::create_dir_all(self.root()).map_err(|_| {
            failed("state_unavailable", "scoped-source state directory is unavailable", true)
        })?;
        let state = fs::canonicalize(self.root()).map_err(|_| {
            failed("state_unavailable", "scoped-source state directory cannot be resolved", true)
        })?;
        if source.starts_with(&state) || state.starts_with(&source) {
            return Err(rejected(
                "source_root_overlap",
                "selected source overlaps the host's private module state",
            ));
        }
        Ok(source)
    }

    fn effective_exclusions(
        &self,
        requested: Vec<String>,
        source_root: &Path,
    ) -> Result<Vec<String>, BrokerError> {
        let mut exclusions = normalize_exclusions(requested)?.into_iter().collect::<BTreeSet<_>>();
        // Repository and Peritus control data are never application source. Managed Git
        // workspaces do not carry caller-provided protected paths, so excluding these names
        // cannot depend on `ProductWorkspaceKind::protected_paths()`. Keeping the exclusion in
        // the published source authority also makes the omission visible to the module instead
        // of silently admitting metadata that `WorkspacePath` must later reject.
        exclusions.insert(".git".to_owned());
        exclusions.insert(".peritus".to_owned());
        for protected in self.input.workspace_kind.protected_paths() {
            let relative = if protected.is_absolute() {
                match protected.strip_prefix(source_root) {
                    Ok(relative) => relative,
                    Err(_) => continue,
                }
            } else {
                protected.as_path()
            };
            let text = relative.to_str().ok_or_else(|| {
                rejected(
                    "unsafe_protected_path",
                    "a protected source path cannot be represented as canonical UTF-8",
                )
            })?;
            if text.is_empty() {
                return Err(rejected(
                    "source_root_protected",
                    "the entire selected workspace is protected from module source capture",
                ));
            }
            let canonical = text.replace(std::path::MAIN_SEPARATOR, "/");
            validate_relative_path(&canonical)?;
            exclusions.insert(canonical);
        }
        Ok(exclusions.into_iter().collect())
    }

    fn require_scope<'index>(
        &self,
        index: &'index BrokerIndex,
        authority: &ScopedSourceAuthority,
    ) -> Result<&'index ScopeRecord, BrokerError> {
        let scope = index.scopes.get(&authority.handle).ok_or_else(|| {
            rejected("unknown_source_handle", "the source handle is not retained by this module")
        })?;
        if scope.scope_revision != authority.scope_revision {
            return Err(rejected(
                "scope_revision_mismatch",
                "the source handle belongs to a different authority revision",
            ));
        }
        if scope.revoked {
            return Err(rejected(
                "scope_revoked",
                "this source authority has been revoked and cannot be rehydrated",
            ));
        }
        Ok(scope)
    }

    fn load_manifest(&self, scope: &ScopeRecord) -> Result<SourceManifest, BrokerError> {
        let digest = parse_digest(&scope.manifest_artifact)?;
        let bytes = self.artifacts()?.read(digest, ARTIFACT_LIMIT).map_err(|_| {
            failed(
                "artifact_integrity",
                "source manifest artifact is absent, quarantined, or corrupt; preserve this campaign for diagnosis and begin a fresh campaign before using this handle again",
                false,
            )
        })?;
        if sha256_hex(&bytes) != scope.snapshot_digest {
            return Err(failed(
                "manifest_integrity",
                "source manifest identity does not match its retained scope record",
                false,
            ));
        }
        let manifest: SourceManifest = serde_json::from_slice(&bytes).map_err(|_| {
            failed(
                "manifest_integrity",
                "source manifest bytes are not valid schema-one JSON",
                false,
            )
        })?;
        if manifest.schema != SCHEMA {
            return Err(failed(
                "manifest_integrity",
                "source manifest has an unsupported schema",
                false,
            ));
        }
        Ok(manifest)
    }

    fn artifacts(&self) -> Result<ArtifactStore, BrokerError> {
        let config =
            StoreConfig::new_with_quota_policy(self.root().join("artifacts"), ARTIFACT_LIMIT, None)
                .map_err(|_| {
                    failed(
                        "artifact_configuration",
                        "artifact store configuration is invalid",
                        false,
                    )
                })?;
        ArtifactStore::open(config).map_err(|_| {
            failed(
                "artifact_recovery",
                "artifact store recovery found unavailable or corrupt durable state; preserve this campaign for diagnosis and begin a fresh campaign instead of retrying this state",
                false,
            )
        })
    }

    fn recover_temporary_root(&self) -> Result<PathBuf, BrokerError> {
        let temporary_root = self.root().join("temporary");
        fs::create_dir_all(&temporary_root).map_err(|_| {
            failed("artifact_io", "source capture temporary directory is unavailable", true)
        })?;
        let metadata = fs::symlink_metadata(&temporary_root).map_err(|_| {
            failed("artifact_recovery", "source staging directory cannot be inspected", false)
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(failed(
                "artifact_recovery",
                "source staging path is not a private no-follow directory",
                false,
            ));
        }
        let entries = fs::read_dir(&temporary_root)
            .map_err(|_| {
                failed("artifact_recovery", "source staging directory cannot be listed", false)
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| {
                failed("artifact_recovery", "source staging entry cannot be inspected", false)
            })?;
        for entry in entries {
            let name = entry.file_name();
            let name = name.to_str().ok_or_else(|| {
                failed("artifact_recovery", "source staging entry has an invalid name", false)
            })?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|_| {
                failed("artifact_recovery", "source staging entry changed during recovery", false)
            })?;
            if !metadata.is_file() || metadata.file_type().is_symlink() || !name.starts_with(".tmp")
            {
                return Err(failed(
                    "artifact_recovery",
                    "source staging contains an unexpected entry; preserved for diagnosis",
                    false,
                ));
            }
            fs::remove_file(entry.path()).map_err(|_| {
                failed("artifact_recovery", "interrupted source staging could not be removed", true)
            })?;
        }
        #[cfg(unix)]
        fs::File::open(&temporary_root).and_then(|directory| directory.sync_all()).map_err(
            |_| {
                failed(
                    "artifact_recovery",
                    "source staging recovery could not be synchronized",
                    true,
                )
            },
        )?;
        Ok(temporary_root)
    }

    fn root(&self) -> PathBuf {
        self.state.join("scoped-source-v1")
    }

    fn load_index(&self) -> Result<BrokerIndex, BrokerError> {
        let path = self.root().join("index.json");
        match read_json::<BrokerIndex>(&path) {
            Ok(index) if index.schema == SCHEMA => Ok(index),
            Ok(_) | Err(ReadError::Invalid) => Err(failed(
                "scope_state_integrity",
                "scoped-source index is damaged or uses an unsupported schema",
                false,
            )),
            Err(ReadError::Absent) => Ok(BrokerIndex::default()),
        }
    }

    fn save_index(&self, index: &BrokerIndex) -> Result<(), BrokerError> {
        publish_json(&self.root().join("index.json"), index)
    }

    fn cancelled(&self) -> bool {
        self.input.cancelled.load(std::sync::atomic::Ordering::Acquire)
            || self.input.provider_cancellation.is_cancelled()
            || self.input.conversation.revision() != self.revision
    }
}

pub(super) fn authority_live(state: &Path, authority: &ScopedSourceAuthority) -> bool {
    let path = state.join("scoped-source-v1").join("index.json");
    let Ok(index) = read_json::<BrokerIndex>(&path) else {
        return false;
    };
    index.schema == SCHEMA
        && index
            .scopes
            .get(&authority.handle)
            .is_some_and(|scope| scope.scope_revision == authority.scope_revision && !scope.revoked)
}

#[derive(Debug)]
pub(super) enum BrokerError {
    Rejected { code: String, detail: String },
    Failed { code: String, detail: String, retryable: bool },
}

fn rejected(code: &str, detail: &str) -> BrokerError {
    BrokerError::Rejected { code: code.into(), detail: detail.into() }
}

fn failed(code: &str, detail: &str, retryable: bool) -> BrokerError {
    BrokerError::Failed { code: code.into(), detail: detail.into(), retryable }
}

fn overflow() -> BrokerError {
    failed("representation_overflow", "source capture exceeds durable representation", false)
}

fn json_failure(_: serde_json::Error) -> BrokerError {
    failed("json_integrity", "scoped-source state could not be encoded", false)
}

fn decode<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, BrokerError> {
    serde_json::from_value(value).map_err(|_| {
        rejected("invalid_service_input", "scoped-source input does not match contract version 1")
    })
}

fn normalize_exclusions(exclusions: Vec<String>) -> Result<Vec<String>, BrokerError> {
    let mut normalized = BTreeSet::new();
    for exclusion in exclusions {
        validate_relative_path(&exclusion)?;
        normalized.insert(exclusion);
    }
    Ok(normalized.into_iter().collect())
}

fn validate_relative_path(path: &str) -> Result<(), BrokerError> {
    if path.is_empty()
        || path.contains('\0')
        || path.contains('\\')
        || Path::new(path).is_absolute()
        || !Path::new(path).components().all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(rejected(
            "unsafe_source_path",
            "source paths must be canonical relative paths without traversal",
        ));
    }
    Ok(())
}

fn excluded(path: &str, exclusions: &[String]) -> bool {
    exclusions.iter().any(|excluded| {
        path == excluded
            || path.strip_prefix(excluded).is_some_and(|suffix| suffix.starts_with('/'))
    })
}

fn inventory(
    root: &Path,
    exclusions: &[String],
    mut cancelled: impl FnMut() -> bool,
) -> Result<Inventory, BrokerError> {
    let mut pending = vec![(root.to_path_buf(), String::new())];
    let mut files = Vec::new();
    let mut directories = Vec::new();
    while let Some((directory, relative)) = pending.pop() {
        if cancelled() {
            return Err(failed("source_cancelled", "source inventory was cancelled", true));
        }
        let metadata = fs::symlink_metadata(&directory).map_err(|_| {
            failed("mixed_source_revision", "source directory changed during inventory", true)
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(rejected(
                "unsafe_source_link",
                "source inventory refuses linked or non-directory traversal",
            ));
        }
        if !relative.is_empty() {
            directories.push(DirectoryEntry {
                path: relative.clone(),
                version: version_stamp(&metadata)?,
            });
        }
        let mut children = fs::read_dir(&directory)
            .map_err(|_| failed("source_unavailable", "source directory cannot be listed", true))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| {
                failed("source_unavailable", "source directory entry is unavailable", true)
            })?;
        children.sort_unstable_by_key(fs::DirEntry::file_name);
        for child in children.into_iter().rev() {
            let name = child.file_name().into_string().map_err(|_| {
                rejected("unsafe_source_name", "source contains a non-UTF-8 path component")
            })?;
            let path = if relative.is_empty() { name } else { format!("{relative}/{name}") };
            validate_relative_path(&path)?;
            if excluded(&path, exclusions) {
                continue;
            }
            let metadata = fs::symlink_metadata(child.path()).map_err(|_| {
                failed("mixed_source_revision", "source entry changed during inventory", true)
            })?;
            let kind = metadata.file_type();
            if kind.is_symlink() {
                return Err(rejected(
                    "unsafe_source_link",
                    "source contains a symbolic link outside the explicit exclusion set",
                ));
            }
            if metadata.is_dir() {
                pending.push((child.path(), path));
            } else if metadata.is_file() {
                files.push((path, version_stamp(&metadata)?));
            } else {
                return Err(rejected(
                    "unsafe_source_special_file",
                    "source contains a special file outside the explicit exclusion set",
                ));
            }
        }
    }
    files.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    directories.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    Ok(Inventory { files, directories })
}

fn version_stamp(metadata: &fs::Metadata) -> Result<VersionStamp, BrokerError> {
    let modified = metadata
        .modified()
        .map_err(|_| failed("source_metadata", "source modification time is unavailable", true))?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            failed("source_metadata", "source modification time predates the epoch", true)
        })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        Ok(VersionStamp {
            bytes: metadata.len(),
            modified_nanos: format!("{}:{:09}", modified.as_secs(), modified.subsec_nanos()),
            identity: Some(format!("{}:{}", metadata.dev(), metadata.ino())),
            changed: Some(format!("{}:{}", metadata.ctime(), metadata.ctime_nsec())),
            mode: Some(metadata.mode()),
        })
    }
    #[cfg(not(unix))]
    {
        Ok(VersionStamp {
            bytes: metadata.len(),
            modified_nanos: format!("{}:{:09}", modified.as_secs(), modified.subsec_nanos()),
            identity: None,
            changed: None,
            mode: None,
        })
    }
}

fn event_id(digest: ArtifactDigest) -> EventId {
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    if bytes.iter().all(|byte| *byte == 0) {
        bytes[15] = 1;
    }
    EventId::new(bytes).expect("nonzero artifact digest prefix")
}

fn parse_digest(value: &str) -> Result<ArtifactDigest, BrokerError> {
    if value.len() != 64 {
        return Err(failed("artifact_identity", "artifact digest is not canonical SHA-256", false));
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = nibble(pair[0]).ok_or_else(|| {
            failed("artifact_identity", "artifact digest is not canonical SHA-256", false)
        })?;
        let low = nibble(pair[1]).ok_or_else(|| {
            failed("artifact_identity", "artifact digest is not canonical SHA-256", false)
        })?;
        bytes[index] = high * 16 + low;
    }
    Ok(ArtifactDigest::from_sha256(Sha256Digest::new(bytes)))
}

const fn nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    encode_hex(&Sha256::digest(bytes))
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

enum ReadError {
    Absent,
    Invalid,
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, ReadError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| ReadError::Invalid),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(ReadError::Absent),
        Err(_) => Err(ReadError::Invalid),
    }
}

fn publish_json(path: &Path, value: &impl Serialize) -> Result<(), BrokerError> {
    let parent = path.parent().ok_or_else(|| {
        failed("state_unavailable", "durable scoped-source path has no parent", false)
    })?;
    fs::create_dir_all(parent).map_err(|_| {
        failed("state_unavailable", "durable scoped-source directory is unavailable", true)
    })?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| {
        failed("state_unavailable", "durable scoped-source temporary file is unavailable", true)
    })?;
    serde_json::to_writer(&mut temporary, value).map_err(json_failure)?;
    temporary.write_all(b"\n").map_err(|_| {
        failed("state_unavailable", "durable scoped-source state could not be written", true)
    })?;
    temporary.as_file().sync_all().map_err(|_| {
        failed("state_unavailable", "durable scoped-source state could not be synchronized", true)
    })?;
    temporary.persist(path).map_err(|_| {
        failed("state_unavailable", "durable scoped-source state could not be published", true)
    })?;
    #[cfg(unix)]
    fs::File::open(parent).and_then(|directory| directory.sync_all()).map_err(|_| {
        failed(
            "state_unavailable",
            "durable scoped-source directory could not be synchronized",
            true,
        )
    })?;
    Ok(())
}

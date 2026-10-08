//! Digest-bound, chunk-addressed candidate evidence for restart-safe review paging.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use peritus_run_settlement::CandidateIdentity;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{CandidateBaseline, managed, process};
use crate::{ProductRunnerError, ProductRunnerErrorKind, file_metadata};

const CHUNK_BYTES: usize = 32 * 1024;
const MAX_INDEX_RECORD_BYTES: usize = 256 * 1024;
const MARKER: &str = "peritus_candidate_evidence_v1";
const SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ArtifactRef {
    sha256: String,
    bytes: u64,
    descriptor: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ChunkRef {
    sha256: String,
    bytes: u32,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArtifactDescriptor {
    version: u16,
    sha256: String,
    bytes: u64,
    chunks: Vec<ChunkRef>,
}

#[derive(Serialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum IndexRecord<'a> {
    Header {
        version: u16,
        binding: &'a str,
        run_id: String,
        workspace_id: String,
        content_sha256: String,
        repository_sha256: String,
        requirements_revision: u64,
        complete: bool,
        changed_paths: usize,
        exclusions: usize,
    },
    Path {
        path: String,
        before: Option<ContentRecord>,
        after: Option<ContentRecord>,
        before_complete: bool,
    },
    Exclusion { path: String, reason: &'static str },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ContentRecord {
    kind: String,
    mode: Option<String>,
    permissions: Option<u32>,
    object: Option<String>,
    artifact: Option<ArtifactRef>,
    handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unavailable: Option<UnavailableContent>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UnavailableContent {
    sha256: String,
    bytes: u64,
}

#[derive(Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum DecodedIndexRecord {
    Header {
        version: u16,
        binding: String,
        run_id: String,
        workspace_id: String,
        content_sha256: String,
        repository_sha256: String,
        requirements_revision: u64,
        complete: bool,
        changed_paths: usize,
        exclusions: usize,
    },
    Path {
        path: String,
        before: Option<ContentRecord>,
        after: Option<ContentRecord>,
        before_complete: bool,
    },
    Exclusion { path: String, reason: String },
}

struct PendingEntry {
    path: PathBuf,
    before: Option<PendingContent>,
    after: Option<PendingContent>,
    before_complete: bool,
}

enum PendingContent {
    Managed(managed::EvidenceContent),
    File {
        content: crate::workspace_delivery::scope::EvidenceContent,
        current: bool,
    },
    UnavailableFile(crate::workspace_delivery::scope::UnavailableEvidenceContent),
    Metadata { kind: String, permissions: Option<u32> },
}

enum Materialized {
    Ready(Option<ContentRecord>),
    Superseded,
}

pub(crate) struct Published {
    preview: String,
}

impl Published {
    pub(crate) fn preview(self) -> String { self.preview }
}

pub(crate) fn publish(
    root: &Path,
    baseline: &CandidateBaseline,
    trace: &Path,
    identity: CandidateIdentity,
    cancellation: process::Cancellation,
) -> Result<Option<Published>, ProductRunnerError> {
    process::with_cancellation(cancellation.clone(), || {
        publish_inner(root, baseline, trace, identity, &cancellation)
    })
}

fn publish_inner(
    root: &Path,
    baseline: &CandidateBaseline,
    trace: &Path,
    identity: CandidateIdentity,
    cancellation: &process::Cancellation,
) -> Result<Option<Published>, ProductRunnerError> {
    cancellation.check("publish candidate evidence")?;
    let binding = binding(identity);
    let store = StorePaths::new(trace);
    store.prepare()?;
    let (pending, exclusions, mut complete) = if let Some(managed) = baseline.managed() {
        let snapshot = managed.evidence_snapshot(root)?;
        if snapshot.content_digest != identity.content_digest()
            || snapshot.repository_digest != identity.repository_digest()
        {
            return Ok(None);
        }
        let entries = snapshot.entries.into_iter().map(|entry| PendingEntry {
            path: entry.path,
            before: entry.before.map(PendingContent::Managed),
            after: entry.after.map(PendingContent::Managed),
            before_complete: true,
        }).collect();
        (entries, snapshot.exclusions, true)
    } else if let Some(scope) = baseline.scope() {
        let mut entries = Vec::new();
        let mut scope_complete = true;
        let snapshot = scope.evidence_snapshot(root)?;
        if snapshot.content_digest != identity.content_digest()
            || snapshot.repository_digest != identity.repository_digest()
        {
            return Ok(None);
        }
        for entry in snapshot.entries {
            let before_complete = !entry.legacy_preimage_missing;
            scope_complete &= before_complete;
            entries.push(PendingEntry {
                path: entry.path,
                before: entry.before.map(|content| PendingContent::File {
                    content,
                    current: false,
                }).or_else(|| {
                    entry.before_unavailable.map(PendingContent::UnavailableFile)
                }).or_else(|| {
                    (entry.before_kind != "absent").then(|| PendingContent::Metadata {
                        kind: entry.before_kind,
                        permissions: entry.before_permissions,
                    })
                }),
                after: entry.after.map(|content| PendingContent::File {
                    content,
                    current: true,
                }).or_else(|| {
                    (entry.after_kind != "absent").then(|| PendingContent::Metadata {
                        kind: entry.after_kind,
                        permissions: entry.after_permissions,
                    })
                }),
                before_complete,
            });
        }
        (entries, Vec::new(), scope_complete)
    } else {
        if !legacy_identity_is_current(root, baseline, identity, cancellation)? {
            return Ok(None);
        }
        let mut entries = Vec::new();
        for path in baseline.changed_paths(root)? {
            cancellation.check("publish legacy candidate evidence")?;
            let absolute = root.join(&path);
            let after = match fs::symlink_metadata(&absolute) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    let (sha256, bytes) = digest_file(&absolute, cancellation)?;
                    Some(PendingContent::File {
                        content: crate::workspace_delivery::scope::EvidenceContent {
                            path: absolute,
                            sha256,
                            bytes,
                            permissions: file_metadata::permission_fingerprint(&metadata),
                        },
                        current: true,
                    })
                }
                Ok(metadata) => Some(PendingContent::Metadata {
                    kind: if metadata.file_type().is_symlink() { "symlink" } else { "directory" }.to_owned(),
                    permissions: Some(file_metadata::permission_fingerprint(&metadata)),
                }),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(failure("inspect legacy candidate path", error)),
            };
            entries.push(PendingEntry { path, before: None, after, before_complete: false });
        }
        (entries, Vec::new(), false)
    };

    let mut cached = BTreeMap::<(PathBuf, String), ArtifactRef>::new();
    let mut records = Vec::with_capacity(pending.len());
    for entry in pending {
        cancellation.check("materialize candidate evidence")?;
        let before = match materialize(
            entry.before,
            &binding,
            &store,
            cancellation,
            &mut cached,
        )? {
            Materialized::Ready(content) => content,
            Materialized::Superseded => return Ok(None),
        };
        let after = match materialize(
            entry.after,
            &binding,
            &store,
            cancellation,
            &mut cached,
        )? {
            Materialized::Ready(content) => content,
            Materialized::Superseded => return Ok(None),
        };
        records.push((entry.path, before, after, entry.before_complete));
    }
    complete &= records.iter().all(|(_, _, _, before_complete)| *before_complete);

    let mut index = ArtifactBuilder::new(&store);
    write_record(&mut index, &IndexRecord::Header {
        version: SCHEMA_VERSION,
        binding: &binding,
        run_id: hex(identity.run_id().as_bytes()),
        workspace_id: hex(identity.workspace_id().as_bytes()),
        content_sha256: hex(identity.content_digest().as_bytes()),
        repository_sha256: hex(identity.repository_digest().as_bytes()),
        requirements_revision: identity.requirements_revision(),
        complete,
        changed_paths: records.len(),
        exclusions: exclusions.len(),
    })?;
    for (path, before, after, before_complete) in &records {
        write_record(&mut index, &IndexRecord::Path {
            path: path_text(path)?,
            before: before.clone(),
            after: after.clone(),
            before_complete: *before_complete,
        })?;
    }
    for path in &exclusions {
        write_record(&mut index, &IndexRecord::Exclusion {
            path: path_text(path)?,
            reason: "nonignored untracked generated path excluded by workspace policy",
        })?;
    }
    let index = index.finish()?;
    let handle = index_handle(&binding, &index.descriptor);
    let mut preview = format!(
        "{MARKER} binding={binding} handle={handle} bytes={} sha256={} complete={} changed_paths={} exclusions={}\n",
        index.bytes,
        index.sha256,
        complete,
        records.len(),
        exclusions.len(),
    );
    preview.push_str(
        "This is a bounded candidate preview. The digest-bound index and its per-file before/current content handles are the complete review evidence. Binary content is paged as hexadecimal bytes.\n\n",
    );
    for (path, before, after, before_complete) in records.iter().take(256) {
        let _ = writeln!(
            preview,
            "{}: before={} after={} before_complete={before_complete}",
            path.display(),
            summary(before.as_ref()),
            summary(after.as_ref()),
        );
    }
    if records.len() > 256 {
        let _ = writeln!(preview, "[{} additional paths are indexed]", records.len() - 256);
    }
    Ok(Some(Published { preview: crate::bundle::limit_text(&preview, 256 * 1024) }))
}

fn legacy_identity_is_current(
    root: &Path,
    baseline: &CandidateBaseline,
    identity: CandidateIdentity,
    cancellation: &process::Cancellation,
) -> Result<bool, ProductRunnerError> {
    loop {
        cancellation.check("bind legacy candidate evidence")?;
        let before = baseline.checkpoint(root)?.digest();
        let content = baseline.content_digest(root)?;
        let after = baseline.checkpoint(root)?.digest();
        cancellation.check("bind legacy candidate evidence")?;
        if before == after {
            if content != identity.content_digest() || after != identity.repository_digest() {
                return Ok(false);
            }
            return Ok(true);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn materialize(
    pending: Option<PendingContent>,
    binding: &str,
    store: &StorePaths,
    cancellation: &process::Cancellation,
    cached: &mut BTreeMap<(PathBuf, String), ArtifactRef>,
) -> Result<Materialized, ProductRunnerError> {
    let Some(pending) = pending else { return Ok(Materialized::Ready(None)) };
    match pending {
        PendingContent::Managed(content) if content.mode == "160000" => Ok(Materialized::Ready(Some(ContentRecord {
            kind: "repository".to_owned(),
            mode: Some(content.mode),
            permissions: Some(content.permissions),
            object: Some(content.object),
            artifact: None,
            handle: None,
            unavailable: None,
        }))),
        PendingContent::Managed(content) => {
            let key = (content.repository.clone(), content.object.clone());
            let artifact = if let Some(artifact) = cached.get(&key) {
                artifact.clone()
            } else {
                let mut command = Command::new("git");
                command.args(["cat-file", "blob", &content.object]).current_dir(&content.repository);
                let mut builder = ArtifactBuilder::new(store);
                let completed = process::stream(
                    command,
                    None,
                    &mut builder,
                    cancellation,
                    "stream candidate Git object",
                )?;
                if !completed.status.success() {
                    return Err(failure(
                        "stream candidate Git object",
                        String::from_utf8_lossy(&completed.stderr),
                    ));
                }
                let artifact = builder.finish()?;
                cached.insert(key, artifact.clone());
                artifact
            };
            Ok(Materialized::Ready(Some(ContentRecord {
                kind: if content.mode == "120000" { "symlink" } else { "file" }.to_owned(),
                mode: Some(content.mode),
                permissions: Some(content.permissions),
                object: Some(content.object),
                handle: Some(object_handle(binding, &artifact.descriptor)),
                artifact: Some(artifact),
                unavailable: None,
            })))
        }
        PendingContent::File { content, current } => {
            let mut source = fs::File::open(&content.path)
                .map_err(|error| failure("read candidate evidence file", error))?;
            let mut builder = ArtifactBuilder::new(store);
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                cancellation.check("read candidate evidence file")?;
                let count = source.read(&mut buffer)
                    .map_err(|error| failure("read candidate evidence file", error))?;
                if count == 0 { break; }
                builder.write_all(&buffer[..count])
                    .map_err(|error| failure("store candidate evidence file", error))?;
            }
            let artifact = builder.finish()?;
            if artifact.sha256 != hex(&content.sha256) || artifact.bytes != content.bytes {
                if current {
                    return Ok(Materialized::Superseded);
                }
                return Err(failure(
                    "read candidate evidence file",
                    "retained candidate preimage conflicts with its recorded digest",
                ));
            }
            Ok(Materialized::Ready(Some(ContentRecord {
                kind: "file".to_owned(),
                mode: None,
                permissions: Some(content.permissions),
                object: None,
                handle: Some(object_handle(binding, &artifact.descriptor)),
                artifact: Some(artifact),
                unavailable: None,
            })))
        }
        PendingContent::UnavailableFile(content) => {
            Ok(Materialized::Ready(Some(ContentRecord {
                kind: "file".to_owned(),
                mode: None,
                permissions: Some(content.permissions),
                object: None,
                artifact: None,
                handle: None,
                unavailable: Some(UnavailableContent {
                    sha256: hex(&content.sha256),
                    bytes: content.bytes,
                }),
            })))
        }
        PendingContent::Metadata { kind, permissions } => Ok(Materialized::Ready(Some(ContentRecord {
            kind,
            mode: None,
            permissions,
            object: None,
            artifact: None,
            handle: None,
            unavailable: None,
        }))),
    }
}

fn write_record(
    writer: &mut ArtifactBuilder,
    record: &IndexRecord<'_>,
) -> Result<(), ProductRunnerError> {
    serde_json::to_writer(&mut *writer, record)
        .map_err(|error| failure("encode candidate evidence index", error))?;
    writer.write_all(b"\n")
        .map_err(|error| failure("encode candidate evidence index", error))
}

struct StorePaths { root: PathBuf }

impl StorePaths {
    fn new(trace: &Path) -> Self {
        Self { root: trace.with_extension("candidate-evidence-v1") }
    }

    fn prepare(&self) -> Result<(), ProductRunnerError> {
        for path in [self.root.clone(), self.chunks(), self.descriptors()] {
            fs::create_dir_all(&path)
                .map_err(|error| failure("prepare candidate evidence store", error))?;
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| failure("prepare candidate evidence store", error))?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(failure(
                    "prepare candidate evidence store",
                    "candidate evidence path is not an owned directory",
                ));
            }
        }
        #[cfg(unix)]
        {
            for path in [self.chunks(), self.descriptors(), self.root.clone()] {
                fs::File::open(path)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|error| failure("sync candidate evidence store", error))?;
            }
            let parent = self.root.parent().ok_or_else(|| {
                failure("sync candidate evidence store", "candidate evidence store has no parent")
            })?;
            fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| failure("sync candidate evidence store", error))?;
        }
        Ok(())
    }

    fn chunks(&self) -> PathBuf { self.root.join("chunks") }
    fn descriptors(&self) -> PathBuf { self.root.join("descriptors") }
    fn chunk(&self, digest: &str) -> PathBuf { self.chunks().join(digest) }
    fn descriptor(&self, digest: &str) -> PathBuf { self.descriptors().join(format!("{digest}.json")) }
}

struct ArtifactBuilder<'a> {
    store: &'a StorePaths,
    raw: Sha256,
    bytes: u64,
    pending: Vec<u8>,
    chunks: Vec<ChunkRef>,
}

impl<'a> ArtifactBuilder<'a> {
    fn new(store: &'a StorePaths) -> Self {
        Self { store, raw: Sha256::new(), bytes: 0, pending: Vec::with_capacity(CHUNK_BYTES), chunks: Vec::new() }
    }

    fn flush_chunk(&mut self) -> std::io::Result<()> {
        if self.pending.is_empty() { return Ok(()) }
        let digest = hex(&Sha256::digest(&self.pending));
        persist_bytes(&self.store.chunk(&digest), &self.pending, &digest)?;
        self.chunks.push(ChunkRef {
            sha256: digest,
            bytes: u32::try_from(self.pending.len()).map_err(|_| std::io::Error::other("candidate chunk length overflow"))?,
        });
        self.pending.clear();
        Ok(())
    }

    fn finish(mut self) -> Result<ArtifactRef, ProductRunnerError> {
        self.flush_chunk().map_err(|error| failure("store candidate evidence chunk", error))?;
        let raw = hex(&self.raw.finalize());
        let descriptor = ArtifactDescriptor {
            version: SCHEMA_VERSION,
            sha256: raw.clone(),
            bytes: self.bytes,
            chunks: self.chunks,
        };
        let bytes = serde_json::to_vec(&descriptor)
            .map_err(|error| failure("encode candidate evidence descriptor", error))?;
        let descriptor_digest = hex(&Sha256::digest(&bytes));
        persist_bytes(&self.store.descriptor(&descriptor_digest), &bytes, &descriptor_digest)
            .map_err(|error| failure("store candidate evidence descriptor", error))?;
        Ok(ArtifactRef { sha256: raw, bytes: self.bytes, descriptor: descriptor_digest })
    }
}

impl Write for ArtifactBuilder<'_> {
    fn write(&mut self, mut bytes: &[u8]) -> std::io::Result<usize> {
        let total = bytes.len();
        self.raw.update(bytes);
        self.bytes = self.bytes.checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("candidate artifact length overflow"))?;
        while !bytes.is_empty() {
            let count = (CHUNK_BYTES - self.pending.len()).min(bytes.len());
            self.pending.extend_from_slice(&bytes[..count]);
            bytes = &bytes[count..];
            if self.pending.len() == CHUNK_BYTES { self.flush_chunk()?; }
        }
        Ok(total)
    }

    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

fn persist_bytes(path: &Path, bytes: &[u8], digest: &str) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| std::io::Error::other("candidate artifact has no parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(path) {
        Ok(_) => fs::File::open(parent)?.sync_all(),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(std::io::Error::other(
                    "candidate content-addressed object is not a regular file",
                ));
            }
            let existing = fs::read(path)?;
            if existing.len() != bytes.len() || hex(&Sha256::digest(&existing)) != digest {
                Err(std::io::Error::other("candidate content-addressed object conflicts with its digest"))
            } else {
                Ok(())
            }
        }
        Err(error) => Err(error.error),
    }
}

fn digest_file(
    path: &Path,
    cancellation: &process::Cancellation,
) -> Result<([u8; 32], u64), ProductRunnerError> {
    let mut file = fs::File::open(path).map_err(|error| failure("read legacy candidate file", error))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        cancellation.check("read legacy candidate file")?;
        let count = file.read(&mut buffer).map_err(|error| failure("read legacy candidate file", error))?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
        bytes = bytes.checked_add(count as u64).ok_or_else(|| failure("read legacy candidate file", "file length overflow"))?;
    }
    Ok((hasher.finalize().into(), bytes))
}

fn binding(identity: CandidateIdentity) -> String {
    let mut value = Sha256::new();
    value.update(b"peritus/candidate-evidence/v1\0");
    value.update(identity.run_id().as_bytes());
    value.update(identity.workspace_id().as_bytes());
    value.update(identity.content_digest().as_bytes());
    value.update(identity.repository_digest().as_bytes());
    value.update(identity.requirements_revision().to_be_bytes());
    hex(&value.finalize())
}

fn index_handle(binding: &str, descriptor: &str) -> String {
    format!("candidate:{binding}:index:{descriptor}:utf8")
}

fn object_handle(binding: &str, descriptor: &str) -> String {
    format!("candidate:{binding}:object:{descriptor}:hex")
}

fn summary(content: Option<&ContentRecord>) -> String {
    match content {
        None => "absent".to_owned(),
        Some(content) => match (&content.artifact, &content.unavailable) {
            (Some(artifact), _) => format!(
                "{} bytes={} sha256={} handle={}",
                content.kind,
                artifact.bytes,
                artifact.sha256,
                content.handle.as_deref().unwrap_or("missing"),
            ),
            (None, Some(unavailable)) => format!(
                "{} bytes={} sha256={} unavailable=true",
                content.kind, unavailable.bytes, unavailable.sha256,
            ),
            (None, None) => format!(
                "{} object={}",
                content.kind,
                content.object.as_deref().unwrap_or("none"),
            ),
        },
    }
}

fn path_text(path: &Path) -> Result<String, ProductRunnerError> {
    path.to_str().map(ToOwned::to_owned)
        .ok_or_else(|| failure("encode candidate evidence path", "candidate path is not UTF-8"))
}

fn failure(operation: &'static str, detail: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, operation, detail.to_string())
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes { let _ = write!(output, "{byte:02x}"); }
    output
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Encoding { Utf8, Hex }

pub(crate) struct Source {
    pub(crate) handle: String,
    pub(crate) label: String,
    pub(crate) sha256: String,
    pub(crate) bytes: u64,
    pub(crate) encoding: Encoding,
    descriptor: Arc<ArtifactDescriptor>,
}

pub(crate) struct Page {
    pub(crate) end: u64,
    pub(crate) next: Option<u64>,
    pub(crate) text: String,
}

pub(crate) struct ReviewStore {
    paths: StorePaths,
    binding: String,
    index: ArtifactRef,
    index_descriptor: Arc<ArtifactDescriptor>,
    allowed: BTreeMap<String, ArtifactRef>,
    descriptors: Mutex<BTreeMap<String, Arc<ArtifactDescriptor>>>,
    complete: bool,
}

impl ReviewStore {
    pub(crate) fn open(
        trace: &Path,
        identity: CandidateIdentity,
        preview: &str,
    ) -> Result<Option<Self>, ProductRunnerError> {
        let Some(line) = preview.lines().next().filter(|line| line.starts_with(MARKER)) else {
            return Ok(None);
        };
        let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
        if fields.len() != 8 || fields[0] != MARKER {
            return Err(failure("restore candidate evidence", "candidate evidence marker is malformed"));
        }
        let expected_binding = binding(identity);
        if marker_value(&fields, "binding=")? != expected_binding {
            return Err(failure("restore candidate evidence", "candidate evidence binding is stale"));
        }
        let handle = marker_value(&fields, "handle=")?;
        let handle_descriptor = parse_handle(handle, &expected_binding, "index", "utf8")?;
        let index = ArtifactRef {
            sha256: marker_value(&fields, "sha256=")?.to_owned(),
            bytes: marker_value(&fields, "bytes=")?.parse().map_err(|_| failure("restore candidate evidence", "candidate index length is invalid"))?,
            descriptor: handle_descriptor.to_owned(),
        };
        let complete = marker_value(&fields, "complete=")?.parse()
            .map_err(|_| failure("restore candidate evidence", "candidate completeness flag is invalid"))?;
        let expected_paths = marker_value(&fields, "changed_paths=")?.parse()
            .map_err(|_| failure("restore candidate evidence", "candidate path count is invalid"))?;
        let expected_exclusions = marker_value(&fields, "exclusions=")?.parse()
            .map_err(|_| failure("restore candidate evidence", "candidate exclusion count is invalid"))?;
        let paths = StorePaths::new(trace);
        let index_descriptor = Arc::new(descriptor(&paths, &index)?);
        let mut allowed = BTreeMap::new();
        let mut header = false;
        let mut path_count = 0_usize;
        let mut exclusion_count = 0_usize;
        read_index_records(&paths, &index, &index_descriptor, |line| {
            let record: DecodedIndexRecord = serde_json::from_slice(line)
                .map_err(|error| failure("restore candidate evidence", error))?;
            match record {
                DecodedIndexRecord::Header {
                    version, binding, run_id, workspace_id, content_sha256,
                    repository_sha256, requirements_revision, complete: indexed_complete,
                    changed_paths, exclusions,
                } => {
                    if header || version != SCHEMA_VERSION || binding != expected_binding
                        || run_id != hex(identity.run_id().as_bytes())
                        || workspace_id != hex(identity.workspace_id().as_bytes())
                        || content_sha256 != hex(identity.content_digest().as_bytes())
                        || repository_sha256 != hex(identity.repository_digest().as_bytes())
                        || requirements_revision != identity.requirements_revision()
                        || indexed_complete != complete
                        || changed_paths != expected_paths
                        || exclusions != expected_exclusions
                    {
                        return Err(failure("restore candidate evidence", "candidate index header conflicts with its marker"));
                    }
                    header = true;
                }
                DecodedIndexRecord::Path { path, before, after, before_complete } => {
                    if !header || !valid_relative_path(&path)
                        || (!before_complete && complete)
                    {
                        return Err(failure("restore candidate evidence", "candidate path record is invalid"));
                    }
                    for (content, allow_unavailable) in
                        [(before, !before_complete), (after, false)]
                    {
                        let Some(content) = content else { continue };
                        validate_content_record(&content, allow_unavailable)?;
                        let artifact = match (content.artifact, content.handle) {
                            (Some(artifact), Some(handle))
                                if handle == object_handle(&expected_binding, &artifact.descriptor) => artifact,
                            (None, None) => continue,
                            _ => return Err(failure("restore candidate evidence", "candidate content handle is incomplete")),
                        };
                        validate_descriptor(&paths, &artifact)?;
                        if allowed.insert(artifact.descriptor.clone(), artifact.clone())
                            .is_some_and(|existing| existing != artifact)
                        {
                            return Err(failure(
                                "restore candidate evidence",
                                "candidate descriptor has conflicting index identities",
                            ));
                        }
                    }
                    path_count = path_count.checked_add(1)
                        .ok_or_else(|| failure("restore candidate evidence", "candidate path count overflow"))?;
                }
                DecodedIndexRecord::Exclusion { path, reason } => {
                    if !header || !valid_relative_path(&path) || reason.is_empty() {
                        return Err(failure("restore candidate evidence", "candidate exclusion record is invalid"));
                    }
                    exclusion_count = exclusion_count.checked_add(1)
                        .ok_or_else(|| failure("restore candidate evidence", "candidate exclusion count overflow"))?;
                }
            }
            Ok(())
        })?;
        if !header
            || path_count != expected_paths
            || exclusion_count != expected_exclusions
        {
            return Err(failure("restore candidate evidence", "candidate index frontier is incomplete"));
        }
        Ok(Some(Self {
            paths,
            binding: expected_binding,
            index,
            index_descriptor,
            allowed,
            descriptors: Mutex::new(BTreeMap::new()),
            complete,
        }))
    }

    pub(crate) fn complete(&self) -> bool { self.complete }

    pub(crate) fn index_source(&self) -> Source {
        Source {
            handle: index_handle(&self.binding, &self.index.descriptor),
            label: "candidate_index".to_owned(),
            sha256: self.index.sha256.clone(),
            bytes: self.index.bytes,
            encoding: Encoding::Utf8,
            descriptor: Arc::clone(&self.index_descriptor),
        }
    }

    pub(crate) fn source(&self, handle: &str) -> Result<Source, ProductRunnerError> {
        if handle == index_handle(&self.binding, &self.index.descriptor) {
            return Ok(self.index_source());
        }
        let descriptor = parse_handle(handle, &self.binding, "object", "hex")?;
        let artifact = self.allowed.get(descriptor).cloned()
            .ok_or_else(|| failure("read candidate evidence", "candidate artifact handle is outside the bound index"))?;
        let parsed = self.cached_descriptor(&artifact)?;
        Ok(Source {
            handle: handle.to_owned(),
            label: "candidate_content".to_owned(),
            sha256: artifact.sha256.clone(),
            bytes: artifact.bytes,
            encoding: Encoding::Hex,
            descriptor: parsed,
        })
    }

    pub(crate) fn page(
        &self,
        source: &Source,
        offset: u64,
        maximum_raw_bytes: usize,
    ) -> Result<Page, ProductRunnerError> {
        if offset > source.bytes {
            return Err(failure("read candidate evidence", "candidate evidence offset is outside the source"));
        }
        let count = usize::try_from((source.bytes - offset).min(maximum_raw_bytes as u64))
            .map_err(|_| failure("read candidate evidence", "candidate page length overflow"))?;
        let mut bytes = read_range(&self.paths, &source.descriptor, offset, count)?;
        if source.encoding == Encoding::Utf8 {
            while std::str::from_utf8(&bytes).is_err() && !bytes.is_empty() { bytes.pop(); }
            if bytes.is_empty() && maximum_raw_bytes != 0 && offset < source.bytes {
                return Err(failure("read candidate evidence", "candidate UTF-8 page starts inside a character"));
            }
        }
        let consumed = u64::try_from(bytes.len())
            .map_err(|_| failure("read candidate evidence", "candidate page length overflow"))?;
        let end = offset.checked_add(consumed)
            .ok_or_else(|| failure("read candidate evidence", "candidate page offset overflow"))?;
        let text = match source.encoding {
            Encoding::Utf8 => String::from_utf8(bytes)
                .map_err(|_| failure("read candidate evidence", "candidate index is not UTF-8"))?,
            Encoding::Hex => hex(&bytes),
        };
        Ok(Page { end, next: (end < source.bytes).then_some(end), text })
    }

    fn cached_descriptor(
        &self,
        artifact: &ArtifactRef,
    ) -> Result<Arc<ArtifactDescriptor>, ProductRunnerError> {
        let mut descriptors = self.descriptors.lock()
            .map_err(|_| failure("read candidate evidence", "candidate descriptor cache is poisoned"))?;
        if let Some(descriptor) = descriptors.get(&artifact.descriptor) {
            return Ok(Arc::clone(descriptor));
        }
        let parsed = Arc::new(descriptor(&self.paths, artifact)?);
        descriptors.insert(artifact.descriptor.clone(), Arc::clone(&parsed));
        Ok(parsed)
    }
}

fn marker_value<'a>(
    fields: &[&'a str],
    name: &str,
) -> Result<&'a str, ProductRunnerError> {
    fields.iter().find_map(|field| field.strip_prefix(name))
        .ok_or_else(|| failure(
            "restore candidate evidence",
            "candidate evidence marker is incomplete",
        ))
}

fn parse_handle<'a>(
    handle: &'a str,
    binding: &str,
    kind: &str,
    encoding: &str,
) -> Result<&'a str, ProductRunnerError> {
    let prefix = format!("candidate:{binding}:{kind}:");
    let descriptor = handle.strip_prefix(&prefix)
        .and_then(|value| value.strip_suffix(&format!(":{encoding}")))
        .ok_or_else(|| failure("read candidate evidence", "candidate evidence handle is invalid"))?;
    if !valid_hex(descriptor) {
        return Err(failure("read candidate evidence", "candidate evidence descriptor is invalid"));
    }
    Ok(descriptor)
}

fn validate_descriptor(paths: &StorePaths, artifact: &ArtifactRef) -> Result<(), ProductRunnerError> {
    descriptor(paths, artifact).map(|_| ())
}

fn validate_content_record(
    content: &ContentRecord,
    allow_unavailable: bool,
) -> Result<(), ProductRunnerError> {
    let has_artifact = content.artifact.is_some();
    if has_artifact != content.handle.is_some() || content.permissions.is_none() {
        return Err(failure(
            "restore candidate evidence",
            "candidate content permissions, artifact, and handle are incomplete",
        ));
    }
    if content.unavailable.is_some() && !allow_unavailable {
        return Err(failure(
            "restore candidate evidence",
            "candidate unavailable content is not an incomplete historical preimage",
        ));
    }
    match content.kind.as_str() {
        "file" => match (
            &content.mode,
            &content.object,
            has_artifact,
            content.unavailable.as_ref(),
        ) {
            (Some(mode), Some(object), true, None)
                if matches!(mode.as_str(), "100644" | "100755")
                    && valid_git_object(object) => {}
            (None, None, true, None) => {}
            (None, None, false, Some(unavailable))
                if valid_hex(&unavailable.sha256) => {}
            _ => return Err(failure(
                "restore candidate evidence",
                "candidate file metadata is inconsistent",
            )),
        },
        "symlink" => match (
            &content.mode,
            &content.object,
            has_artifact,
            content.unavailable.as_ref(),
        ) {
            (Some(mode), Some(object), true, None)
                if mode.as_str() == "120000" && valid_git_object(object) => {}
            (None, None, false, None) => {}
            _ => return Err(failure(
                "restore candidate evidence",
                "candidate symlink metadata is inconsistent",
            )),
        },
        "repository"
            if !has_artifact
                && content.unavailable.is_none()
                && content.mode.as_deref() == Some("160000")
                && content.object.as_deref().is_some_and(valid_git_object) => {}
        "directory"
            if !has_artifact
                && content.unavailable.is_none()
                && content.mode.is_none()
                && content.object.is_none() => {}
        _ => return Err(failure(
            "restore candidate evidence",
            "candidate content kind is invalid",
        )),
    }
    Ok(())
}

fn descriptor(paths: &StorePaths, artifact: &ArtifactRef) -> Result<ArtifactDescriptor, ProductRunnerError> {
    if !valid_hex(&artifact.sha256) || !valid_hex(&artifact.descriptor) {
        return Err(failure(
            "read candidate artifact descriptor",
            "candidate artifact identity is invalid",
        ));
    }
    let bytes = read_regular(
        &paths.descriptor(&artifact.descriptor),
        "read candidate artifact descriptor",
    )?;
    if hex(&Sha256::digest(&bytes)) != artifact.descriptor {
        return Err(failure("read candidate artifact descriptor", "candidate descriptor digest mismatch"));
    }
    let descriptor: ArtifactDescriptor = serde_json::from_slice(&bytes)
        .map_err(|error| failure("read candidate artifact descriptor", error))?;
    validate_descriptor_shape(&descriptor)?;
    if descriptor.sha256 != artifact.sha256 || descriptor.bytes != artifact.bytes {
        return Err(failure("read candidate artifact descriptor", "candidate descriptor identity mismatch"));
    }
    Ok(descriptor)
}

fn validate_descriptor_shape(descriptor: &ArtifactDescriptor) -> Result<(), ProductRunnerError> {
    let last = descriptor.chunks.len().saturating_sub(1);
    let total = descriptor.chunks.iter().enumerate().try_fold(0_u64, |total, (index, chunk)| {
        if !valid_hex(&chunk.sha256) || chunk.bytes == 0 || chunk.bytes as usize > CHUNK_BYTES {
            return Err(failure("read candidate artifact descriptor", "candidate chunk descriptor is invalid"));
        }
        if index != last && chunk.bytes as usize != CHUNK_BYTES {
            return Err(failure(
                "read candidate artifact descriptor",
                "candidate artifact has a short nonterminal chunk",
            ));
        }
        total.checked_add(u64::from(chunk.bytes))
            .ok_or_else(|| failure("read candidate artifact descriptor", "candidate artifact length overflow"))
    })?;
    if descriptor.version != SCHEMA_VERSION || !valid_hex(&descriptor.sha256)
        || total != descriptor.bytes || (descriptor.bytes == 0) != descriptor.chunks.is_empty()
    {
        return Err(failure("read candidate artifact descriptor", "candidate artifact descriptor is inconsistent"));
    }
    Ok(())
}

fn read_index_records(
    paths: &StorePaths,
    artifact: &ArtifactRef,
    descriptor: &ArtifactDescriptor,
    mut observe: impl FnMut(&[u8]) -> Result<(), ProductRunnerError>,
) -> Result<(), ProductRunnerError> {
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut record = Vec::new();
    for chunk in &descriptor.chunks {
        let bytes = read_chunk(paths, chunk)?;
        digest.update(&bytes);
        total = total.checked_add(bytes.len() as u64)
            .ok_or_else(|| failure("restore candidate evidence", "candidate index length overflow"))?;
        for byte in bytes {
            if byte == b'\n' {
                if record.is_empty() {
                    return Err(failure(
                        "restore candidate evidence",
                        "candidate index contains an empty record",
                    ));
                }
                observe(&record)?;
                record.clear();
            } else {
                if record.len() == MAX_INDEX_RECORD_BYTES {
                    return Err(failure(
                        "restore candidate evidence",
                        "candidate index record exceeds its physical bound",
                    ));
                }
                record.push(byte);
            }
        }
    }
    if !record.is_empty() {
        return Err(failure(
            "restore candidate evidence",
            "candidate index ends with a torn record",
        ));
    }
    if total != artifact.bytes || hex(&digest.finalize()) != artifact.sha256 {
        return Err(failure("restore candidate evidence", "candidate index digest mismatch"));
    }
    Ok(())
}

fn read_range(
    paths: &StorePaths,
    descriptor: &ArtifactDescriptor,
    offset: u64,
    count: usize,
) -> Result<Vec<u8>, ProductRunnerError> {
    let mut output = Vec::with_capacity(count);
    let first = usize::try_from(offset / CHUNK_BYTES as u64)
        .map_err(|_| failure("read candidate evidence", "candidate page offset exceeds native addressing"))?;
    let mut cursor = u64::try_from(first)
        .map_err(|_| failure("read candidate evidence", "candidate page offset exceeds its representation"))?
        .checked_mul(CHUNK_BYTES as u64)
        .ok_or_else(|| failure("read candidate evidence", "candidate page cursor overflow"))?;
    let end = offset.checked_add(count as u64)
        .ok_or_else(|| failure("read candidate evidence", "candidate page range overflow"))?;
    for chunk in descriptor.chunks.iter().skip(first) {
        let chunk_end = cursor + u64::from(chunk.bytes);
        if chunk_end > offset && cursor < end {
            let bytes = read_chunk(paths, chunk)?;
            let start = usize::try_from(offset.saturating_sub(cursor)).unwrap_or(usize::MAX).min(bytes.len());
            let finish = usize::try_from(end.saturating_sub(cursor)).unwrap_or(usize::MAX).min(bytes.len());
            output.extend_from_slice(&bytes[start..finish]);
        }
        cursor = chunk_end;
        if cursor >= end { break; }
    }
    if output.len() != count {
        return Err(failure("read candidate evidence", "candidate artifact page is incomplete"));
    }
    Ok(output)
}

fn read_chunk(paths: &StorePaths, chunk: &ChunkRef) -> Result<Vec<u8>, ProductRunnerError> {
    let bytes = read_regular(&paths.chunk(&chunk.sha256), "read candidate evidence chunk")?;
    if bytes.len() != chunk.bytes as usize || hex(&Sha256::digest(&bytes)) != chunk.sha256 {
        return Err(failure("read candidate evidence chunk", "candidate chunk digest mismatch"));
    }
    Ok(bytes)
}

fn read_regular(path: &Path, operation: &'static str) -> Result<Vec<u8>, ProductRunnerError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| failure(operation, error))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(failure(operation, "candidate evidence object is not a regular file"));
    }
    fs::read(path).map_err(|error| failure(operation, error))
}

fn valid_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_git_object(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.as_bytes().contains(&0)
        && Path::new(value).components().all(|component| {
            matches!(component, std::path::Component::Normal(name) if !name.eq_ignore_ascii_case(".git"))
        })
}

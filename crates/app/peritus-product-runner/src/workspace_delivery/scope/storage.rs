use super::{BaselineEntry, Entry, ScopedBaseline, failure, same_open_file, valid_preimage};
use crate::ProductRunnerError;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead as _, Read as _, Seek as _, SeekFrom, Write as _},
    path::{Path, PathBuf},
};

const STORE_VERSION: u16 = 1;
const SHARD_COUNT: usize = 256;
const LEGACY_RECORD_BYTES: u64 = 512 * 1024;
const CONTROL_BYTES: u64 = 512 * 1024;
const SEGMENT_BYTES: u64 = 1024 * 1024;
const PENDING_PREFIX: &str = ".pending-";

pub(super) struct Snapshot {
    pub(super) entries: BTreeMap<String, BaselineEntry>,
    pub(super) frontier: [u8; 32],
}

#[derive(Clone, Copy)]
pub(super) struct Artifact {
    pub(super) sha256: [u8; 32],
    pub(super) bytes: u64,
}

pub(super) struct ArtifactWriter {
    temporary: tempfile::NamedTempFile,
    directory: PathBuf,
    hasher: Sha256,
    bytes: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Adoption {
    version: u16,
    scope_sha256: [u8; 32],
    legacy_sha256: [u8; 32],
    legacy_bytes: u64,
    migrated_entries: u64,
    initial_catalog_sha256: [u8; 32],
    checksum: [u8; 32],
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    version: u16,
    scope_sha256: [u8; 32],
    generation: u64,
    entries: u64,
    shards: Vec<Option<ShardRef>>,
    checksum: [u8; 32],
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ShardRef {
    sha256: [u8; 32],
    bytes: u64,
    entries: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Shard {
    version: u16,
    scope_sha256: [u8; 32],
    bucket: u8,
    entries: BTreeMap<String, Checkpoint>,
    checksum: [u8; 32],
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    version: u16,
    scope_sha256: [u8; 32],
    path: String,
    segment_sha256: [u8; 32],
    segment_bytes: u64,
    checksum: [u8; 32],
}

struct Legacy {
    entries: Vec<Entry>,
    sha256: [u8; 32],
    bytes: u64,
}

struct Ownership {
    _file: fs::File,
}

pub(super) fn snapshot(scope: &ScopedBaseline) -> Result<Snapshot, ProductRunnerError> {
    let _owner = Ownership::acquire(scope)?;
    ensure_adopted(scope)?;
    let catalog = read_catalog(scope)?;
    let mut entries = BTreeMap::new();
    for (bucket, reference) in catalog.shards.iter().enumerate() {
        let Some(reference) = reference else { continue };
        let bucket = u8::try_from(bucket)
            .map_err(|_| failure("in-place evidence shard index overflow"))?;
        let shard = read_shard(scope, bucket, reference)?;
        for checkpoint in shard.entries.into_values() {
            let entry = load_checkpoint_entry(scope, &checkpoint)?;
            if entries
                .insert(
                    entry.path,
                    BaselineEntry {
                        before: entry.before,
                        preimage: entry.preimage,
                        complete: entry.version == 2,
                    },
                )
                .is_some()
            {
                return Err(failure("in-place evidence index contains a duplicate path"));
            }
        }
    }
    if u64::try_from(entries.len()).ok() != Some(catalog.entries) {
        return Err(failure("in-place evidence catalog count conflicts with its shards"));
    }
    Ok(Snapshot { entries, frontier: catalog.checksum })
}

pub(super) fn load_one(
    scope: &ScopedBaseline,
    path: &str,
) -> Result<Option<BaselineEntry>, ProductRunnerError> {
    let _owner = Ownership::acquire(scope)?;
    ensure_adopted(scope)?;
    let catalog = read_catalog(scope)?;
    let bucket = bucket(path);
    let Some(reference) = &catalog.shards[usize::from(bucket)] else { return Ok(None) };
    let shard = read_shard(scope, bucket, reference)?;
    let Some(checkpoint) = shard.entries.get(path) else { return Ok(None) };
    let entry = load_checkpoint_entry(scope, checkpoint)?;
    Ok(Some(BaselineEntry {
        before: entry.before,
        preimage: entry.preimage,
        complete: entry.version == 2,
    }))
}

pub(super) fn enroll(scope: &ScopedBaseline, entry: &Entry) -> Result<(), ProductRunnerError> {
    let _owner = Ownership::acquire(scope)?;
    ensure_adopted(scope)?;
    validate_entry(scope, entry)?;
    let mut catalog = read_catalog(scope)?;
    let bucket = bucket(&entry.path);
    let mut shard = match &catalog.shards[usize::from(bucket)] {
        Some(reference) => read_shard(scope, bucket, reference)?,
        None => empty_shard(scope, bucket)?,
    };
    if let Some(existing) = shard.entries.get(&entry.path) {
        load_checkpoint_entry(scope, existing)?;
        return Ok(());
    }
    let checkpoint = persist_segment(scope, entry)?;
    shard.entries.insert(entry.path.clone(), checkpoint);
    shard.checksum = shard_checksum(&shard);
    catalog.shards[usize::from(bucket)] = Some(persist_shard(scope, &shard)?);
    catalog.entries = catalog
        .entries
        .checked_add(1)
        .ok_or_else(|| failure("in-place evidence entry count overflow"))?;
    catalog.generation = catalog
        .generation
        .checked_add(1)
        .ok_or_else(|| failure("in-place evidence generation overflow"))?;
    catalog.checksum = catalog_checksum(&catalog);
    replace_catalog(scope, &catalog)
}

pub(super) fn path_count(scope: &ScopedBaseline) -> Result<usize, ProductRunnerError> {
    let _owner = Ownership::acquire(scope)?;
    ensure_adopted(scope)?;
    usize::try_from(read_catalog(scope)?.entries)
        .map_err(|_| failure("in-place evidence path count exceeds native addressing"))
}

pub(super) fn frontier_matches(
    scope: &ScopedBaseline,
    expected: [u8; 32],
) -> Result<bool, ProductRunnerError> {
    let _owner = Ownership::acquire(scope)?;
    ensure_adopted(scope)?;
    Ok(read_catalog(scope)?.checksum == expected)
}

pub(super) fn binding(scope: &ScopedBaseline) -> Result<[u8; 32], ProductRunnerError> {
    scope_binding(scope)
}

pub(super) fn artifact_writer(
    scope: &ScopedBaseline,
) -> Result<ArtifactWriter, ProductRunnerError> {
    let _owner = Ownership::acquire(scope)?;
    ensure_adopted(scope)?;
    let directory = paths(scope).artifacts();
    validate_directory(&directory, "in-place evidence artifact store")?;
    let temporary = tempfile::Builder::new()
        .prefix(PENDING_PREFIX)
        .tempfile_in(&directory)
        .map_err(|error| failure(error.to_string()))?;
    Ok(ArtifactWriter { temporary, directory, hasher: Sha256::new(), bytes: 0 })
}

pub(super) fn read_artifact_page(
    scope: &ScopedBaseline,
    digest: [u8; 32],
    expected_bytes: u64,
    offset: u64,
    maximum: usize,
) -> Result<Vec<u8>, ProductRunnerError> {
    let _owner = Ownership::acquire(scope)?;
    ensure_adopted(scope)?;
    if offset > expected_bytes {
        return Err(failure("in-place evidence page offset is outside the retained artifact"));
    }
    let maximum = u64::try_from(maximum)
        .map_err(|_| failure("in-place evidence page size exceeds its representation"))?;
    let count = usize::try_from((expected_bytes - offset).min(maximum))
        .map_err(|_| failure("in-place evidence page length exceeds native addressing"))?;
    let path = paths(scope).artifact(&digest);
    let metadata = regular_metadata(&path, "in-place evidence artifact")?;
    if metadata.len() != expected_bytes {
        return Err(failure("in-place evidence artifact length conflicts with its handle"));
    }
    let mut file = open_same(&path, &metadata, "in-place evidence artifact")?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut page = Vec::with_capacity(count);
    let end = offset
        .checked_add(count as u64)
        .ok_or_else(|| failure("in-place evidence page range overflow"))?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| failure(error.to_string()))?;
        if read == 0 { break; }
        let next = total
            .checked_add(read as u64)
            .ok_or_else(|| failure("in-place evidence artifact length overflow"))?;
        hasher.update(&buffer[..read]);
        if next > offset && total < end {
            let from = usize::try_from(offset.saturating_sub(total))
                .unwrap_or(usize::MAX)
                .min(read);
            let through = usize::try_from(end.saturating_sub(total))
                .unwrap_or(usize::MAX)
                .min(read);
            page.extend_from_slice(&buffer[from..through]);
        }
        total = next;
    }
    let actual: [u8; 32] = hasher.finalize().into();
    if total != expected_bytes || actual != digest || page.len() != count {
        return Err(failure("in-place evidence artifact failed exact digest verification"));
    }
    Ok(page)
}

impl ArtifactWriter {
    pub(super) fn finish(self) -> Result<Artifact, ProductRunnerError> {
        self.finish_inner(None)
    }

    pub(super) fn finish_expected(
        self,
        sha256: [u8; 32],
        bytes: u64,
    ) -> Result<Artifact, ProductRunnerError> {
        self.finish_inner(Some((sha256, bytes)))
    }

    fn finish_inner(
        mut self,
        expected: Option<([u8; 32], u64)>,
    ) -> Result<Artifact, ProductRunnerError> {
        self.temporary.as_file_mut().flush().map_err(|error| failure(error.to_string()))?;
        self.temporary.as_file().sync_all().map_err(|error| failure(error.to_string()))?;
        let sha256: [u8; 32] = self.hasher.finalize().into();
        let artifact = Artifact { sha256, bytes: self.bytes };
        if expected.is_some_and(|expected| expected != (artifact.sha256, artifact.bytes)) {
            return Err(failure(
                "in-place evidence source changed while its exact artifact was retained",
            ));
        }
        let destination = self.directory.join(hex(&artifact.sha256));
        persist_temporary(self.temporary, &destination, artifact.sha256, artifact.bytes)?;
        Ok(artifact)
    }
}

impl Write for ArtifactWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.temporary.write_all(bytes)?;
        self.hasher.update(bytes);
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("in-place evidence artifact length overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.temporary.flush()
    }
}

impl Ownership {
    fn acquire(scope: &ScopedBaseline) -> Result<Self, ProductRunnerError> {
        let path = paths(scope).lock();
        let parent = path.parent().ok_or_else(|| failure("scope lock has no parent"))?;
        fs::create_dir_all(parent).map_err(|error| failure(error.to_string()))?;
        let file = match fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path) {
            Ok(file) => {
                file.sync_all().map_err(|error| failure(error.to_string()))?;
                sync_directory(parent)?;
                file
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let before = regular_metadata(&path, "in-place evidence ownership lock")?;
                let file = fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .map_err(|error| failure(error.to_string()))?;
                let opened = file.metadata().map_err(|error| failure(error.to_string()))?;
                let current = regular_metadata(&path, "in-place evidence ownership lock")?;
                if !same_open_file(&before, &opened) || !same_open_file(&current, &opened) {
                    return Err(failure(
                        "in-place evidence ownership changed while it was opened",
                    ));
                }
                file
            }
            Err(error) => return Err(failure(error.to_string())),
        };
        file.lock().map_err(|error| failure(error.to_string()))?;
        let after = regular_metadata(&path, "in-place evidence ownership lock")?;
        let opened = file.metadata().map_err(|error| failure(error.to_string()))?;
        if !same_open_file(&after, &opened) {
            return Err(failure("in-place evidence ownership changed after it was locked"));
        }
        Ok(Self { _file: file })
    }
}

struct Paths {
    journal: PathBuf,
    root: PathBuf,
}

impl Paths {
    fn marker(&self) -> PathBuf { self.root.join("adoption.json") }
    fn catalog(&self) -> PathBuf { self.root.join("catalog.json") }
    fn segments(&self) -> PathBuf { self.root.join("segments") }
    fn indexes(&self) -> PathBuf { self.root.join("indexes") }
    fn artifacts(&self) -> PathBuf { self.root.join("artifacts") }
    fn lock(&self) -> PathBuf { self.journal.with_extension("scope-v3.lock") }
    fn segment(&self, digest: &[u8; 32]) -> PathBuf { self.segments().join(hex(digest)) }
    fn index(&self, digest: &[u8; 32]) -> PathBuf { self.indexes().join(hex(digest)) }
    fn artifact(&self, digest: &[u8; 32]) -> PathBuf { self.artifacts().join(hex(digest)) }
}

fn paths(scope: &ScopedBaseline) -> Paths {
    Paths {
        journal: scope.journal.clone(),
        root: scope.journal.with_extension("scope-v3"),
    }
}

fn ensure_adopted(scope: &ScopedBaseline) -> Result<(), ProductRunnerError> {
    let store = paths(scope);
    match fs::symlink_metadata(store.marker()) {
        Ok(_) => {
            validate_store(&store)?;
            validate_adoption(scope, &store)?;
            read_catalog(scope).map(|_| ())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let legacy = load_legacy(scope)?;
            prepare_store(&store)?;
            let scope_sha256 = scope_binding(scope)?;
            let mut buckets = (0..SHARD_COUNT)
                .map(|_| BTreeMap::<String, Checkpoint>::new())
                .collect::<Vec<_>>();
            for entry in &legacy.entries {
                validate_entry(scope, entry)?;
                let checkpoint = persist_segment(scope, entry)?;
                if buckets[usize::from(bucket(&entry.path))]
                    .insert(entry.path.clone(), checkpoint)
                    .is_some()
                {
                    return Err(failure("legacy in-place evidence contains a duplicate path"));
                }
            }
            let mut catalog = empty_catalog(scope_sha256);
            for (bucket, entries) in buckets.into_iter().enumerate() {
                if entries.is_empty() { continue; }
                let bucket = u8::try_from(bucket)
                    .map_err(|_| failure("in-place evidence shard index overflow"))?;
                let mut shard = Shard {
                    version: STORE_VERSION,
                    scope_sha256,
                    bucket,
                    entries,
                    checksum: [0; 32],
                };
                shard.checksum = shard_checksum(&shard);
                catalog.shards[usize::from(bucket)] = Some(persist_shard(scope, &shard)?);
            }
            catalog.entries = u64::try_from(legacy.entries.len())
                .map_err(|_| failure("legacy in-place evidence entry count overflow"))?;
            catalog.generation = catalog.entries;
            catalog.checksum = catalog_checksum(&catalog);
            replace_catalog(scope, &catalog)?;
            let catalog_bytes = encode_catalog(&catalog)?;
            let initial_catalog_sha256: [u8; 32] = Sha256::digest(&catalog_bytes).into();
            let migrated_entries = catalog.entries;
            let checksum = adoption_checksum(
                scope_sha256,
                legacy.sha256,
                legacy.bytes,
                migrated_entries,
                initial_catalog_sha256,
            );
            let adoption = Adoption {
                version: STORE_VERSION,
                scope_sha256,
                legacy_sha256: legacy.sha256,
                legacy_bytes: legacy.bytes,
                migrated_entries,
                initial_catalog_sha256,
                checksum,
            };
            let encoded = serde_json::to_vec(&adoption)
                .map_err(|error| failure(error.to_string()))?;
            persist_bytes(&store.marker(), &encoded)?;
            sync_directory(&store.root)
        }
        Err(error) => Err(failure(error.to_string())),
    }
}

fn prepare_store(paths: &Paths) -> Result<(), ProductRunnerError> {
    for directory in [
        paths.root.clone(),
        paths.segments(),
        paths.indexes(),
        paths.artifacts(),
    ] {
        fs::create_dir_all(&directory).map_err(|error| failure(error.to_string()))?;
        validate_directory(&directory, "in-place evidence store")?;
        sync_directory(&directory)?;
    }
    if let Some(parent) = paths.root.parent() { sync_directory(parent)?; }
    Ok(())
}

fn validate_store(paths: &Paths) -> Result<(), ProductRunnerError> {
    for directory in [
        paths.root.clone(),
        paths.segments(),
        paths.indexes(),
        paths.artifacts(),
    ] {
        validate_directory(&directory, "in-place evidence store")?;
    }
    Ok(())
}

fn validate_adoption(scope: &ScopedBaseline, paths: &Paths) -> Result<(), ProductRunnerError> {
    let bytes = read_regular_limited(
        &paths.marker(),
        CONTROL_BYTES,
        "in-place evidence adoption checkpoint",
    )?;
    let adoption: Adoption =
        serde_json::from_slice(&bytes).map_err(|error| failure(error.to_string()))?;
    let checksum = adoption_checksum(
        adoption.scope_sha256,
        adoption.legacy_sha256,
        adoption.legacy_bytes,
        adoption.migrated_entries,
        adoption.initial_catalog_sha256,
    );
    if adoption.version != STORE_VERSION
        || adoption.scope_sha256 != scope_binding(scope)?
        || adoption.checksum != checksum
    {
        return Err(failure("in-place evidence adoption checkpoint is corrupt or stale"));
    }
    Ok(())
}

fn empty_catalog(scope_sha256: [u8; 32]) -> Catalog {
    let mut catalog = Catalog {
        version: STORE_VERSION,
        scope_sha256,
        generation: 0,
        entries: 0,
        shards: vec![None; SHARD_COUNT],
        checksum: [0; 32],
    };
    catalog.checksum = catalog_checksum(&catalog);
    catalog
}

fn empty_shard(scope: &ScopedBaseline, bucket: u8) -> Result<Shard, ProductRunnerError> {
    let mut shard = Shard {
        version: STORE_VERSION,
        scope_sha256: scope_binding(scope)?,
        bucket,
        entries: BTreeMap::new(),
        checksum: [0; 32],
    };
    shard.checksum = shard_checksum(&shard);
    Ok(shard)
}

fn read_catalog(scope: &ScopedBaseline) -> Result<Catalog, ProductRunnerError> {
    let bytes = read_regular_limited(
        &paths(scope).catalog(),
        CONTROL_BYTES,
        "in-place evidence catalog",
    )?;
    let catalog: Catalog =
        serde_json::from_slice(&bytes).map_err(|error| failure(error.to_string()))?;
    if catalog.version != STORE_VERSION
        || catalog.scope_sha256 != scope_binding(scope)?
        || catalog.shards.len() != SHARD_COUNT
        || catalog.generation != catalog.entries
        || catalog.checksum != catalog_checksum(&catalog)
    {
        return Err(failure("in-place evidence catalog is corrupt or stale"));
    }
    let entries = catalog.shards.iter().flatten().try_fold(0_u64, |total, shard| {
        total
            .checked_add(shard.entries)
            .ok_or_else(|| failure("in-place evidence catalog count overflow"))
    })?;
    if entries != catalog.entries {
        return Err(failure("in-place evidence catalog shard counts are inconsistent"));
    }
    Ok(catalog)
}

fn replace_catalog(scope: &ScopedBaseline, catalog: &Catalog) -> Result<(), ProductRunnerError> {
    let bytes = encode_catalog(catalog)?;
    let path = paths(scope).catalog();
    let parent = path.parent().ok_or_else(|| failure("catalog has no parent"))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(PENDING_PREFIX)
        .tempfile_in(parent)
        .map_err(|error| failure(error.to_string()))?;
    temporary.write_all(&bytes).map_err(|error| failure(error.to_string()))?;
    temporary.as_file().sync_all().map_err(|error| failure(error.to_string()))?;
    temporary.persist(&path).map_err(|error| failure(error.error.to_string()))?;
    sync_directory(parent)
}

fn encode_catalog(catalog: &Catalog) -> Result<Vec<u8>, ProductRunnerError> {
    serde_json::to_vec(catalog).map_err(|error| failure(error.to_string()))
}

fn read_shard(
    scope: &ScopedBaseline,
    expected_bucket: u8,
    reference: &ShardRef,
) -> Result<Shard, ProductRunnerError> {
    if reference.bytes == 0 || reference.entries == 0 {
        return Err(failure("in-place evidence shard reference has no bytes"));
    }
    let path = paths(scope).index(&reference.sha256);
    let bytes = read_regular_exact(&path, reference.bytes, "in-place evidence index shard")?;
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    if digest != reference.sha256 {
        return Err(failure("committed in-place evidence index shard is corrupt"));
    }
    let shard: Shard =
        serde_json::from_slice(&bytes).map_err(|error| failure(error.to_string()))?;
    if shard.version != STORE_VERSION
        || shard.scope_sha256 != scope_binding(scope)?
        || shard.bucket != expected_bucket
        || shard.checksum != shard_checksum(&shard)
        || u64::try_from(shard.entries.len()).ok() != Some(reference.entries)
        || shard.entries.values().any(|checkpoint| {
            checkpoint.path.is_empty() || bucket(&checkpoint.path) != expected_bucket
        })
    {
        return Err(failure("in-place evidence index shard is corrupt or stale"));
    }
    for (path, checkpoint) in &shard.entries {
        if path != &checkpoint.path {
            return Err(failure("in-place evidence index key conflicts with its checkpoint"));
        }
        validate_checkpoint(scope, checkpoint)?;
    }
    Ok(shard)
}

fn persist_shard(
    scope: &ScopedBaseline,
    shard: &Shard,
) -> Result<ShardRef, ProductRunnerError> {
    let encoded = serde_json::to_vec(shard).map_err(|error| failure(error.to_string()))?;
    let sha256: [u8; 32] = Sha256::digest(&encoded).into();
    let bytes = u64::try_from(encoded.len())
        .map_err(|_| failure("in-place evidence index shard length overflow"))?;
    persist_bytes(&paths(scope).index(&sha256), &encoded)?;
    Ok(ShardRef {
        sha256,
        bytes,
        entries: u64::try_from(shard.entries.len())
            .map_err(|_| failure("in-place evidence shard count overflow"))?,
    })
}

fn persist_segment(
    scope: &ScopedBaseline,
    entry: &Entry,
) -> Result<Checkpoint, ProductRunnerError> {
    validate_entry(scope, entry)?;
    let encoded = serde_json::to_vec(entry).map_err(|error| failure(error.to_string()))?;
    let segment_bytes = u64::try_from(encoded.len())
        .map_err(|_| failure("in-place evidence segment length overflow"))?;
    if segment_bytes == 0 || segment_bytes > SEGMENT_BYTES {
        return Err(failure("in-place evidence segment exceeds its physical record bound"));
    }
    let segment_sha256: [u8; 32] = Sha256::digest(&encoded).into();
    persist_bytes(&paths(scope).segment(&segment_sha256), &encoded)?;
    let scope_sha256 = scope_binding(scope)?;
    Ok(Checkpoint {
        version: STORE_VERSION,
        scope_sha256,
        path: entry.path.clone(),
        segment_sha256,
        segment_bytes,
        checksum: checkpoint_checksum(
            scope_sha256,
            &entry.path,
            segment_sha256,
            segment_bytes,
        ),
    })
}

fn load_checkpoint_entry(
    scope: &ScopedBaseline,
    checkpoint: &Checkpoint,
) -> Result<Entry, ProductRunnerError> {
    validate_checkpoint(scope, checkpoint)?;
    let path = paths(scope).segment(&checkpoint.segment_sha256);
    let bytes = read_regular_exact(
        &path,
        checkpoint.segment_bytes,
        "in-place evidence segment",
    )?;
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    if digest != checkpoint.segment_sha256 {
        return Err(failure("committed in-place evidence segment is corrupt"));
    }
    let entry: Entry = serde_json::from_slice(&bytes).map_err(|error| failure(error.to_string()))?;
    validate_entry(scope, &entry)?;
    if entry.path != checkpoint.path {
        return Err(failure("in-place evidence segment conflicts with its checkpoint"));
    }
    Ok(entry)
}

fn validate_checkpoint(
    scope: &ScopedBaseline,
    checkpoint: &Checkpoint,
) -> Result<(), ProductRunnerError> {
    if checkpoint.version != STORE_VERSION
        || checkpoint.scope_sha256 != scope_binding(scope)?
        || checkpoint.segment_bytes == 0
        || checkpoint.segment_bytes > SEGMENT_BYTES
        || checkpoint.checksum
            != checkpoint_checksum(
                checkpoint.scope_sha256,
                &checkpoint.path,
                checkpoint.segment_sha256,
                checkpoint.segment_bytes,
            )
    {
        return Err(failure("in-place evidence path checkpoint is corrupt or stale"));
    }
    scope.checked_path(&checkpoint.path)?;
    Ok(())
}

fn validate_entry(scope: &ScopedBaseline, entry: &Entry) -> Result<(), ProductRunnerError> {
    scope.checked_path(&entry.path)?;
    if !matches!(entry.version, 1 | 2)
        || entry.scope != *scope
        || !valid_preimage(entry)
    {
        return Err(failure(
            "in-place evidence has an invalid scope, version or preimage identity",
        ));
    }
    Ok(())
}

fn load_legacy(scope: &ScopedBaseline) -> Result<Legacy, ProductRunnerError> {
    let metadata = match fs::symlink_metadata(&scope.journal) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
        Ok(_) => return Err(failure("in-place evidence is not a regular owned journal")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Legacy {
                entries: Vec::new(),
                sha256: Sha256::digest(b"").into(),
                bytes: 0,
            });
        }
        Err(error) => return Err(failure(error.to_string())),
    };
    let file = open_same(&scope.journal, &metadata, "legacy in-place evidence journal")?;
    let mut reader = std::io::BufReader::new(file);
    let mut entries = Vec::new();
    let mut seen = BTreeMap::<String, ()>::new();
    let mut digest = Sha256::new();
    let mut committed = 0_u64;
    let mut torn = false;
    loop {
        let start = committed;
        let mut bytes = Vec::new();
        let count = reader
            .by_ref()
            .take(LEGACY_RECORD_BYTES + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|error| failure(error.to_string()))?;
        if count == 0 { break; }
        let newline = bytes.last() == Some(&b'\n');
        if count as u64 > LEGACY_RECORD_BYTES {
            if newline || remaining_record_has_newline(&mut reader)? {
                return Err(failure(
                    "committed legacy in-place evidence contains an oversized record",
                ));
            }
            torn = true;
            committed = start;
            break;
        }
        if !newline {
            torn = true;
            committed = start;
            break;
        }
        let entry: Entry = serde_json::from_slice(&bytes)
            .map_err(|error| failure(format!("committed legacy in-place evidence is corrupt: {error}")))?;
        validate_entry(scope, &entry)?;
        if seen.insert(entry.path.clone(), ()).is_some() {
            return Err(failure("committed legacy in-place evidence contains a duplicate path"));
        }
        digest.update(&bytes);
        committed = committed
            .checked_add(count as u64)
            .ok_or_else(|| failure("legacy in-place evidence length overflow"))?;
        entries.push(entry);
    }
    drop(reader);
    if torn {
        reconcile_legacy_tail(scope, &metadata, committed)?;
    } else if committed != metadata.len() {
        return Err(failure("legacy in-place evidence frontier is inconsistent"));
    }
    Ok(Legacy { entries, sha256: digest.finalize().into(), bytes: committed })
}

fn remaining_record_has_newline(
    reader: &mut std::io::BufReader<fs::File>,
) -> Result<bool, ProductRunnerError> {
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer).map_err(|error| failure(error.to_string()))?;
        if count == 0 { return Ok(false); }
        if buffer[..count].contains(&b'\n') { return Ok(true); }
    }
}

fn reconcile_legacy_tail(
    scope: &ScopedBaseline,
    expected: &fs::Metadata,
    committed: u64,
) -> Result<(), ProductRunnerError> {
    let current = regular_metadata(&scope.journal, "legacy in-place evidence journal")?;
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&scope.journal)
        .map_err(|error| failure(error.to_string()))?;
    let opened = file.metadata().map_err(|error| failure(error.to_string()))?;
    if !same_open_file(expected, &opened)
        || !same_open_file(&current, &opened)
        || current.len() != expected.len()
    {
        return Err(failure(
            "legacy in-place evidence changed before its uncommitted tail could be reconciled",
        ));
    }
    file.seek(SeekFrom::Start(committed))
        .and_then(|_| file.set_len(committed))
        .and_then(|()| file.sync_all())
        .map_err(|error| failure(error.to_string()))?;
    if let Some(parent) = scope.journal.parent() { sync_directory(parent)?; }
    Ok(())
}

fn catalog_checksum(catalog: &Catalog) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/in-place-catalog/v1\0");
    hasher.update(catalog.scope_sha256);
    hasher.update(catalog.generation.to_be_bytes());
    hasher.update(catalog.entries.to_be_bytes());
    for reference in &catalog.shards {
        match reference {
            Some(reference) => {
                hasher.update([1]);
                hasher.update(reference.sha256);
                hasher.update(reference.bytes.to_be_bytes());
                hasher.update(reference.entries.to_be_bytes());
            }
            None => hasher.update([0]),
        }
    }
    hasher.finalize().into()
}

fn shard_checksum(shard: &Shard) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/in-place-index-shard/v1\0");
    hasher.update(shard.scope_sha256);
    hasher.update([shard.bucket]);
    for (path, checkpoint) in &shard.entries {
        hash_bytes(&mut hasher, path.as_bytes());
        hasher.update(checkpoint.checksum);
    }
    hasher.finalize().into()
}

fn checkpoint_checksum(
    scope: [u8; 32],
    path: &str,
    segment: [u8; 32],
    bytes: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/in-place-checkpoint/v1\0");
    hasher.update(scope);
    hash_bytes(&mut hasher, path.as_bytes());
    hasher.update(segment);
    hasher.update(bytes.to_be_bytes());
    hasher.finalize().into()
}

fn adoption_checksum(
    scope: [u8; 32],
    legacy: [u8; 32],
    bytes: u64,
    entries: u64,
    catalog: [u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/in-place-adoption/v1\0");
    hasher.update(scope);
    hasher.update(legacy);
    hasher.update(bytes.to_be_bytes());
    hasher.update(entries.to_be_bytes());
    hasher.update(catalog);
    hasher.finalize().into()
}

fn scope_binding(scope: &ScopedBaseline) -> Result<[u8; 32], ProductRunnerError> {
    let encoded = serde_json::to_vec(scope).map_err(|error| failure(error.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/in-place-scope/v3\0");
    hash_bytes(&mut hasher, &encoded);
    Ok(hasher.finalize().into())
}

fn bucket(path: &str) -> u8 {
    path_digest(path)[0]
}

fn path_digest(path: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/in-place-path/v1\0");
    hash_bytes(&mut hasher, path.as_bytes());
    hasher.finalize().into()
}

fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(bytes);
}

fn persist_bytes(path: &Path, bytes: &[u8]) -> Result<(), ProductRunnerError> {
    let parent = path.parent().ok_or_else(|| failure("evidence object has no parent"))?;
    validate_directory(parent, "in-place evidence object parent")?;
    let mut temporary = tempfile::Builder::new()
        .prefix(PENDING_PREFIX)
        .tempfile_in(parent)
        .map_err(|error| failure(error.to_string()))?;
    temporary.write_all(bytes).map_err(|error| failure(error.to_string()))?;
    temporary.as_file().sync_all().map_err(|error| failure(error.to_string()))?;
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    let length = u64::try_from(bytes.len())
        .map_err(|_| failure("evidence object length overflow"))?;
    persist_temporary(temporary, path, digest, length)
}

fn persist_temporary(
    temporary: tempfile::NamedTempFile,
    path: &Path,
    digest: [u8; 32],
    bytes: u64,
) -> Result<(), ProductRunnerError> {
    let parent = path.parent().ok_or_else(|| failure("evidence object has no parent"))?;
    match temporary.persist_noclobber(path) {
        Ok(_) => sync_directory(parent),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = regular_metadata(path, "in-place evidence object")?;
            if metadata.len() != bytes || digest_regular(path, &metadata)? != digest {
                return Err(failure("committed in-place evidence object conflicts with its identity"));
            }
            Ok(())
        }
        Err(error) => Err(failure(error.error.to_string())),
    }
}

fn read_regular_exact(
    path: &Path,
    bytes: u64,
    kind: &str,
) -> Result<Vec<u8>, ProductRunnerError> {
    let metadata = regular_metadata(path, kind)?;
    if metadata.len() != bytes {
        return Err(failure(format!("{kind} length conflicts with its checkpoint")));
    }
    read_regular(path, &metadata, kind)
}

fn read_regular_limited(
    path: &Path,
    maximum: u64,
    kind: &str,
) -> Result<Vec<u8>, ProductRunnerError> {
    let metadata = regular_metadata(path, kind)?;
    if metadata.len() > maximum {
        return Err(failure(format!("{kind} exceeds its physical control bound")));
    }
    read_regular(path, &metadata, kind)
}

fn read_regular(
    path: &Path,
    metadata: &fs::Metadata,
    kind: &str,
) -> Result<Vec<u8>, ProductRunnerError> {
    let mut file = open_same(path, metadata, kind)?;
    let capacity = usize::try_from(metadata.len())
        .map_err(|_| failure(format!("{kind} length exceeds native addressing")))?;
    let mut bytes = Vec::with_capacity(capacity);
    file.read_to_end(&mut bytes).map_err(|error| failure(error.to_string()))?;
    if u64::try_from(bytes.len()).ok() != Some(metadata.len()) {
        return Err(failure(format!("{kind} changed while it was read")));
    }
    Ok(bytes)
}

fn digest_regular(path: &Path, expected: &fs::Metadata) -> Result<[u8; 32], ProductRunnerError> {
    let mut file = open_same(path, expected, "in-place evidence object")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| failure(error.to_string()))?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().into())
}

fn open_same(
    path: &Path,
    expected: &fs::Metadata,
    kind: &str,
) -> Result<fs::File, ProductRunnerError> {
    let file = fs::File::open(path).map_err(|error| failure(error.to_string()))?;
    let opened = file.metadata().map_err(|error| failure(error.to_string()))?;
    let current = regular_metadata(path, kind)?;
    if !same_open_file(expected, &opened) || !same_open_file(&current, &opened) {
        return Err(failure(format!("{kind} changed while it was opened")));
    }
    Ok(file)
}

fn regular_metadata(path: &Path, kind: &str) -> Result<fs::Metadata, ProductRunnerError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| failure(error.to_string()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(failure(format!("{kind} is not a regular owned file")));
    }
    Ok(metadata)
}

fn validate_directory(path: &Path, kind: &str) -> Result<(), ProductRunnerError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| failure(error.to_string()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(failure(format!("{kind} is not an owned directory")));
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), ProductRunnerError> {
    #[cfg(unix)]
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| failure(error.to_string()))?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn hex(bytes: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(64), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

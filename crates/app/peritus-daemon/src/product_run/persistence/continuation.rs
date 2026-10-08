//! Immutable, bounded-page storage for product-run continuations.

use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use peritus_product_runner::ProductRunResume;
use peritus_run_settlement::{CandidateCheckpoint, CandidateIdentity};
use peritus_types::Sha256Digest;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::super::ProductRunServiceError;

const ROOT_VERSION: u16 = 1;
const PAGE_BYTES: usize = 32 * 1024;
const PAGE_DOMAIN: &[u8] = b"peritus-product-continuation-page-v1\0";
const ROOT_DOMAIN: &[u8] = b"peritus-product-continuation-root-v1\0";

/// Bounded authority for an immutable continuation page chain.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PersistedResumeRoot {
    version: u16,
    lineage: ResumeLineage,
    page_bytes: u32,
    page_count: u64,
    total_bytes: u64,
    final_page_digest: [u8; 32],
    root_digest: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ResumeLineage {
    run_id: [u8; 16],
    workspace_id: [u8; 16],
    content_digest: [u8; 32],
    repository_digest: [u8; 32],
    execution_digest: Option<[u8; 32]>,
    requirements_revision: u64,
    checkpoint_sequence: u64,
}

impl PersistedResumeRoot {
    pub(super) fn binds_checkpoint(&self, checkpoint: &CandidateCheckpoint) -> bool {
        self.lineage == ResumeLineage::from_identity(*checkpoint.identity())
    }

    pub(super) fn matches_checkpoint(&self, checkpoint: &CandidateCheckpoint) -> bool {
        self.version == ROOT_VERSION
            && self.page_bytes == PAGE_BYTES as u32
            && self.binds_checkpoint(checkpoint)
            && self.root_digest == self.computed_root_digest()
            && self.page_count > 0
            && self.total_bytes > 0
            && self.page_count
                == self.total_bytes.div_ceil(u64::try_from(PAGE_BYTES).expect("page size fits u64"))
    }

    fn computed_root_digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(ROOT_DOMAIN);
        self.lineage.hash(&mut hasher);
        hasher.update(self.page_bytes.to_be_bytes());
        hasher.update(self.page_count.to_be_bytes());
        hasher.update(self.total_bytes.to_be_bytes());
        hasher.update(self.final_page_digest);
        hasher.finalize().into()
    }

    fn directory_name(&self) -> String {
        hex(&self.root_digest)
    }
}

impl ResumeLineage {
    fn from_identity(identity: CandidateIdentity) -> Self {
        Self {
            run_id: *identity.run_id().as_bytes(),
            workspace_id: *identity.workspace_id().as_bytes(),
            content_digest: identity.content_digest().into_bytes(),
            repository_digest: identity.repository_digest().into_bytes(),
            execution_digest: identity.execution_digest().map(Sha256Digest::into_bytes),
            requirements_revision: identity.requirements_revision(),
            checkpoint_sequence: identity.checkpoint_sequence(),
        }
    }

    fn hash(&self, hasher: &mut Sha256) {
        hasher.update(self.run_id);
        hasher.update(self.workspace_id);
        hasher.update(self.content_digest);
        hasher.update(self.repository_digest);
        match self.execution_digest {
            Some(digest) => {
                hasher.update([1]);
                hasher.update(digest);
            }
            None => hasher.update([0]),
        }
        hasher.update(self.requirements_revision.to_be_bytes());
        hasher.update(self.checkpoint_sequence.to_be_bytes());
    }
}

pub(super) fn publish(
    workbench_root: &Path,
    resume: &ProductRunResume,
) -> Result<PersistedResumeRoot, ProductRunServiceError> {
    let root = workbench_root.join("continuations");
    fs::create_dir_all(&root).map_err(|error| persistence("create continuation store", error))?;
    sync_directory(workbench_root, "sync workbench continuation parent")?;
    let staging = tempfile::Builder::new()
        .prefix(".continuation-")
        .tempdir_in(&root)
        .map_err(|error| persistence("create continuation staging directory", error))?;
    let lineage = ResumeLineage::from_identity(*resume.checkpoint().identity());
    let mut writer = PageWriter::new(staging.path(), lineage.clone());
    resume
        .encode_durable_into(&mut writer)
        .map_err(|error| persistence("encode retained continuation", error))?;
    let descriptor = writer.finish()?;
    sync_directory(staging.path(), "sync continuation staging directory")?;
    let target = root.join(descriptor.directory_name());
    if target.exists() {
        verify_and_sync(&root, &target, &descriptor)?;
        return Ok(descriptor);
    }
    let staging_path = staging.keep();
    match fs::rename(&staging_path, &target) {
        Ok(()) => {}
        Err(error) if target.exists() => {
            let _ = fs::remove_dir_all(&staging_path);
            verify_and_sync(&root, &target, &descriptor)?;
            return Ok(descriptor);
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging_path);
            return Err(persistence("publish continuation page chain", error));
        }
    }
    sync_directory(&root, "sync continuation store")?;
    sync_directory(workbench_root, "sync workbench continuation parent")?;
    Ok(descriptor)
}

pub(super) fn restore(
    workbench_root: &Path,
    root: &PersistedResumeRoot,
    governed: &str,
) -> Result<ProductRunResume, ProductRunServiceError> {
    validate_root(root)?;
    let directory = workbench_root.join("continuations").join(root.directory_name());
    validate_directory(&directory)?;
    let version = ProductRunResume::durable_version_from(PageReader::new(
        &directory,
        root.clone(),
    ))
    .map_err(|error| invalid("inspect retained continuation version", error))?;
    sync_chain(&workbench_root.join("continuations"), &directory, root)?;
    if !ProductRunResume::durable_version_supported(version) {
        return Err(invalid(
            "inspect retained continuation version",
            format!("durable continuation version {version} requires a compatible decoder"),
        ));
    }
    let reader = PageReader::new(&directory, root.clone());
    ProductRunResume::decode_durable_retained_from(reader, governed)
        .map_err(|error| invalid("decode retained continuation", error))
}

fn verify_and_sync(
    store: &Path,
    directory: &Path,
    root: &PersistedResumeRoot,
) -> Result<(), ProductRunServiceError> {
    validate_root(root)?;
    validate_directory(directory)?;
    let mut reader = PageReader::new(directory, root.clone());
    let mut buffer = [0_u8; PAGE_BYTES];
    loop {
        match reader
            .read(&mut buffer)
            .map_err(|error| persistence("verify existing continuation page chain", error))?
        {
            0 => break,
            _ => {}
        }
    }
    sync_chain(store, directory, root)
}

fn sync_chain(
    store: &Path,
    directory: &Path,
    root: &PersistedResumeRoot,
) -> Result<(), ProductRunServiceError> {
    for ordinal in 0..root.page_count {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(page_path(directory, ordinal))
            .and_then(|file| file.sync_all())
            .map_err(|error| persistence("sync existing continuation page", error))?;
    }
    sync_directory(directory, "sync existing continuation directory")?;
    sync_directory(store, "sync continuation store")?;
    let parent = store.parent().ok_or_else(|| {
        invalid(
            "sync continuation store",
            "continuation store has no workbench parent",
        )
    })?;
    sync_directory(parent, "sync workbench continuation parent")
}

fn validate_root(root: &PersistedResumeRoot) -> Result<(), ProductRunServiceError> {
    if root.version != ROOT_VERSION
        || root.page_bytes != PAGE_BYTES as u32
        || root.page_count == 0
        || root.total_bytes == 0
        || root.page_count
            != root.total_bytes.div_ceil(u64::try_from(PAGE_BYTES).expect("page size fits u64"))
        || root.root_digest != root.computed_root_digest()
    {
        return Err(invalid(
            "validate retained continuation root",
            "continuation root is malformed or has an unsupported page format",
        ));
    }
    Ok(())
}

fn validate_directory(directory: &Path) -> Result<(), ProductRunServiceError> {
    let metadata = fs::symlink_metadata(directory)
        .map_err(|error| persistence("inspect continuation directory", error))?;
    if !metadata.file_type().is_dir() {
        return Err(invalid(
            "inspect continuation directory",
            "continuation root is not a directory",
        ));
    }
    Ok(())
}

struct PageWriter<'a> {
    directory: &'a Path,
    lineage: ResumeLineage,
    buffer: Vec<u8>,
    page_count: u64,
    total_bytes: u64,
    prior_digest: [u8; 32],
}

impl<'a> PageWriter<'a> {
    fn new(directory: &'a Path, lineage: ResumeLineage) -> Self {
        Self {
            directory,
            lineage,
            buffer: Vec::with_capacity(PAGE_BYTES),
            page_count: 0,
            total_bytes: 0,
            prior_digest: [0; 32],
        }
    }

    fn write_page(&mut self) -> Result<(), std::io::Error> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let ordinal = self.page_count;
        let digest = page_digest(&self.lineage, ordinal, self.prior_digest, &self.buffer);
        let path = page_path(self.directory, ordinal);
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(&self.buffer)?;
        file.sync_all()?;
        self.total_bytes = self
            .total_bytes
            .checked_add(u64::try_from(self.buffer.len()).map_err(|_| {
                std::io::Error::other("continuation page length is not representable")
            })?)
            .ok_or_else(|| std::io::Error::other("continuation length overflow"))?;
        self.page_count = self
            .page_count
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("continuation page ordinal overflow"))?;
        self.prior_digest = digest;
        self.buffer.clear();
        Ok(())
    }

    fn finish(mut self) -> Result<PersistedResumeRoot, ProductRunServiceError> {
        self.write_page().map_err(|error| persistence("write continuation page", error))?;
        if self.page_count == 0 {
            return Err(invalid(
                "encode retained continuation",
                "continuation encoder produced an empty payload",
            ));
        }
        let mut root = PersistedResumeRoot {
            version: ROOT_VERSION,
            lineage: self.lineage,
            page_bytes: PAGE_BYTES as u32,
            page_count: self.page_count,
            total_bytes: self.total_bytes,
            final_page_digest: self.prior_digest,
            root_digest: [0; 32],
        };
        root.root_digest = root.computed_root_digest();
        Ok(root)
    }
}

impl std::io::Write for PageWriter<'_> {
    fn write(&mut self, mut bytes: &[u8]) -> Result<usize, std::io::Error> {
        let supplied = bytes.len();
        while !bytes.is_empty() {
            let available = PAGE_BYTES - self.buffer.len();
            let consumed = available.min(bytes.len());
            self.buffer.extend_from_slice(&bytes[..consumed]);
            bytes = &bytes[consumed..];
            if self.buffer.len() == PAGE_BYTES {
                self.write_page()?;
            }
        }
        Ok(supplied)
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

struct PageReader {
    directory: PathBuf,
    root: PersistedResumeRoot,
    ordinal: u64,
    offset: usize,
    buffer: Vec<u8>,
    prior_digest: [u8; 32],
    observed_bytes: u64,
    complete: bool,
}

impl PageReader {
    fn new(directory: &Path, root: PersistedResumeRoot) -> Self {
        Self {
            directory: directory.to_path_buf(),
            root,
            ordinal: 0,
            offset: 0,
            buffer: Vec::new(),
            prior_digest: [0; 32],
            observed_bytes: 0,
            complete: false,
        }
    }

    fn load_page(&mut self) -> Result<(), std::io::Error> {
        if self.ordinal >= self.root.page_count {
            if self.observed_bytes != self.root.total_bytes
                || self.prior_digest != self.root.final_page_digest
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "continuation page chain does not match its root",
                ));
            }
            self.complete = true;
            return Ok(());
        }
        let path = page_path(&self.directory, self.ordinal);
        let mut file = fs::File::open(path)?;
        let metadata = file.metadata()?;
        let remaining = self.root.total_bytes.saturating_sub(self.observed_bytes);
        let expected = remaining.min(u64::from(self.root.page_bytes));
        if !metadata.file_type().is_file() || metadata.len() != expected {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "continuation page has an unexpected type or length",
            ));
        }
        self.buffer.clear();
        std::io::Read::by_ref(&mut file)
            .take(u64::from(self.root.page_bytes) + 1)
            .read_to_end(&mut self.buffer)?;
        if self.buffer.len() as u64 != expected {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "continuation page changed while it was read",
            ));
        }
        let digest = page_digest(
            &self.root.lineage,
            self.ordinal,
            self.prior_digest,
            &self.buffer,
        );
        self.prior_digest = digest;
        self.observed_bytes = self
            .observed_bytes
            .checked_add(expected)
            .ok_or_else(|| std::io::Error::other("continuation length overflow"))?;
        self.ordinal = self
            .ordinal
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("continuation page ordinal overflow"))?;
        self.offset = 0;
        Ok(())
    }
}

impl std::io::Read for PageReader {
    fn read(&mut self, output: &mut [u8]) -> Result<usize, std::io::Error> {
        if output.is_empty() {
            return Ok(0);
        }
        while self.offset == self.buffer.len() && !self.complete {
            self.load_page()?;
        }
        if self.complete {
            return Ok(0);
        }
        let available = &self.buffer[self.offset..];
        let copied = available.len().min(output.len());
        output[..copied].copy_from_slice(&available[..copied]);
        self.offset += copied;
        Ok(copied)
    }
}

fn page_digest(
    lineage: &ResumeLineage,
    ordinal: u64,
    prior_digest: [u8; 32],
    bytes: &[u8],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(PAGE_DOMAIN);
    lineage.hash(&mut hasher);
    hasher.update(ordinal.to_be_bytes());
    hasher.update(prior_digest);
    hasher.update(
        u64::try_from(bytes.len())
            .expect("a bounded continuation page length fits u64")
            .to_be_bytes(),
    );
    hasher.update(bytes);
    hasher.finalize().into()
}

fn page_path(directory: &Path, ordinal: u64) -> PathBuf {
    directory.join(format!("{ordinal:016x}.page"))
}

pub(super) fn sync_directory(
    path: &Path,
    operation: &'static str,
) -> Result<(), ProductRunServiceError> {
    sync_directory_os(path).map_err(|error| persistence(operation, error))
}

#[cfg(not(windows))]
fn sync_directory_os(path: &Path) -> std::io::Result<()> {
    fs::File::open(path)?.sync_all()
}

#[cfg(windows)]
fn sync_directory_os(path: &Path) -> std::io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt as _;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?
        .sync_all()
}

fn persistence(operation: &'static str, error: impl std::fmt::Display) -> ProductRunServiceError {
    ProductRunServiceError::persistence(operation, error)
}

fn invalid(operation: &'static str, error: impl std::fmt::Display) -> ProductRunServiceError {
    ProductRunServiceError::internal(operation, error.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        use std::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
        text
    })
}

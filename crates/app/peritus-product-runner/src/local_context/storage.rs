//! Single-owner C0 journal/artifact storage and atomic checkpoint-root publication.

mod append;
mod identity;
mod inspection;
mod location;
pub(super) use inspection::inspect;
#[cfg(test)]
mod tests;

use super::error;
use identity::StorageIdentity;
use peritus_agent::DeveloperLoopError;
use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_codec::{CodecLimits, decode_frame, sha256};
use peritus_context::working::WorkingBinding;
use peritus_journal::{AggregateHead, SqliteJournal, SqliteJournalOptions};
use peritus_types::Sha256Digest;
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
    time::Duration,
};

const FRAME_FAMILY: u16 = 3401;
const STATE_NAMESPACE: u16 = 3401;
const STATE_KEY: &[u8] = b"local-working-memory/checkpoint/v1";
pub(super) const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RECORDS: u64 = 262_144;

/// Local artifact handle with exact verified size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StoredArtifact {
    pub(super) digest: Sha256Digest,
    pub(super) bytes: u64,
}

mod artifact_wire;

pub(super) struct LocalStore {
    journal: SqliteJournal,
    artifacts: ArtifactStore,
    identity: StorageIdentity,
    head: Option<AggregateHead>,
    generation: u64,
    // The lock's lifetime covers both journal and artifact owners, including failed invocations.
    _owner: File,
}

impl LocalStore {
    pub(super) fn open(
        root: &Path,
        workspace: &Path,
        binding: WorkingBinding,
    ) -> Result<Self, DeveloperLoopError> {
        Self::open_folder(root, workspace, binding, &[])
    }

    pub(super) fn open_folder(
        root: &Path,
        workspace: &Path,
        binding: WorkingBinding,
        protected: &[std::path::PathBuf],
    ) -> Result<Self, DeveloperLoopError> {
        location::validate(root, workspace, protected)?;
        fs::create_dir_all(root).map_err(|_| error("create storage root"))?;
        let root = root.canonicalize().map_err(|_| error("resolve storage root"))?;
        location::validate(&root, workspace, protected)?;
        let owner = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("owner.lock"))
            .map_err(|_| error("open lineage ownership lock"))?;
        lock_owner(&owner)?;
        let identity = StorageIdentity::new(binding)?;
        let database = root.join("journal.sqlite3");
        let journal = SqliteJournal::open(
            &database,
            identity.store,
            SqliteJournalOptions { busy_timeout: Duration::from_millis(250) },
        )
        .map_err(|_| error("open C0 journal"))?;
        let config =
            StoreConfig::new(root.join("artifacts"), MAX_ARTIFACT_BYTES, 1024 * 1024 * 1024)
                .and_then(|config| config.with_database_path(&database))
                .map_err(|_| error("configure C0 artifact store"))?;
        let artifacts = ArtifactStore::open(config).map_err(|_| error("open C0 artifact store"))?;
        let head = journal.head(identity.aggregate).map_err(|_| error("read lineage head"))?;
        let checkpoint = journal
            .state_record(STATE_NAMESPACE, STATE_KEY)
            .map_err(|_| error("read checkpoint root"))?;
        let generation =
            checkpoint.as_ref().map_or(0, peritus_journal::DurableStateRecord::revision);
        Ok(Self { journal, artifacts, identity, head, generation, _owner: owner })
    }

    pub(super) const fn scope_digest(&self) -> Sha256Digest {
        self.identity.scope
    }
    pub(super) const fn generation(&self) -> u64 {
        self.generation
    }
    pub(super) fn sequence(&self) -> u64 {
        self.head.map_or(0, |head| head.sequence().get())
    }

    pub(super) fn records(&self) -> Result<Vec<Vec<u8>>, DeveloperLoopError> {
        if self.sequence() > MAX_RECORDS {
            return Err(error("journal record capacity exceeded"));
        }
        self.journal
            .records_for_aggregate(self.identity.aggregate)
            .map_err(|_| error("verify journal chain"))?
            .into_iter()
            .map(|record| {
                let frame = decode_frame(record.frame_bytes(), CodecLimits::PRODUCTION)
                    .map_err(|_| error("decode local event frame"))?;
                if frame.header().family() != FRAME_FAMILY || frame.header().schema_version() != 1 {
                    return Err(error("unsupported local event schema"));
                }
                Ok(frame.payload().to_vec())
            })
            .collect()
    }

    pub(super) fn checkpoint_root(&self) -> Result<Option<Vec<u8>>, DeveloperLoopError> {
        self.journal
            .state_record(STATE_NAMESPACE, STATE_KEY)
            .map(|record| record.map(|record| record.bytes().to_vec()))
            .map_err(|_| error("read committed checkpoint"))
    }

    pub(super) fn store(&self, bytes: &[u8]) -> Result<StoredArtifact, DeveloperLoopError> {
        let length = u64::try_from(bytes.len()).map_err(|_| error("artifact size overflow"))?;
        if length > MAX_ARTIFACT_BYTES {
            return Err(error("artifact capacity exceeded"));
        }
        let digest = sha256(bytes);
        let event = self.identity.event(
            self.sequence().checked_add(1).ok_or_else(|| error("event sequence overflow"))?,
        )?;
        let request = WriteRequest::new(
            ArtifactDigest::from_sha256(digest),
            length,
            MAX_ARTIFACT_BYTES,
            MediaType::new("application/octet-stream").map_err(|_| error("artifact media type"))?,
            EncryptionMetadata::unencrypted(),
            event,
        );
        let mut writer =
            self.artifacts.begin_write(request).map_err(|_| error("begin artifact write"))?;
        writer.write_chunk(bytes).map_err(|_| error("write artifact bytes"))?;
        let finalized = writer.finalize().map_err(|_| error("finalize artifact"))?;
        Ok(StoredArtifact { digest, bytes: finalized.size() })
    }

    pub(super) fn read(&self, artifact: StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError> {
        if artifact.bytes > MAX_ARTIFACT_BYTES {
            return Err(error("artifact read capacity exceeded"));
        }
        let bytes = self
            .artifacts
            .read(ArtifactDigest::from_sha256(artifact.digest), artifact.bytes)
            .map_err(|_| error("artifact unavailable or digest mismatch"))?;
        if bytes.len() as u64 != artifact.bytes {
            return Err(error("artifact length mismatch"));
        }
        Ok(bytes)
    }
}

fn lock_owner(owner: &File) -> Result<(), DeveloperLoopError> {
    let started = std::time::Instant::now();
    loop {
        match owner.try_lock() {
            Ok(()) => return Ok(()),
            Err(fs::TryLockError::WouldBlock) if started.elapsed() < Duration::from_millis(250) => {
                // Another thread's fork can briefly inherit a CLOEXEC descriptor before exec.
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return Err(error("lineage already has an owner or cannot be locked")),
        }
    }
}

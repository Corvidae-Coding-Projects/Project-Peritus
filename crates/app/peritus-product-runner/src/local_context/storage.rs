//! Single-owner C0 journal/artifact storage and atomic checkpoint-root publication.

mod append;
mod identity;
mod inspection;
mod location;
mod ownership;
pub(super) use inspection::inspect;
pub(in crate::local_context) use inspection::open as open_inspection;
pub use inspection::LocalContextInspection;
#[cfg(test)]
mod tests;

use super::error;
use identity::StorageIdentity;
use peritus_agent::DeveloperLoopError;
use peritus_artifact_store::{
    ArtifactCatalogCancellation, ArtifactDigest, ArtifactReadHandle, ArtifactStore, ArtifactStoreError,
    EncryptionMetadata, ErrorCode as ArtifactErrorCode, MediaType, ReferenceOwner, StoreConfig, WriteRequest,
};
use peritus_codec::{CodecLimits, decode_frame, sha256};
use peritus_context::working::WorkingBinding;
use peritus_journal::{AggregateHead, JournalCancellation, JournalError, JournalErrorKind, SqliteJournal};
use peritus_provider_core::CancellationToken;
use peritus_types::Sha256Digest;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

const FRAME_FAMILY: u16 = 3401;
const STATE_NAMESPACE: u16 = 3401;
const STATE_KEY: &[u8] = b"local-working-memory/checkpoint/v1";
pub(super) const MAX_ARTIFACT_BYTES: u64 = i64::MAX as u64;

/// Local artifact handle with exact verified size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StoredArtifact {
    pub(super) digest: Sha256Digest,
    pub(super) bytes: u64,
}

mod artifact_wire;

pub(super) struct LocalStore {
    root: PathBuf,
    journal: SqliteJournal,
    artifacts: ArtifactStore,
    identity: StorageIdentity,
    head: Option<AggregateHead>,
    generation: u64,
    journal_cancellation: JournalCancellation,
    catalog_cancellation: ArtifactCatalogCancellation,
    // The lock's lifetime covers both journal and artifact owners, including failed invocations.
    _owner: File,
}

#[derive(Clone, Copy)]
pub(super) struct LocalAppendReceipt {
    owner: Sha256Digest,
}

impl LocalAppendReceipt {
    pub(super) const fn owner(self) -> Sha256Digest {
        self.owner
    }
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
        Self::open_folder_cancellable(root, workspace, binding, protected, &CancellationToken::new())
    }

    pub(super) fn open_folder_cancellable(
        root: &Path,
        workspace: &Path,
        binding: WorkingBinding,
        protected: &[PathBuf],
        cancellation: &CancellationToken,
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
            .map_err(|failure| context_failure("open lineage ownership lock", failure))?;
        let identity = StorageIdentity::new(binding)?;
        ownership::acquire(&owner, &root, identity.scope, cancellation)?;
        let journal_signal = cancellation.clone();
        let catalog_signal = cancellation.clone();
        let journal_cancellation = JournalCancellation::with_cancellation_check(move || journal_signal.is_cancelled());
        let catalog_cancellation = ArtifactCatalogCancellation::with_cancellation_check(move || catalog_signal.is_cancelled());
        let database = root.join("journal.sqlite3");
        let journal = SqliteJournal::open_waiting_with_options(
            &database,
            identity.store,
            peritus_journal::SqliteJournalOptions::native(),
            &journal_cancellation,
        )
        .map_err(|failure| map_journal_failure(&journal_cancellation, "open C0 journal", failure))?;
        let config = StoreConfig::for_available_space(root.join("artifacts"), MAX_ARTIFACT_BYTES)
            .and_then(|config| config.with_database_path(&database))
            .map_err(|_| error("configure C0 artifact store"))?;
        let artifacts = catalog_cancellation.run(|| ArtifactStore::open(config))
            .map_err(|failure| map_artifact_failure("open C0 artifact store", failure))?;
        let head = journal_cancellation.run(|| journal.head(identity.aggregate))
            .map_err(|failure| map_journal_failure(&journal_cancellation, "read lineage head", failure))?;
        let checkpoint = journal_cancellation.run(|| journal.state_record(STATE_NAMESPACE, STATE_KEY))
            .map_err(|failure| map_journal_failure(&journal_cancellation, "read checkpoint root", failure))?;
        let generation =
            checkpoint.as_ref().map_or(0, peritus_journal::DurableStateRecord::revision);
        Ok(Self { root, journal, artifacts, identity, head, generation, journal_cancellation, catalog_cancellation, _owner: owner })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
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
        self.check_cancelled()?;
        self.journal_cancellation.run(|| self.journal
            .records_for_aggregate(self.identity.aggregate)
        )
            .map_err(|failure| self.journal_failure("verify journal chain", failure))?
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

    pub(super) fn record_page_after(
        &self,
        sequence: u64,
    ) -> Result<Vec<Vec<u8>>, DeveloperLoopError> {
        self.check_cancelled()?;
        self.journal_cancellation.run(|| self.journal
            .aggregate_events_after(
                self.identity.aggregate,
                sequence,
                peritus_journal::MAX_GLOBAL_WINDOW_RECORDS,
            )
        )
            .map_err(|failure| self.journal_failure("verify journal suffix page", failure))?
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
        self.check_cancelled()?;
        self.journal_cancellation.run(|| self.journal
            .state_record(STATE_NAMESPACE, STATE_KEY)
        )
            .map(|record| record.map(|record| record.bytes().to_vec()))
            .map_err(|failure| self.journal_failure("read committed checkpoint", failure))
    }

    pub(super) fn store(&self, bytes: &[u8]) -> Result<StoredArtifact, DeveloperLoopError> {
        self.check_cancelled()?;
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
        self.catalog_cancellation.run(|| {
        let mut writer =
            self.artifacts.begin_write(request).map_err(|failure| self.artifact_failure("begin artifact write", failure))?;
        writer.write_chunk(bytes).map_err(|failure| self.artifact_failure("write artifact bytes", failure))?;
        let finalized = writer.finalize().map_err(|failure| self.artifact_failure("finalize artifact", failure))?;
        Ok(StoredArtifact { digest, bytes: finalized.size() })
        })
    }

    pub(super) fn store_bundle(
        &self,
        bytes: &[u8],
        children: &[StoredArtifact],
    ) -> Result<StoredArtifact, DeveloperLoopError> {
        let parent = self.store(bytes)?;
        self.check_cancelled()?;
        self.catalog_cancellation.run(|| {
            self.artifacts.bind_dependencies(
                ArtifactDigest::from_sha256(parent.digest),
                children.iter().map(|child| {
                    (ArtifactDigest::from_sha256(child.digest), child.bytes)
                }),
            )
        })
            .map_err(|failure| self.artifact_failure("bind artifact dependency bundle", failure))?;
        Ok(parent)
    }

    pub(super) fn read(&self, artifact: StoredArtifact) -> Result<Vec<u8>, DeveloperLoopError> {
        self.check_cancelled()?;
        if artifact.bytes > MAX_ARTIFACT_BYTES {
            return Err(error("artifact read capacity exceeded"));
        }
        let bytes = self.catalog_cancellation.run(|| self
            .artifacts
            .read(ArtifactDigest::from_sha256(artifact.digest), artifact.bytes)
        )
            .map_err(|failure| self.artifact_failure("artifact unavailable or digest mismatch", failure))?;
        if bytes.len() as u64 != artifact.bytes {
            return Err(error("artifact length mismatch"));
        }
        Ok(bytes)
    }

    pub(super) fn open_artifact(
        &self,
        artifact: StoredArtifact,
    ) -> Result<ArtifactReadHandle, DeveloperLoopError> {
        self.check_cancelled()?;
        if artifact.bytes > MAX_ARTIFACT_BYTES {
            return Err(error("artifact read capacity exceeded"));
        }
        let digest = ArtifactDigest::from_sha256(artifact.digest);
        let handle = self.catalog_cancellation.run(|| self
            .artifacts
            .open_read(digest)
        )
            .map_err(|failure| self.artifact_failure("open verified artifact reader", failure))?;
        if handle.metadata().digest() != digest {
            return Err(error("artifact reader digest mismatch"));
        }
        if handle.metadata().size() != artifact.bytes {
            return Err(error("artifact reader length mismatch"));
        }
        Ok(handle)
    }

    pub(super) fn read_digest(&self, digest: [u8; 32]) -> Result<Vec<u8>, DeveloperLoopError> {
        self.check_cancelled()?;
        self.catalog_cancellation.run(|| self.artifacts
            .read(ArtifactDigest::from_sha256(Sha256Digest::new(digest)), MAX_ARTIFACT_BYTES)
        )
            .map_err(|failure| self.artifact_failure("artifact unavailable or digest mismatch", failure))
    }

    pub(super) fn checkpoint_owner(
        &self,
        sequence: u64,
        payload: &[u8],
    ) -> Result<Sha256Digest, DeveloperLoopError> {
        self.check_cancelled()?;
        let command = self.identity.command(sequence)?;
        match self.journal_cancellation.run(|| self
            .journal
            .resolve_command(command, sha256(payload))
        )
            .map_err(|failure| self.journal_failure("resolve checkpoint publication", failure))?
        {
            peritus_journal::CommandResolution::Committed(batch)
                if batch.records().len() == 1
                    && batch.records()[0].aggregate() == self.identity.aggregate
                    && batch.records()[0].sequence().get() == sequence =>
            {
                Ok(batch.batch_hash())
            }
            _ => Err(error("checkpoint publication receipt is unavailable")),
        }
    }

    pub(super) fn retire_checkpoint_owner(
        &self,
        owner: Sha256Digest,
    ) -> Result<(), DeveloperLoopError> {
        self.check_cancelled()?;
        self.catalog_cancellation.run(|| self.artifacts
            .retire_reference_owner(ReferenceOwner::journal(owner))
        )
            .map(|_| ())
            .map_err(|failure| self.artifact_failure("retire obsolete checkpoint artifact roots", failure))
    }

    fn check_cancelled(&self) -> Result<(), DeveloperLoopError> {
        if self.journal_cancellation.is_cancelled() || self.catalog_cancellation.is_cancelled() {
            Err(DeveloperLoopError::Cancelled)
        } else { Ok(()) }
    }

    fn journal_failure(&self, operation: &str, failure: JournalError) -> DeveloperLoopError {
        map_journal_failure(&self.journal_cancellation, operation, failure)
    }

    fn artifact_failure(&self, operation: &str, failure: ArtifactStoreError) -> DeveloperLoopError {
        map_artifact_failure(operation, failure)
    }
}

fn map_journal_failure(
    cancellation: &JournalCancellation,
    operation: &str,
    failure: JournalError,
) -> DeveloperLoopError {
    if cancellation.is_cancelled() && failure.kind() == JournalErrorKind::Busy {
        DeveloperLoopError::Cancelled
    } else {
        context_failure(operation, failure)
    }
}

fn map_artifact_failure(operation: &str, failure: ArtifactStoreError) -> DeveloperLoopError {
    if failure.code() == ArtifactErrorCode::CatalogWaitCancelled {
        DeveloperLoopError::Cancelled
    } else {
        context_failure(operation, failure)
    }
}

fn context_failure(operation: &str, failure: impl std::fmt::Display) -> DeveloperLoopError {
    DeveloperLoopError::Context(format!("local working memory: {operation}: {failure}"))
}

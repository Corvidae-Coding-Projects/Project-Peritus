//! Serialized transfer registry co-owned with the journal and artifact catalog.

mod error;

use std::collections::BTreeMap;

use peritus_app_protocol::{
    AppEventPayload, ArtifactCancellation, ArtifactChunk, ArtifactCompletion, ArtifactMetadata,
    ArtifactTransferState, CanonicalMediaType, TransferId,
};
use peritus_artifact_store::{
    ArtifactDigest, ArtifactReadHandle, ArtifactStore, ArtifactWriteHandle, EncryptionMetadata,
    MediaType, WriteRequest,
};
use peritus_journal::{ApplicationArtifactState, NewApplicationArtifact, SqliteJournal};
use peritus_types::{ActorId, SessionId};

use super::publication;
use crate::DaemonError;

use error::{
    corrupt, invalid, journal_error, require_owner, resource_limit, store_error, transfer_error,
};

pub(crate) struct ArtifactPoll {
    pub(crate) payload: AppEventPayload,
    pub(crate) terminal: bool,
}

pub(crate) struct ArtifactAuthority {
    store: ArtifactStore,
    transfers: BTreeMap<TransferId, ActiveTransfer>,
    maximum_artifact_bytes: u64,
    maximum_transfers: usize,
}

enum ActiveTransfer {
    Download(Download),
    Upload(Upload),
}

struct Download {
    actor_id: ActorId,
    session_id: SessionId,
    state: ArtifactTransferState,
    reader: ArtifactReadHandle,
}

struct Upload {
    actor_id: ActorId,
    session_id: SessionId,
    state: ArtifactTransferState,
    writer: ArtifactWriteHandle,
}

impl ArtifactAuthority {
    pub(crate) fn new(
        store: ArtifactStore,
        maximum_artifact_bytes: u64,
        maximum_transfers: usize,
    ) -> Result<Self, DaemonError> {
        if maximum_artifact_bytes == 0 || maximum_transfers == 0 {
            return Err(invalid("artifact service limits must be positive"));
        }
        Ok(Self { store, transfers: BTreeMap::new(), maximum_artifact_bytes, maximum_transfers })
    }

    pub(crate) fn open_download(
        &mut self,
        journal: &SqliteJournal,
        actor_id: ActorId,
        session_id: SessionId,
        transfer_id: TransferId,
        artifact_id: peritus_types::ArtifactId,
        maximum_chunk_bytes: usize,
    ) -> Result<ArtifactMetadata, DaemonError> {
        self.ensure_capacity(transfer_id)?;
        let catalog = journal
            .application_artifact(artifact_id)
            .map_err(journal_error)?
            .ok_or_else(|| invalid("application artifact does not exist"))?;
        if catalog.state() != ApplicationArtifactState::Available {
            return Err(invalid("application artifact is not available"));
        }
        let digest = ArtifactDigest::from_sha256(catalog.digest());
        let reader = self.store.open_read(digest).map_err(store_error)?;
        if reader.metadata().size() != catalog.byte_size()
            || reader.metadata().digest() != digest
            || reader.metadata().media_type().as_str() != catalog.media_type()
        {
            return Err(corrupt("artifact store metadata disagrees with application catalog"));
        }
        let media = CanonicalMediaType::new(catalog.media_type().to_owned(), 255)
            .map_err(transfer_error)?;
        let preferred = u32::try_from(maximum_chunk_bytes.min(64 * 1024))
            .map_err(|_| invalid("negotiated artifact chunk limit cannot be represented"))?;
        let metadata = ArtifactMetadata::new(
            transfer_id,
            artifact_id,
            catalog.byte_size(),
            media,
            catalog.digest(),
            preferred,
            maximum_chunk_bytes,
        )
        .map_err(transfer_error)?;
        let state = ArtifactTransferState::new(metadata.clone(), maximum_chunk_bytes)
            .map_err(transfer_error)?;
        self.transfers.insert(
            transfer_id,
            ActiveTransfer::Download(Download { actor_id, session_id, state, reader }),
        );
        Ok(metadata)
    }

    pub(crate) fn begin_upload(
        &mut self,
        journal: &mut SqliteJournal,
        actor_id: ActorId,
        session_id: SessionId,
        metadata: ArtifactMetadata,
        maximum_chunk_bytes: usize,
    ) -> Result<(), DaemonError> {
        self.ensure_capacity(metadata.transfer_id())?;
        if metadata.byte_size() > self.maximum_artifact_bytes {
            return Err(resource_limit("artifact exceeds the configured per-object limit"));
        }
        let state = ArtifactTransferState::new(metadata.clone(), maximum_chunk_bytes)
            .map_err(transfer_error)?;
        let catalog = NewApplicationArtifact::new(
            metadata.artifact_id(),
            metadata.digest(),
            metadata.byte_size(),
            metadata.media_type().as_str().to_owned(),
        )
        .map_err(journal_error)?;
        let retained = journal.begin_application_artifact(catalog).map_err(journal_error)?;
        if retained.state() == ApplicationArtifactState::Available {
            return Err(invalid("application artifact is already available"));
        }
        let request = WriteRequest::new(
            ArtifactDigest::from_sha256(metadata.digest()),
            metadata.byte_size(),
            metadata.byte_size().max(1),
            MediaType::new(metadata.media_type().as_str()).map_err(store_error)?,
            EncryptionMetadata::unencrypted(),
            publication::event_id(&metadata),
        );
        let writer = self.store.begin_owned_write(request).map_err(store_error)?;
        self.transfers.insert(
            metadata.transfer_id(),
            ActiveTransfer::Upload(Upload { actor_id, session_id, state, writer }),
        );
        Ok(())
    }

    pub(crate) fn upload_chunk(
        &mut self,
        actor_id: ActorId,
        session_id: SessionId,
        chunk: &ArtifactChunk,
    ) -> Result<(), DaemonError> {
        let transfer_id = chunk.transfer_id();
        let result = {
            let upload = self.upload_mut(transfer_id, actor_id, session_id)?;
            upload.state.accept_chunk(chunk).map_err(transfer_error)?;
            upload.writer.write_chunk(chunk.bytes()).map_err(store_error)
        };
        if result.is_err() {
            self.transfers.remove(&transfer_id);
        }
        result
    }

    pub(crate) fn complete_upload(
        &mut self,
        journal: &mut SqliteJournal,
        actor_id: ActorId,
        session_id: SessionId,
        completion: ArtifactCompletion,
    ) -> Result<(), DaemonError> {
        let transfer_id = completion.transfer_id();
        let mut upload = match self.transfers.remove(&transfer_id) {
            Some(ActiveTransfer::Upload(upload)) => upload,
            Some(other) => {
                self.transfers.insert(transfer_id, other);
                return Err(invalid("artifact transfer is not an upload"));
            }
            None => return Err(invalid("artifact upload does not exist")),
        };
        require_owner(upload.actor_id, upload.session_id, actor_id, session_id)?;
        let metadata = upload.state.metadata().clone();
        if completion.artifact_id() != metadata.artifact_id()
            || completion.byte_size() != metadata.byte_size()
            || completion.digest() != metadata.digest()
        {
            return Err(invalid("artifact completion disagrees with upload metadata"));
        }
        upload.state.complete(completion.digest()).map_err(transfer_error)?;
        let batch = publication::record(journal, &metadata)?;
        let finalized = self.store.complete_write(upload.writer).map_err(store_error)?;
        if finalized.size() != metadata.byte_size()
            || finalized.digest() != ArtifactDigest::from_sha256(metadata.digest())
        {
            return Err(corrupt("finalized artifact observation disagrees with upload metadata"));
        }
        journal
            .complete_application_artifact(metadata.artifact_id(), batch.last_position())
            .map_err(journal_error)?;
        Ok(())
    }

    pub(crate) fn poll_download(
        &mut self,
        actor_id: ActorId,
        session_id: SessionId,
        transfer_id: TransferId,
        maximum_chunk_bytes: usize,
    ) -> Result<ArtifactPoll, DaemonError> {
        let download = match self.transfers.get_mut(&transfer_id) {
            Some(ActiveTransfer::Download(download)) => download,
            Some(ActiveTransfer::Upload(_)) => {
                return Err(invalid("artifact transfer is not a download"));
            }
            None => return Err(invalid("artifact download does not exist")),
        };
        require_owner(download.actor_id, download.session_id, actor_id, session_id)?;
        let metadata = download.state.metadata().clone();
        let preferred_chunk_bytes = usize::try_from(metadata.preferred_chunk_size())
            .map_err(|_| invalid("artifact preferred chunk size cannot be represented"))?;
        let chunk_bytes = preferred_chunk_bytes.min(maximum_chunk_bytes);
        if chunk_bytes == 0 {
            return Err(invalid("artifact download chunk limit must be positive"));
        }
        match download.reader.read_chunk(chunk_bytes).map_err(store_error)? {
            Some(read) => {
                let chunk = ArtifactChunk::new(
                    transfer_id,
                    metadata.artifact_id(),
                    download.state.next_ordinal(),
                    read.offset(),
                    read.bytes().to_vec(),
                    chunk_bytes,
                )
                .map_err(transfer_error)?;
                download.state.accept_chunk(&chunk).map_err(transfer_error)?;
                Ok(ArtifactPoll { payload: AppEventPayload::ArtifactChunk(chunk), terminal: false })
            }
            None => {
                download.state.complete(metadata.digest()).map_err(transfer_error)?;
                self.transfers.remove(&transfer_id);
                Ok(ArtifactPoll {
                    payload: AppEventPayload::ArtifactComplete(ArtifactCompletion::new(
                        transfer_id,
                        metadata.artifact_id(),
                        metadata.byte_size(),
                        metadata.digest(),
                    )),
                    terminal: true,
                })
            }
        }
    }

    pub(crate) fn cancel(
        &mut self,
        actor_id: ActorId,
        session_id: SessionId,
        cancellation: ArtifactCancellation,
    ) -> Result<(), DaemonError> {
        let transfer_id = cancellation.transfer_id();
        let transfer = self
            .transfers
            .get_mut(&transfer_id)
            .ok_or_else(|| invalid("artifact transfer does not exist"))?;
        let (owner, session, state) = match transfer {
            ActiveTransfer::Download(value) => (value.actor_id, value.session_id, &mut value.state),
            ActiveTransfer::Upload(value) => (value.actor_id, value.session_id, &mut value.state),
        };
        require_owner(owner, session, actor_id, session_id)?;
        state.cancel(cancellation).map_err(transfer_error)?;
        self.transfers.remove(&transfer_id);
        Ok(())
    }

    pub(crate) fn abandon(
        &mut self,
        actor_id: ActorId,
        session_id: SessionId,
        transfer_ids: &[TransferId],
    ) {
        for transfer_id in transfer_ids {
            let owned = self.transfers.get(transfer_id).is_some_and(|transfer| match transfer {
                ActiveTransfer::Download(value) => {
                    value.actor_id == actor_id && value.session_id == session_id
                }
                ActiveTransfer::Upload(value) => {
                    value.actor_id == actor_id && value.session_id == session_id
                }
            });
            if owned {
                self.transfers.remove(transfer_id);
            }
        }
    }

    fn ensure_capacity(&self, transfer_id: TransferId) -> Result<(), DaemonError> {
        if self.transfers.contains_key(&transfer_id) {
            return Err(invalid("artifact transfer identity is already active"));
        }
        if self.transfers.len() >= self.maximum_transfers {
            return Err(resource_limit("artifact transfer registry is full"));
        }
        Ok(())
    }

    fn upload_mut(
        &mut self,
        transfer_id: TransferId,
        actor_id: ActorId,
        session_id: SessionId,
    ) -> Result<&mut Upload, DaemonError> {
        let upload = match self.transfers.get_mut(&transfer_id) {
            Some(ActiveTransfer::Upload(upload)) => upload,
            Some(ActiveTransfer::Download(_)) => {
                return Err(invalid("artifact transfer is not an upload"));
            }
            None => return Err(invalid("artifact upload does not exist")),
        };
        require_owner(upload.actor_id, upload.session_id, actor_id, session_id)?;
        Ok(upload)
    }
}

//! Serialized transfer registry co-owned with the journal and artifact catalog.

mod error;
mod scoped;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use peritus_app_protocol::{
    AppEventPayload, ArtifactCancellation, ArtifactChunk, ArtifactCompletion, ArtifactMetadata,
    ArtifactTransferState, CanonicalMediaType, TransferId,
};
use peritus_artifact_store::{
    ArtifactCatalogCancellation, ArtifactDigest, ArtifactReadHandle, ArtifactStore,
    ArtifactWriteHandle, EncryptionMetadata, FinalizedArtifact, MediaType, WriteRequest,
};
use peritus_journal::{ApplicationArtifactState, NewApplicationArtifact, SqliteJournal};
use peritus_types::{ActorId, SessionId};

use super::{ArtifactScope, publication, scope};
use crate::DaemonError;

use error::{
    corrupt, invalid, journal_error, require_owner, resource_limit, store_error, transfer_error,
};

/// Global physical transfer slots owned by the production artifact authority.
///
/// This is independent of application-command idempotency capacity. Completed and abandoned
/// transfers release their slots.
const PRODUCTION_ACTIVE_TRANSFERS: usize = 4_096;

pub struct ArtifactPoll {
    pub(crate) payload: AppEventPayload,
    pub(crate) terminal: bool,
}

pub struct ArtifactAuthority {
    store: Arc<ArtifactStoreOwnerQueue>,
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
    writer: Option<ArtifactWriteHandle>,
    producing_position: Option<u64>,
    cancellation: ArtifactCatalogCancellation,
    finalizing: bool,
}

pub(crate) struct PendingArtifactCompletion {
    transfer_id: TransferId,
    metadata: ArtifactMetadata,
    producing_position: u64,
    writer: ArtifactWriteHandle,
    cancellation: ArtifactCatalogCancellation,
}

pub(crate) struct CompletedArtifactFinalization {
    pending: PendingArtifactCompletion,
    result: Result<FinalizedArtifact, DaemonError>,
}

pub(crate) struct ArtifactStoreOwnerQueue {
    owner: std::sync::Mutex<ArtifactStore>,
    order: std::sync::Mutex<ArtifactQueueState>,
    ready: std::sync::Condvar,
}

#[derive(Default)]
struct ArtifactQueueState {
    active: bool,
    waiters: VecDeque<Arc<()>>,
}

struct ArtifactStorePermit<'a>(&'a ArtifactStoreOwnerQueue);

impl ArtifactStoreOwnerQueue {
    fn new(owner: ArtifactStore) -> Self {
        Self {
            owner: std::sync::Mutex::new(owner),
            order: std::sync::Mutex::new(ArtifactQueueState::default()),
            ready: std::sync::Condvar::new(),
        }
    }

    fn try_acquire(&self) -> Result<ArtifactStorePermit<'_>, DaemonError> {
        let mut order = self
            .order
            .lock()
            .map_err(|_| corrupt("artifact owner queue lock is poisoned"))?;
        if order.active || !order.waiters.is_empty() {
            return Err(resource_limit("artifact store owner is busy"));
        }
        order.active = true;
        Ok(ArtifactStorePermit(self))
    }

    fn acquire(
        &self,
        cancellation: &ArtifactCatalogCancellation,
    ) -> Result<ArtifactStorePermit<'_>, DaemonError> {
        let waiter = Arc::new(());
        let mut order = self
            .order
            .lock()
            .map_err(|_| corrupt("artifact owner queue lock is poisoned"))?;
        order.waiters.push_back(Arc::clone(&waiter));
        loop {
            if cancellation.is_cancelled() {
                if let Some(index) =
                    order.waiters.iter().position(|queued| Arc::ptr_eq(queued, &waiter))
                {
                    order.waiters.remove(index);
                }
                self.ready.notify_all();
                return Err(resource_limit("artifact store owner wait was cancelled"));
            }
            if !order.active
                && order.waiters.front().is_some_and(|queued| Arc::ptr_eq(queued, &waiter))
            {
                order.waiters.pop_front();
                order.active = true;
                return Ok(ArtifactStorePermit(self));
            }
            let wake = self
                .ready
                .wait_timeout(order, Duration::from_millis(1))
                .map_err(|_| corrupt("artifact owner queue wait is poisoned"))?;
            order = wake.0;
        }
    }

    fn with_foreground<T>(
        &self,
        operation: impl FnOnce(&ArtifactStore) -> Result<T, DaemonError>,
    ) -> Result<T, DaemonError> {
        let _permit = self.try_acquire()?;
        let owner = self
            .owner
            .lock()
            .map_err(|_| corrupt("artifact store owner lock is poisoned"))?;
        operation(&owner)
    }

    pub(crate) fn finalize(
        &self,
        mut pending: PendingArtifactCompletion,
    ) -> CompletedArtifactFinalization {
        let result = self.acquire(&pending.cancellation).and_then(|_permit| {
            let owner = self
                .owner
                .lock()
                .map_err(|_| corrupt("artifact store owner lock is poisoned"))?;
            owner
                .try_complete_write(&mut pending.writer, &pending.cancellation)
                .map_err(store_error)
        });
        CompletedArtifactFinalization { pending, result }
    }
}

impl Drop for ArtifactStorePermit<'_> {
    fn drop(&mut self) {
        if let Ok(mut order) = self.0.order.lock() {
            order.active = false;
            self.0.ready.notify_all();
        }
    }
}

impl ArtifactAuthority {
    pub(crate) fn production(
        store: ArtifactStore,
        maximum_artifact_bytes: u64,
    ) -> Result<Self, DaemonError> {
        Self::new(store, maximum_artifact_bytes, PRODUCTION_ACTIVE_TRANSFERS)
    }

    pub(crate) fn new(
        store: ArtifactStore,
        maximum_artifact_bytes: u64,
        maximum_transfers: usize,
    ) -> Result<Self, DaemonError> {
        if maximum_artifact_bytes == 0 || maximum_transfers == 0 {
            return Err(invalid("artifact service limits must be positive"));
        }
        Ok(Self {
            store: Arc::new(ArtifactStoreOwnerQueue::new(store)),
            transfers: BTreeMap::new(),
            maximum_artifact_bytes,
            maximum_transfers,
        })
    }

    pub(crate) fn reconcile_restart(
        &self,
        journal: &mut SqliteJournal,
    ) -> Result<(), DaemonError> {
        let mut cursor = None;
        loop {
            let uploading = journal
                .uploading_application_artifacts_after(cursor, 256)
                .map_err(journal_error)?;
            if uploading.is_empty() {
                return Ok(());
            }
            for artifact in &uploading {
                let digest = ArtifactDigest::from_sha256(artifact.digest());
                let durable = self.store.with_foreground(|store| {
                    store.metadata(digest).map_err(store_error)
                })?;
                if let Some(durable) = durable {
                    if durable.digest() != digest || durable.size() != artifact.byte_size() {
                        return Err(corrupt(
                            "restart artifact metadata disagrees with the application catalog",
                        ));
                    }
                    if durable.is_referenceable() {
                        let event = publication::event_id_for(
                            artifact.artifact_id(),
                            artifact.byte_size(),
                            artifact.digest(),
                            artifact.media_type(),
                        );
                        journal
                            .complete_application_artifact_from_event(
                                artifact.artifact_id(),
                                event,
                            )
                            .map_err(journal_error)?;
                    } else {
                        crate::diagnostic::report(&format!(
                            "application artifact remains unresolved after restart: artifact_id={} digest={} size={} integrity={:?} quarantine={:?}",
                            hex(artifact.artifact_id().as_bytes()),
                            digest.to_hex(),
                            artifact.byte_size(),
                            durable.integrity(),
                            durable.quarantine(),
                        ));
                    }
                } else {
                    crate::diagnostic::report(&format!(
                        "application artifact bytes are missing after restart: artifact_id={} digest={} size={}",
                        hex(artifact.artifact_id().as_bytes()),
                        digest.to_hex(),
                        artifact.byte_size(),
                    ));
                }
                cursor = Some(artifact.artifact_id());
            }
            if uploading.len() < 256 {
                return Ok(());
            }
        }
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
        self.open_download_with_media_type_limit(
            journal,
            actor_id,
            session_id,
            transfer_id,
            artifact_id,
            maximum_chunk_bytes,
            usize::MAX,
        )
    }

    pub(crate) fn open_download_with_media_type_limit(
        &mut self,
        journal: &SqliteJournal,
        actor_id: ActorId,
        session_id: SessionId,
        transfer_id: TransferId,
        artifact_id: peritus_types::ArtifactId,
        maximum_chunk_bytes: usize,
        maximum_media_type_bytes: usize,
    ) -> Result<ArtifactMetadata, DaemonError> {
        self.ensure_capacity(transfer_id)?;
        let catalog = journal
            .application_artifact(artifact_id)
            .map_err(journal_error)?
            .ok_or_else(|| invalid("application artifact does not exist"))?;
        if let Some(scope) = scope::claimed_scope(journal, artifact_id)? {
            if scope.actor() != actor_id {
                return Err(scope::unauthorized());
            }
            scope::authorize(journal, scope, &catalog)?;
        }
        if catalog.state() != ApplicationArtifactState::Available {
            return Err(invalid("application artifact is not available"));
        }
        let digest = ArtifactDigest::from_sha256(catalog.digest());
        let reader = self
            .store
            .with_foreground(|store| store.open_read(digest).map_err(store_error))?;
        if reader.metadata().size() != catalog.byte_size() || reader.metadata().digest() != digest {
            return Err(corrupt("artifact store metadata disagrees with application catalog"));
        }
        let media = CanonicalMediaType::new(
            catalog.media_type().to_owned(),
            maximum_media_type_bytes,
        )
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
        self.begin_upload_inner(journal, actor_id, session_id, metadata, maximum_chunk_bytes, None)
    }

    fn begin_upload_inner(
        &mut self,
        journal: &mut SqliteJournal,
        actor_id: ActorId,
        session_id: SessionId,
        metadata: ArtifactMetadata,
        maximum_chunk_bytes: usize,
        scope: Option<ArtifactScope>,
    ) -> Result<(), DaemonError> {
        self.ensure_capacity(metadata.transfer_id())?;
        if metadata.byte_size() > self.maximum_artifact_bytes {
            return Err(resource_limit("artifact exceeds the configured per-object limit"));
        }
        let state = ArtifactTransferState::new(metadata.clone(), maximum_chunk_bytes)
            .map_err(transfer_error)?;
        if self.transfers.values().any(|transfer| matches!(transfer,
            ActiveTransfer::Upload(upload) if upload.state.metadata().artifact_id() == metadata.artifact_id())) {
            return Err(invalid("artifact already has an active upload"));
        }
        if let Some(scope) = scope {
            if scope.actor() != actor_id {
                return Err(scope::unauthorized());
            }
            scope::claim(journal, scope, &metadata)?;
        } else if scope::claimed_scope(journal, metadata.artifact_id())?.is_some() {
            return Err(scope::unauthorized());
        }
        let catalog = NewApplicationArtifact::new(
            metadata.artifact_id(),
            metadata.digest(),
            metadata.byte_size(),
            metadata.media_type().as_str().to_owned(),
        )
        .map_err(journal_error)?;
        // Exact metadata is idempotent. A new transfer still verifies every uploaded byte;
        // finalization deduplicates the content and reuses the original publication receipt.
        journal.begin_application_artifact(catalog).map_err(journal_error)?;
        let request = WriteRequest::new(
            ArtifactDigest::from_sha256(metadata.digest()),
            metadata.byte_size(),
            metadata.byte_size().max(1),
            MediaType::new(metadata.media_type().as_str()).map_err(store_error)?,
            EncryptionMetadata::unencrypted(),
            publication::event_id(&metadata),
        );
        let writer = self
            .store
            .with_foreground(|store| store.begin_owned_write(request).map_err(store_error))?;
        self.transfers.insert(
            metadata.transfer_id(),
            ActiveTransfer::Upload(Upload {
                actor_id,
                session_id,
                state,
                writer: Some(writer),
                producing_position: None,
                cancellation: ArtifactCatalogCancellation::new(),
                finalizing: false,
            }),
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
            if upload.writer.is_none() {
                return Err(resource_limit("artifact upload finalization is already in progress"));
            }
            upload.state.accept_chunk(chunk).map_err(transfer_error)?;
            upload
                .writer
                .as_mut()
                .ok_or_else(|| invalid("artifact upload is already finalizing"))?
                .write_chunk(chunk.bytes())
                .map_err(store_error)
        };
        if result.is_err() {
            self.transfers.remove(&transfer_id);
        }
        result
    }

    pub(crate) fn prepare_complete_upload(
        &mut self,
        journal: &mut SqliteJournal,
        actor_id: ActorId,
        session_id: SessionId,
        completion: ArtifactCompletion,
    ) -> Result<PendingArtifactCompletion, DaemonError> {
        let transfer_id = completion.transfer_id();
        let upload = self.upload_mut(transfer_id, actor_id, session_id)?;
        let metadata = upload.state.metadata().clone();
        if completion.artifact_id() != metadata.artifact_id()
            || completion.byte_size() != metadata.byte_size()
            || completion.digest() != metadata.digest()
        {
            return Err(invalid("artifact completion disagrees with upload metadata"));
        }
        if upload.finalizing {
            return Err(resource_limit("artifact upload finalization is already in progress"));
        }
        let producing_position = match upload.producing_position {
            Some(position) => position,
            None => {
                let mut accepted = upload.state.clone();
                accepted.complete(completion.digest()).map_err(transfer_error)?;
                let position = publication::record(journal, &metadata)?.last_position();
                upload.state = accepted;
                upload.producing_position = Some(position);
                position
            }
        };
        let writer = upload
            .writer
            .take()
            .ok_or_else(|| corrupt("artifact completion lost its exact writer"))?;
        upload.finalizing = true;
        Ok(PendingArtifactCompletion {
            transfer_id,
            metadata,
            producing_position,
            writer,
            cancellation: upload.cancellation.clone(),
        })
    }

    #[cfg(test)]
    pub(crate) fn complete_upload(
        &mut self,
        journal: &mut SqliteJournal,
        actor_id: ActorId,
        session_id: SessionId,
        completion: ArtifactCompletion,
    ) -> Result<(), DaemonError> {
        let pending =
            self.prepare_complete_upload(journal, actor_id, session_id, completion)?;
        let completed = self.store.finalize(pending);
        self.reconcile_complete_upload(journal, completed)
    }

    pub(crate) fn store_owner(&self) -> Arc<ArtifactStoreOwnerQueue> {
        Arc::clone(&self.store)
    }

    pub(crate) fn reconcile_complete_upload(
        &mut self,
        journal: &mut SqliteJournal,
        completed: CompletedArtifactFinalization,
    ) -> Result<(), DaemonError> {
        let transfer_id = completed.pending.transfer_id;
        let upload = match self.transfers.get_mut(&transfer_id) {
            Some(ActiveTransfer::Upload(upload)) => upload,
            Some(ActiveTransfer::Download(_)) => {
                return Err(corrupt("completed artifact finalization resolved to a download"));
            }
            None => return Err(corrupt("completed artifact finalization lost its transfer")),
        };
        upload.writer = Some(completed.pending.writer);
        upload.finalizing = false;
        if upload.cancellation.is_cancelled() {
            upload.cancellation = ArtifactCatalogCancellation::new();
        }
        let finalized = completed.result?;
        if finalized.size() != completed.pending.metadata.byte_size()
            || finalized.digest()
                != ArtifactDigest::from_sha256(completed.pending.metadata.digest())
        {
            return Err(corrupt("finalized artifact observation disagrees with upload metadata"));
        }
        journal
            .complete_application_artifact(
                completed.pending.metadata.artifact_id(),
                completed.pending.producing_position,
            )
            .map_err(journal_error)?;
        self.transfers.remove(&transfer_id);
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
        if let Some(read) = download.reader.read_chunk(chunk_bytes).map_err(store_error)? {
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
        } else {
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
        match transfer {
            ActiveTransfer::Download(value) => {
                require_owner(value.actor_id, value.session_id, actor_id, session_id)?;
                value.state.cancel(cancellation).map_err(transfer_error)?;
                self.transfers.remove(&transfer_id);
            }
            ActiveTransfer::Upload(value) => {
                require_owner(value.actor_id, value.session_id, actor_id, session_id)?;
                if value.producing_position.is_some() || value.finalizing {
                    value.cancellation.cancel();
                } else {
                    value.state.cancel(cancellation).map_err(transfer_error)?;
                    self.transfers.remove(&transfer_id);
                }
            }
        }
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
            let accepted_completion = self.transfers.get(transfer_id).is_some_and(|transfer| {
                matches!(transfer, ActiveTransfer::Upload(value) if value.producing_position.is_some() || value.finalizing)
            });
            if owned && !accepted_completion {
                self.transfers.remove(transfer_id);
            }
        }
    }

    pub(crate) fn cancel_finalizations(&self) {
        for transfer in self.transfers.values() {
            if let ActiveTransfer::Upload(upload) = transfer {
                if upload.producing_position.is_some() || upload.finalizing {
                    upload.cancellation.cancel();
                }
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

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

//! Run-owned immutable product text and deliverable evidence.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
    sync::{Arc, Mutex},
};

use peritus_app_protocol::{
    MAX_PRODUCT_ARTIFACT_CHUNK_BYTES, MAX_PRODUCT_DELIVERABLE_INDEX_PAGE,
    ProductArtifactPage, ProductArtifactQuery, ProductArtifactReference,
    ProductDeliverableIndexEntry, ProductDeliverableIndexKind, ProductDeliverableIndexPage,
    ProductDeliverableIndexQuery, ProductRunOperationReference, ProductRunReferenceSnapshot,
};
use peritus_artifact_store::{
    ArtifactDigest, ArtifactReadHandle, ArtifactStore, EncryptionMetadata, MediaType,
    ReferenceOwner, StoreConfig, WriteRequest,
};
use peritus_types::{EventId, RunId, Sha256Digest};
use sha2::{Digest as _, Sha256};

use super::super::{ProductRunServiceError, RunRecord};

const MAX_CACHED_READERS: usize = 32;

pub(super) struct ProductArtifactStore {
    config: StoreConfig,
    writer: Mutex<ArtifactStore>,
    readers: Mutex<ReaderCache>,
    authorized: Mutex<BTreeMap<RunId, BTreeSet<Sha256Digest>>>,
}

#[derive(Default)]
struct ReaderCache {
    readers: BTreeMap<Sha256Digest, Arc<Mutex<ArtifactReadHandle>>>,
    recent: VecDeque<Sha256Digest>,
}

impl ProductArtifactStore {
    pub(super) fn open(root: &Path) -> Result<Self, peritus_artifact_store::ArtifactStoreError> {
        let config = StoreConfig::for_available_space_without_artifact_limit(root)?;
        let writer = ArtifactStore::open(config.clone())?;
        Ok(Self {
            config,
            writer: Mutex::new(writer),
            readers: Mutex::new(ReaderCache::default()),
            authorized: Mutex::new(BTreeMap::new()),
        })
    }

    pub(super) fn publish_record(
        &self,
        record: &RunRecord,
    ) -> Result<(), ProductRunServiceError> {
        let run = record.request.run_id();
        self.publish_text(run, record.request.execution_task())?;
        self.referenced_snapshot(
            &record.snapshot,
            super::super::snapshot::delivery_settlement(record),
        )?;
        Ok(())
    }

    pub(super) fn publish_snapshot(
        &self,
        run: RunId,
        snapshot: &peritus_app_protocol::ProductRunSnapshot,
    ) -> Result<ProductRunReferenceSnapshot, ProductRunServiceError> {
        if run != snapshot.run_id() {
            return Err(ProductRunServiceError::InvalidState);
        }
        self.referenced_snapshot(snapshot, None)
    }

    pub(super) fn referenced_snapshot(
        &self,
        snapshot: &peritus_app_protocol::ProductRunSnapshot,
        settlement: Option<peritus_run_settlement::RunSettlement>,
    ) -> Result<ProductRunReferenceSnapshot, ProductRunServiceError> {
        let run = snapshot.run_id();
        let task = self.publish_text(run, snapshot.task())?;
        let status = self.publish_text(run, snapshot.status())?;
        let diff = self.publish_optional_text(run, snapshot.diff())?;
        let gates = self.publish_optional_text(run, snapshot.gates())?;
        let review = self.publish_optional_text(run, snapshot.review())?;
        let summary = self.publish_optional_text(run, snapshot.summary())?;
        let operation = snapshot.operation();
        let operation = ProductRunOperationReference::new(
            operation.kind(),
            operation.state(),
            self.publish_text(run, operation.identity())?,
            self.publish_text(run, operation.known())?,
            self.publish_optional_text(run, operation.uncertainty())?,
            operation.legal_controls(),
        )
        .map_err(|_| ProductRunServiceError::InvalidState)?;
        let deliverable = snapshot
            .deliverable()
            .map(|deliverable| {
                self.publish_deliverable(run, deliverable)?;
                deliverable
                    .reference()
                    .map_err(|_| ProductRunServiceError::InvalidMessage)
            })
            .transpose()?;
        ProductRunReferenceSnapshot::new(
            snapshot.run_id(),
            snapshot.workspace_id(),
            snapshot.providers(),
            snapshot.phase(),
            snapshot.cycle(),
            task,
            status,
            diff,
            gates,
            review,
            summary,
            operation,
            deliverable,
            settlement,
        )
        .map_err(|_| ProductRunServiceError::InvalidState)
    }

    pub(super) fn publish_deliverable(
        &self,
        run: RunId,
        deliverable: &peritus_app_protocol::ProductDeliverable,
    ) -> Result<(), ProductRunServiceError> {
        self.publish_text(run, deliverable.workspace_path())?;
        self.publish_text(run, deliverable.run_instructions())?;
        for value in deliverable.changed_paths().iter().chain(deliverable.successful_commands()) {
            self.publish_text(run, value)?;
        }
        if !deliverable.commit_revision().is_empty() {
            self.publish_text(run, deliverable.commit_revision())?;
        }
        if !deliverable.export_path().is_empty() {
            self.publish_text(run, deliverable.export_path())?;
        }
        Ok(())
    }

    pub(super) fn read(
        &self,
        query: ProductArtifactQuery,
        maximum_chunk_bytes: usize,
    ) -> Result<ProductArtifactPage, ProductRunServiceError> {
        if !self
            .authorized
            .lock()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .get(&query.run_id())
            .is_some_and(|digests| digests.contains(&query.source().digest()))
        {
            return Err(ProductRunServiceError::NotFound);
        }
        if query.offset() == query.source().bytes() {
            return ProductArtifactPage::new(query, Vec::new(), None)
                .map_err(|_| ProductRunServiceError::InvalidMessage);
        }
        let reader = self.reader(query.source())?;
        let mut reader = reader.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        let maximum = maximum_chunk_bytes.min(MAX_PRODUCT_ARTIFACT_CHUNK_BYTES);
        if maximum == 0 {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let chunk = reader
            .read_chunk_at(query.offset(), maximum)
            .map_err(|error| {
                ProductRunServiceError::internal("read product artifact", error.to_string())
            })?
            .ok_or(ProductRunServiceError::Unavailable)?;
        let bytes = chunk.bytes().to_vec();
        let end = bytes.len();
        let end_offset = query
            .offset()
            .checked_add(u64::try_from(end).map_err(|_| ProductRunServiceError::Unavailable)?)
            .ok_or(ProductRunServiceError::Unavailable)?;
        let next = (end_offset < query.source().bytes()).then_some(end_offset);
        ProductArtifactPage::new(query, bytes, next)
            .map_err(|_| ProductRunServiceError::InvalidMessage)
    }

    pub(super) fn deliverable_index_page(
        &self,
        record: &RunRecord,
        query: ProductDeliverableIndexQuery,
    ) -> Result<ProductDeliverableIndexPage, ProductRunServiceError> {
        if query.run_id() != record.request.run_id() {
            return Err(ProductRunServiceError::NotFound);
        }
        let deliverable = record
            .snapshot
            .deliverable()
            .ok_or(ProductRunServiceError::NotFound)?;
        let reference = deliverable
            .reference()
            .map_err(|_| ProductRunServiceError::InvalidState)?;
        let (expected, values) = match query.kind() {
            ProductDeliverableIndexKind::ChangedPaths => {
                (reference.changed_paths(), deliverable.changed_paths())
            }
            ProductDeliverableIndexKind::SuccessfulCommands => {
                (reference.successful_commands(), deliverable.successful_commands())
            }
        };
        if query.index() != expected {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let start = query
            .after()
            .map_or(0, |after| after.saturating_add(1));
        let start = usize::try_from(start).map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let end = start.saturating_add(MAX_PRODUCT_DELIVERABLE_INDEX_PAGE).min(values.len());
        let entries = values[start..end]
            .iter()
            .enumerate()
            .map(|(offset, value)| {
                let value = self.publish_text(query.run_id(), value)?;
                let ordinal = u64::try_from(start.saturating_add(offset))
                    .map_err(|_| ProductRunServiceError::Unavailable)?;
                Ok(ProductDeliverableIndexEntry::new(ordinal, value))
            })
            .collect::<Result<Vec<_>, ProductRunServiceError>>()?;
        let next = (end < values.len())
            .then(|| entries.last().map(|entry| entry.ordinal()))
            .flatten();
        ProductDeliverableIndexPage::new(query, entries, next)
            .map_err(|_| ProductRunServiceError::InvalidState)
    }

    fn publish_optional_text(
        &self,
        run: RunId,
        text: &str,
    ) -> Result<Option<ProductArtifactReference>, ProductRunServiceError> {
        (!text.is_empty()).then(|| self.publish_text(run, text)).transpose()
    }

    pub(super) fn publish_text(
        &self,
        run: RunId,
        text: &str,
    ) -> Result<ProductArtifactReference, ProductRunServiceError> {
        let reference = ProductArtifactReference::measure(text)
            .map_err(|_| ProductRunServiceError::InvalidMessage)?;
        let digest = ArtifactDigest::from_sha256(reference.digest());
        let store = self.writer.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        let request = WriteRequest::new(
            digest,
            reference.bytes(),
            reference.bytes(),
            MediaType::new("text/plain;charset=utf-8")
                .map_err(|_| ProductRunServiceError::InvalidState)?,
            EncryptionMetadata::unencrypted(),
            creating_event(run, reference.digest())?,
        );
        let mut writer = store.begin_write(request).map_err(|error| {
            ProductRunServiceError::internal("prepare product artifact", error.to_string())
        })?;
        for chunk in text.as_bytes().chunks(MAX_PRODUCT_ARTIFACT_CHUNK_BYTES) {
            writer.write_chunk(chunk).map_err(|error| {
                ProductRunServiceError::internal("write product artifact", error.to_string())
            })?;
        }
        let finalized = writer.finalize().map_err(|error| {
            ProductRunServiceError::internal("publish product artifact", error.to_string())
        })?;
        if finalized.digest() != digest || finalized.size() != reference.bytes() {
            return Err(ProductRunServiceError::InvalidState);
        }
        store.add_reference(reference_owner(run), digest).map_err(|error| {
            ProductRunServiceError::internal("retain product artifact", error.to_string())
        })?;
        drop(store);
        self.authorized
            .lock()
            .map_err(|_| ProductRunServiceError::Unavailable)?
            .entry(run)
            .or_default()
            .insert(reference.digest());
        Ok(reference)
    }

    fn reader(
        &self,
        reference: ProductArtifactReference,
    ) -> Result<Arc<Mutex<ArtifactReadHandle>>, ProductRunServiceError> {
        let digest = reference.digest();
        {
            let mut readers = self.readers.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
            if let Some(reader) = readers.readers.get(&digest).cloned() {
                readers.recent.retain(|candidate| *candidate != digest);
                readers.recent.push_back(digest);
                drop(readers);
                validate_reader(&reader, reference)?;
                return Ok(reader);
            }
        }
        let reader = ArtifactStore::open_existing(
            &self.config,
            ArtifactDigest::from_sha256(digest),
        )
        .map_err(|error| ProductRunServiceError::internal("open product artifact", error.to_string()))?;
        let candidate = Arc::new(Mutex::new(reader));
        validate_reader(&candidate, reference)?;
        let mut readers = self.readers.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        let selected = if let Some(reader) = readers.readers.get(&digest).cloned() {
            reader
        } else {
            if readers.readers.len() >= MAX_CACHED_READERS
                && let Some(evicted) = readers.recent.pop_front()
            {
                readers.readers.remove(&evicted);
            }
            readers.readers.insert(digest, candidate.clone());
            candidate
        };
        readers.recent.retain(|candidate| *candidate != digest);
        readers.recent.push_back(digest);
        drop(readers);
        validate_reader(&selected, reference)?;
        Ok(selected)
    }
}

fn validate_reader(
    reader: &Arc<Mutex<ArtifactReadHandle>>,
    reference: ProductArtifactReference,
) -> Result<(), ProductRunServiceError> {
    let reader = reader.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
    if reader.metadata().digest() != ArtifactDigest::from_sha256(reference.digest())
        || reader.metadata().size() != reference.bytes()
    {
        return Err(ProductRunServiceError::InvalidState);
    }
    Ok(())
}

fn reference_owner(run: RunId) -> ReferenceOwner {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.product-artifact-owner.v1\0");
    hasher.update(run.as_bytes());
    ReferenceOwner::journal(Sha256Digest::new(hasher.finalize().into()))
}

fn creating_event(run: RunId, digest: Sha256Digest) -> Result<EventId, ProductRunServiceError> {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.product-artifact-event.v1\0");
    hasher.update(run.as_bytes());
    hasher.update(digest.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[0] |= 1;
    EventId::new(bytes).map_err(|_| ProductRunServiceError::InvalidState)
}

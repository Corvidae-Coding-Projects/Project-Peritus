//! Run-owned immutable product-finding bodies and bounded authenticated range reads.

use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::{Arc, Mutex},
};

use peritus_artifact_store::{
    ArtifactDigest, ArtifactReadHandle, ArtifactStore, EncryptionMetadata, MediaType,
    ReferenceOwner, StoreConfig, WriteRequest,
};
use peritus_product_runner::{
    ContextSource, ContextSourceSlice, MAX_CONTEXT_SOURCE_SLICE_BYTES, ProductRunner,
};
use peritus_review::{
    ProductFinding, ProductFindingBodyPublisher, ProductFindingBodyReference, ProductFindingLedger,
    ProductReviewError, ProductReviewSummaryReference,
};
use peritus_types::{EventId, RunId, Sha256Digest};
use sha2::{Digest as _, Sha256};

use super::super::{
    FindingSourceCatalog, ProductRunServiceError, ReviewArtifactReference, RunRecord,
};

const MAX_CACHED_FINDING_READERS: usize = 32;

pub(super) struct FindingBodyStore {
    config: StoreConfig,
    writer: Mutex<ArtifactStore>,
    readers: Mutex<ReaderCache>,
}

#[derive(Default)]
struct ReaderCache {
    readers: BTreeMap<Sha256Digest, Arc<Mutex<ArtifactReadHandle>>>,
    recent: VecDeque<Sha256Digest>,
}

pub(super) struct FindingSourcePage {
    pub(super) sources: Vec<ContextSource>,
    pub(super) has_more: bool,
}

struct RunPublisher<'a> {
    store: &'a FindingBodyStore,
    run: RunId,
}

impl FindingBodyStore {
    pub(super) fn open(root: &Path) -> Result<Self, peritus_artifact_store::ArtifactStoreError> {
        let config = StoreConfig::for_available_space_without_artifact_limit(root)?;
        let writer = ArtifactStore::open(config.clone())?;
        Ok(Self {
            config,
            writer: Mutex::new(writer),
            readers: Mutex::new(ReaderCache::default()),
        })
    }

    pub(super) fn publish(
        &self,
        run: RunId,
        finding: &ProductFinding,
        source_ordinal: u64,
    ) -> Result<ProductFindingBodyReference, ProductReviewError> {
        let reference = finding.canonical_body_reference(source_ordinal)?;
        let digest = ArtifactDigest::from_sha256(reference.digest());
        let event = creating_event(run, reference.digest())?;
        let media_type = MediaType::new("text/plain;charset=utf-8").map_err(|_| {
            ProductReviewError::new("finding body media type is unavailable")
        })?;
        let store = self.writer.lock().map_err(|_| {
            ProductReviewError::new("finding body artifact store lock was poisoned")
        })?;
        let request = WriteRequest::new(
            digest,
            reference.bytes(),
            reference.bytes(),
            media_type,
            EncryptionMetadata::unencrypted(),
            event,
        );
        let mut writer = store.begin_write(request).map_err(|_| {
            ProductReviewError::new("finding body artifact writer is unavailable")
        })?;
        finding.stream_canonical_body(&mut |chunk| {
            writer.write_chunk(chunk).map_err(|_| {
                ProductReviewError::new("finding body artifact write failed")
            })
        })?;
        let finalized = writer.finalize().map_err(|_| {
            ProductReviewError::new("finding body artifact publication failed")
        })?;
        if finalized.digest() != digest || finalized.size() != reference.bytes() {
            return Err(ProductReviewError::new(
                "finding body artifact publication changed its identity",
            ));
        }
        store
            .add_reference(reference_owner(run), digest)
            .map_err(|_| {
                ProductReviewError::new("finding body durable reference publication failed")
            })?;
        Ok(reference)
    }

    pub(super) fn publish_summary(
        &self,
        run: RunId,
        summary: &str,
        review_cycle: u32,
        source_ordinal: u64,
    ) -> Result<ProductReviewSummaryReference, ProductReviewError> {
        let reference =
            ProductReviewSummaryReference::measure(review_cycle, summary, source_ordinal)?;
        let digest = ArtifactDigest::from_sha256(reference.digest());
        let event = creating_event(run, reference.digest())?;
        let media_type = MediaType::new("text/plain;charset=utf-8").map_err(|_| {
            ProductReviewError::new("review summary media type is unavailable")
        })?;
        let store = self.writer.lock().map_err(|_| {
            ProductReviewError::new("review summary artifact store lock was poisoned")
        })?;
        let request = WriteRequest::new(
            digest,
            reference.bytes(),
            reference.bytes(),
            media_type,
            EncryptionMetadata::unencrypted(),
            event,
        );
        let mut writer = store.begin_write(request).map_err(|_| {
            ProductReviewError::new("review summary artifact writer is unavailable")
        })?;
        ProductReviewSummaryReference::stream_canonical(
            review_cycle,
            summary,
            &mut |chunk| {
                writer.write_chunk(chunk).map_err(|_| {
                    ProductReviewError::new("review summary artifact write failed")
                })
            },
        )?;
        let finalized = writer.finalize().map_err(|_| {
            ProductReviewError::new("review summary artifact publication failed")
        })?;
        if finalized.digest() != digest || finalized.size() != reference.bytes() {
            return Err(ProductReviewError::new(
                "review summary artifact publication changed its identity",
            ));
        }
        store.add_reference(reference_owner(run), digest).map_err(|_| {
            ProductReviewError::new("review summary durable reference publication failed")
        })?;
        Ok(reference)
    }

    pub(super) fn migrate_record(
        &self,
        record: &mut RunRecord,
    ) -> Result<bool, ProductRunServiceError> {
        if record.opaque_resume.is_some() {
            return Err(ProductRunServiceError::internal(
                "migrate product finding bodies",
                "the retained continuation must decode before its finding bodies can migrate",
            ));
        }
        let run = record.request.run_id();
        let publisher = RunPublisher { store: self, run };
        let (finding_state, mut changed) = ProductRunner::externalize_finding_state(
            &record.finding_state,
            &publisher,
        )
        .map_err(|error| {
            ProductRunServiceError::internal(
                "migrate product finding bodies",
                error.to_string(),
            )
        })?;
        if let Some(resume) = &mut record.resume {
            changed |= resume.externalize_finding_bodies(&publisher).map_err(|error| {
                ProductRunServiceError::internal(
                    "migrate retained product finding bodies",
                    error.to_string(),
                )
            })?;
            if !resume.review_artifacts_externalized() {
                return Err(ProductRunServiceError::internal(
                    "migrate retained product finding bodies",
                    "the retained continuation still contains inline review artifacts",
                ));
            }
        }
        let finding_catalog = Self::published_catalog(&finding_state)?;
        record.finding_state = finding_state;
        record.finding_catalog = finding_catalog;
        Ok(changed)
    }

    pub(super) fn catalog(
        finding_state: &str,
    ) -> Result<Arc<FindingSourceCatalog>, ProductRunServiceError> {
        let ledger = ProductRunner::decode_finding_state(finding_state).map_err(|error| {
            ProductRunServiceError::internal(
                "decode product finding source catalog",
                error.to_string(),
            )
        })?;
        Self::catalog_from_ledger(finding_state, &ledger)
    }

    pub(super) fn published_catalog(
        finding_state: &str,
    ) -> Result<Arc<FindingSourceCatalog>, ProductRunServiceError> {
        let ledger = ProductRunner::decode_finding_state(finding_state).map_err(|error| {
            ProductRunServiceError::internal(
                "decode published product finding source catalog",
                error.to_string(),
            )
        })?;
        if ledger.has_inline_bodies() || ledger.has_inline_summary() {
            return Err(ProductRunServiceError::internal(
                "decode published product finding source catalog",
                "a governed finding ledger still contains inline review artifacts",
            ));
        }
        Self::catalog_from_ledger(finding_state, &ledger)
    }

    pub(super) fn catalog_from_ledger(
        finding_state: &str,
        ledger: &ProductFindingLedger,
    ) -> Result<Arc<FindingSourceCatalog>, ProductRunServiceError> {
        let mut entries = Vec::new();
        if let Some(reference) = ledger.review_summary_reference() {
            entries.push(catalog_entry(
                "Product review summary".to_owned(),
                reference.source_ordinal(),
                reference.digest(),
                reference.bytes(),
            )?);
        }
        for reference in ledger.findings().filter_map(ProductFinding::body_reference) {
            let identity = reference.finding_id().as_bytes();
            let label = format!(
                "Product finding {}",
                identity[..8].iter().fold(String::new(), |mut value, byte| {
                    use core::fmt::Write as _;
                    let _ = write!(value, "{byte:02x}");
                    value
                }),
            );
            entries.push(catalog_entry(
                label,
                reference.source_ordinal(),
                reference.digest(),
                reference.bytes(),
            )?);
        }
        entries.sort_by_key(|(source, _)| source.ordinal());
        if entries.windows(2).any(|pair| pair[0].0.ordinal() == pair[1].0.ordinal()) {
            return Err(ProductRunServiceError::internal(
                "construct product finding source catalog",
                "review artifact source ordinals are not unique",
            ));
        }
        Ok(Arc::new(FindingSourceCatalog {
            head_digest: peritus_codec::sha256(finding_state.as_bytes()),
            review_artifacts_externalized: !ledger.has_inline_bodies()
                && !ledger.has_inline_summary(),
            entries,
        }))
    }

    pub(super) fn page(
        &self,
        catalog: &FindingSourceCatalog,
        after: Option<u64>,
        maximum: usize,
    ) -> Result<FindingSourcePage, ProductRunServiceError> {
        let start = catalog.entries.partition_point(|(source, _)| {
            after.is_some_and(|after| source.ordinal() <= after)
        });
        let remaining = &catalog.entries[start..];
        let count = remaining.len().min(maximum);
        Ok(FindingSourcePage {
            sources: remaining[..count].iter().map(|(source, _)| source.clone()).collect(),
            has_more: remaining.len() > count,
        })
    }

    pub(super) fn read(
        &self,
        catalog: &FindingSourceCatalog,
        source: u64,
        offset: u64,
    ) -> Result<ContextSourceSlice, ProductRunServiceError> {
        let (descriptor, reference) = catalog
            .entries
            .binary_search_by_key(&source, |(descriptor, _)| descriptor.ordinal())
            .ok()
            .and_then(|index| catalog.entries.get(index).cloned())
            .ok_or(ProductRunServiceError::NotFound)?;
        if descriptor.digest() != reference.digest.as_bytes()
            || descriptor.bytes() != reference.bytes
            || descriptor.ordinal() != reference.source_ordinal
            || offset >= descriptor.bytes()
        {
            return Err(ProductRunServiceError::InvalidMessage);
        }
        let reader = self.reader(reference.digest, reference.bytes)?;
        read_slice(&descriptor, offset, reader)
    }

    fn reader(
        &self,
        digest: Sha256Digest,
        bytes: u64,
    ) -> Result<Arc<Mutex<ArtifactReadHandle>>, ProductRunServiceError> {
        {
            let mut readers = self.readers.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
            if let Some(reader) = readers.readers.get(&digest).cloned() {
                readers.recent.retain(|candidate| *candidate != digest);
                readers.recent.push_back(digest);
                drop(readers);
                validate_reader(&reader, digest, bytes)?;
                return Ok(reader);
            }
        }
        let reader = ArtifactStore::open_existing(
            &self.config,
            ArtifactDigest::from_sha256(digest),
        )
        .map_err(|error| {
            ProductRunServiceError::internal("open product finding body", error.to_string())
        })?;
        let candidate = Arc::new(Mutex::new(reader));
        validate_reader(&candidate, digest, bytes)?;
        let mut readers = self.readers.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
        let selected = if let Some(reader) = readers.readers.get(&digest).cloned() {
            reader
        } else {
            if readers.readers.len() >= MAX_CACHED_FINDING_READERS
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
        validate_reader(&selected, digest, bytes)?;
        Ok(selected)
    }
}

fn validate_reader(
    reader: &Arc<Mutex<ArtifactReadHandle>>,
    digest: Sha256Digest,
    bytes: u64,
) -> Result<(), ProductRunServiceError> {
    let reader = reader.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
    if reader.metadata().digest() != ArtifactDigest::from_sha256(digest)
        || reader.metadata().size() != bytes
    {
        return Err(ProductRunServiceError::internal(
            "open product finding body",
            "artifact metadata differs from the durable finding descriptor",
        ));
    }
    Ok(())
}

impl ProductFindingBodyPublisher for RunPublisher<'_> {
    fn publish(
        &self,
        finding: &ProductFinding,
        source_ordinal: u64,
    ) -> Result<ProductFindingBodyReference, ProductReviewError> {
        self.store.publish(self.run, finding, source_ordinal)
    }

    fn publish_summary(
        &self,
        summary: &str,
        review_cycle: u32,
        source_ordinal: u64,
    ) -> Result<ProductReviewSummaryReference, ProductReviewError> {
        self.store
            .publish_summary(self.run, summary, review_cycle, source_ordinal)
    }
}

fn catalog_entry(
    label: String,
    source_ordinal: u64,
    digest: Sha256Digest,
    bytes: u64,
) -> Result<(ContextSource, ReviewArtifactReference), ProductRunServiceError> {
    let descriptor = ContextSource::new(source_ordinal, label, digest.into_bytes(), bytes)
        .map_err(|error| {
            ProductRunServiceError::internal(
                "construct product finding source descriptor",
                error,
            )
        })?;
    Ok((descriptor, ReviewArtifactReference { digest, bytes, source_ordinal }))
}

fn read_slice(
    descriptor: &ContextSource,
    offset: u64,
    reader: Arc<Mutex<ArtifactReadHandle>>,
) -> Result<ContextSourceSlice, ProductRunServiceError> {
    let mut reader = reader.lock().map_err(|_| ProductRunServiceError::Unavailable)?;
    let maximum = MAX_CONTEXT_SOURCE_SLICE_BYTES
        .checked_add(3)
        .ok_or(ProductRunServiceError::Unavailable)?;
    let chunk = reader
        .read_chunk_at(offset, maximum)
        .map_err(|error| {
            ProductRunServiceError::internal("read product finding body", error.to_string())
        })?
        .ok_or(ProductRunServiceError::Unavailable)?;
    let mut end = chunk.bytes().len().min(MAX_CONTEXT_SOURCE_SLICE_BYTES);
    let text = loop {
        match std::str::from_utf8(&chunk.bytes()[..end]) {
            Ok(text) => break text,
            Err(error) if error.error_len().is_none() && end > 0 => end -= 1,
            Err(_) => return Err(ProductRunServiceError::InvalidMessage),
        }
    };
    if text.is_empty() {
        return Err(ProductRunServiceError::InvalidMessage);
    }
    let end_offset = offset
        .checked_add(u64::try_from(end).map_err(|_| ProductRunServiceError::Unavailable)?)
        .ok_or(ProductRunServiceError::Unavailable)?;
    let next = (end_offset < descriptor.bytes()).then_some(end_offset);
    ContextSourceSlice::new(descriptor, offset, text.to_owned(), next).map_err(|error| {
        ProductRunServiceError::internal("page product finding body", error)
    })
}

fn reference_owner(run: RunId) -> ReferenceOwner {
    let mut digest = Sha256::new();
    digest.update(b"peritus.product-finding-body-owner.v1\0");
    digest.update(run.as_bytes());
    ReferenceOwner::journal(Sha256Digest::new(digest.finalize().into()))
}

fn creating_event(
    run: RunId,
    digest: Sha256Digest,
) -> Result<EventId, ProductReviewError> {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.product-finding-body-event.v1\0");
    hasher.update(run.as_bytes());
    hasher.update(digest.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[0] |= 1;
    EventId::new(bytes).map_err(|_| {
        ProductReviewError::new("finding body event identity is invalid")
    })
}

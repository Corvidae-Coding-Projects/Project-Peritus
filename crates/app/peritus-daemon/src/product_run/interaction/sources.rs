//! Admitted typed conversation-source paging, distinct from external context evidence.

use super::{ProductRunService, ProductRunServiceError};
use crate::product_control::{
    CapturedConversation, CapturedFileReaders, RequestSourceBody, RequestSourceSnapshot,
};
use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_product_runner::{
    ContextSource, ContextSourceSlice, MAX_CONTEXT_SOURCE_SLICE_BYTES,
};

const MAX_CACHED_REQUEST_READERS: usize = 32;

impl ProductRunService {
    pub(super) fn read_captured_file_source(
        &self,
        captured: &CapturedConversation,
        readers: &CapturedFileReaders,
        source: u64,
        offset: u64,
    ) -> Result<ContextSourceSlice, ProductRunServiceError> {
        let (descriptor, version) = captured.context_source(source)?;
        if version.external_artifact() {
            return self.read_external_file_source(&descriptor, offset);
        }
        self.with_control_conversation(captured.conversation(), |store| {
            store.read_context_source(captured, readers, source, offset)
        })
        .map_err(Into::into)
    }

    pub(super) fn read_request_source(
        &self,
        snapshot: &RequestSourceSnapshot,
        source: u64,
        offset: u64,
    ) -> Result<ContextSourceSlice, ProductRunServiceError> {
        let body = snapshot.body(source).map_err(ProductRunServiceError::from)?;
        match &body {
            RequestSourceBody::InlineUser { .. } => body.read_inline(offset).map_err(Into::into),
            RequestSourceBody::ArtifactUser { descriptor } => {
                self.read_request_artifact(descriptor, offset)
            }
            RequestSourceBody::Assistant { descriptor, reference } => {
                if reference.digest().as_bytes() != descriptor.digest()
                    || reference.bytes() != descriptor.bytes()
                {
                    return Err(ProductRunServiceError::internal(
                        "read assistant history",
                        "public reply reference differs from its admitted descriptor",
                    ));
                }
                self.read_reply_artifact(descriptor, reference, offset)
            }
        }
    }

    fn read_request_artifact(
        &self,
        descriptor: &ContextSource,
        offset: u64,
    ) -> Result<ContextSourceSlice, ProductRunServiceError> {
        if offset >= descriptor.bytes() {
            return Err(ProductRunServiceError::Control(
                peritus_product_runner::control::ControlError::InvalidInput,
            ));
        }
        let reader = self.request_source_reader(descriptor)?;
        self.read_artifact_slice(descriptor, offset, reader, "authoritative request source")
    }

    pub(super) fn read_external_file_source(
        &self,
        descriptor: &ContextSource,
        offset: u64,
    ) -> Result<ContextSourceSlice, ProductRunServiceError> {
        if descriptor.bytes() == 0 {
            return ContextSourceSlice::new(descriptor, offset, String::new(), None)
                .map_err(|error| {
                    ProductRunServiceError::internal("read external file source", error)
                });
        }
        if offset >= descriptor.bytes() {
            return Err(ProductRunServiceError::Control(
                peritus_product_runner::control::ControlError::InvalidInput,
            ));
        }
        let reader = self.request_source_reader(descriptor)?;
        self.read_artifact_slice(descriptor, offset, reader, "external file source")
    }

    fn read_reply_artifact(
        &self,
        descriptor: &ContextSource,
        reference: &peritus_product_runner::control::PublicReplyReference,
        offset: u64,
    ) -> Result<ContextSourceSlice, ProductRunServiceError> {
        if offset >= descriptor.bytes() {
            return Err(ProductRunServiceError::Control(
                peritus_product_runner::control::ControlError::InvalidInput,
            ));
        }
        let reader = self.public_reply_reader(reference)?;
        self.read_artifact_slice(descriptor, offset, reader, "assistant history")
    }

    pub(super) fn ensure_public_reply_artifact(
        &self,
        reference: &peritus_product_runner::control::PublicReplyReference,
    ) -> Result<(), ProductRunServiceError> {
        self.public_reply_reader(reference).map(drop)
    }

    fn public_reply_reader(
        &self,
        reference: &peritus_product_runner::control::PublicReplyReference,
    ) -> Result<
        std::sync::Arc<std::sync::Mutex<peritus_artifact_store::ArtifactReadHandle>>,
        ProductRunServiceError,
    > {
        let owner_retained = {
            let retained = self.inner.retained_reply_owners.lock().map_err(|_| {
                ProductRunServiceError::internal(
                    "retain assistant history",
                    "public-reply owner cache lock was poisoned",
                )
            })?;
            retained.contains(&reference.operation())
        };
        if owner_retained {
            let digest = reference.digest();
            let mut readers = self.inner.reply_readers.lock().map_err(|_| {
                ProductRunServiceError::internal(
                    "read assistant history",
                    "public-reply reader cache lock was poisoned",
                )
            })?;
            if let Some(reader) = readers.readers.get(&digest).cloned() {
                readers.recent.retain(|candidate| *candidate != digest);
                readers.recent.push_back(digest);
                return Ok(reader);
            }
        }
        let legacy = if owner_retained {
            None
        } else {
            self.with_control_index_read(|store| store.legacy_reply_bytes(reference))
                .map_err(ProductRunServiceError::from)?
        };
        let reader = crate::product_control::open_public_reply_artifact(
            &self.inner.reply_artifacts,
            reference,
            legacy.as_deref(),
        )
        .map_err(ProductRunServiceError::from)?;
        if reader.metadata().size() != reference.bytes()
            || reader.metadata().digest()
                != ArtifactDigest::from_sha256(reference.digest())
        {
            return Err(ProductRunServiceError::internal(
                "retain assistant history",
                "public-reply artifact metadata differs from its control reference",
            ));
        }
        let digest = reference.digest();
        let candidate = std::sync::Arc::new(std::sync::Mutex::new(reader));
        let mut readers = self.inner.reply_readers.lock().map_err(|_| {
            ProductRunServiceError::internal(
                "retain assistant history",
                "public-reply reader cache lock was poisoned",
            )
        })?;
        let reader = if let Some(reader) = readers.readers.get(&digest).cloned() {
            reader
        } else {
            if readers.readers.len() >= MAX_CACHED_REQUEST_READERS
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
        if !owner_retained {
            self.inner
                .retained_reply_owners
                .lock()
                .map_err(|_| {
                    ProductRunServiceError::internal(
                        "retain assistant history",
                        "public-reply owner cache lock was poisoned",
                    )
                })?
                .insert(reference.operation());
        }
        Ok(reader)
    }

    fn read_artifact_slice(
        &self,
        descriptor: &ContextSource,
        offset: u64,
        reader: std::sync::Arc<
            std::sync::Mutex<peritus_artifact_store::ArtifactReadHandle>,
        >,
        subject: &'static str,
    ) -> Result<ContextSourceSlice, ProductRunServiceError> {
        let mut reader = reader.lock().map_err(|_| {
            ProductRunServiceError::internal(
                "read conversation source",
                format!("{subject} artifact reader lock was poisoned"),
            )
        })?;
        let digest = ArtifactDigest::from_sha256(peritus_types::Sha256Digest::new(
            *descriptor.digest(),
        ));
        if reader.metadata().size() != descriptor.bytes()
            || reader.metadata().digest() != digest
        {
            return Err(ProductRunServiceError::internal(
                "read conversation source",
                format!("{subject} artifact metadata differs from its control binding"),
            ));
        }
        let maximum = MAX_CONTEXT_SOURCE_SLICE_BYTES
            .checked_add(3)
            .ok_or(ProductRunServiceError::Unavailable)?;
        let chunk = reader
            .read_chunk_at(offset, maximum)
            .map_err(|error| {
                ProductRunServiceError::internal(
                    "read conversation source",
                    error.to_string(),
                )
            })?
            .ok_or(ProductRunServiceError::Unavailable)?;
        let mut end = chunk.bytes().len().min(MAX_CONTEXT_SOURCE_SLICE_BYTES);
        let text = loop {
            match std::str::from_utf8(&chunk.bytes()[..end]) {
                Ok(text) => break text,
                Err(error) if error.error_len().is_none() && end > 0 => end -= 1,
                Err(_) => {
                    return Err(ProductRunServiceError::Control(
                        peritus_product_runner::control::ControlError::InvalidInput,
                    ));
                }
            }
        };
        if text.is_empty() {
            return Err(ProductRunServiceError::Control(
                peritus_product_runner::control::ControlError::InvalidInput,
            ));
        }
        let end_offset = offset
            .checked_add(u64::try_from(end).map_err(|_| ProductRunServiceError::Unavailable)?)
            .ok_or(ProductRunServiceError::Unavailable)?;
        let next = (end_offset < descriptor.bytes()).then_some(end_offset);
        ContextSourceSlice::new(descriptor, offset, text.to_owned(), next)
            .map_err(|error| ProductRunServiceError::internal("page conversation source", error))
    }

    fn request_source_reader(
        &self,
        descriptor: &ContextSource,
    ) -> Result<
        std::sync::Arc<std::sync::Mutex<peritus_artifact_store::ArtifactReadHandle>>,
        ProductRunServiceError,
    > {
        let digest = peritus_types::Sha256Digest::new(*descriptor.digest());
        {
            let mut readers = self.inner.request_source_readers.lock().map_err(|_| {
                ProductRunServiceError::internal(
                    "read authoritative request source",
                    "request-source reader cache lock was poisoned",
                )
            })?;
            if let Some(reader) = readers.readers.get(&digest).cloned() {
                readers.recent.retain(|candidate| *candidate != digest);
                readers.recent.push_back(digest);
                return Ok(reader);
            }
        }

        let reader = ArtifactStore::open_existing(
            &self.inner.request_source_artifacts,
            ArtifactDigest::from_sha256(digest),
        )
        .map_err(|error| {
            ProductRunServiceError::internal(
                "open authoritative request source",
                error.to_string(),
            )
        })?;
        if reader.metadata().size() != descriptor.bytes()
            || reader.metadata().digest() != ArtifactDigest::from_sha256(digest)
        {
            return Err(ProductRunServiceError::internal(
                "open authoritative request source",
                "artifact metadata differs from its control binding",
            ));
        }
        let candidate = std::sync::Arc::new(std::sync::Mutex::new(reader));
        let mut readers = self.inner.request_source_readers.lock().map_err(|_| {
            ProductRunServiceError::internal(
                "read authoritative request source",
                "request-source reader cache lock was poisoned",
            )
        })?;
        let selected = if let Some(existing) = readers.readers.get(&digest).cloned() {
            existing
        } else {
            if readers.readers.len() >= MAX_CACHED_REQUEST_READERS
                && let Some(evicted) = readers.recent.pop_front()
            {
                readers.readers.remove(&evicted);
            }
            readers.readers.insert(digest, candidate.clone());
            candidate
        };
        readers.recent.retain(|candidate| *candidate != digest);
        readers.recent.push_back(digest);
        Ok(selected)
    }
}

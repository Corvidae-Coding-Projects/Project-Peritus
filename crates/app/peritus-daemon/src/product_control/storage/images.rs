//! Descriptor-only image publication with exact immutable artifact retention.

use super::{
    ControlError, ControlGeneration, ControlOperation, ControlReceipt, ControlStore, Error,
    IMAGE_DESCRIPTOR_NAMESPACE,
};
use peritus_artifact_store::{
    ArtifactDigest, ArtifactReadHandle, ArtifactStore, EncryptionMetadata, MediaType,
    ReferenceOwner, StoreConfig, WriteRequest,
};
use peritus_journal::StateInstall;
use peritus_model_protocol::{MediaInput, MediaKind, MediaType as ProtocolMediaType};
use peritus_product_runner::{
    attachment::{ImageValidation, ValidatedImage},
    control::{ControlIntent, ImageAttachment, ImageFormat},
};
use peritus_types::{ArtifactId, EventId, Sha256Digest};

use super::checkpoints::snapshot::{
    PublicationPlan, PublicationPurpose, publication_claim,
};

const IMAGE_NAMESPACE: u16 = 3407;
const PREVIEW_NAMESPACE: u16 = 3408;
const IMAGE_DESCRIPTOR_MAGIC_V1: &[u8; 8] = b"pcimage1";
const IMAGE_DESCRIPTOR_MAGIC_V2: &[u8; 8] = b"pcimage2";

/// Finalized image bytes and preview proof without marker, reference, or C0 mutation.
pub(crate) struct PreparedImagePublication {
    operation: ControlOperation,
    descriptor: Vec<u8>,
    preview: Option<Vec<u8>>,
    publication: PublicationPlan,
}

impl ControlGeneration {
    pub(crate) fn prepare_image(
        &self,
        operation: &ControlOperation,
        validated: &ValidatedImage,
    ) -> Result<PreparedImagePublication, Error> {
        self.prepare_image_archived(operation, validated, None, None)
    }

    pub(crate) fn prepare_previewed_image(
        &self,
        operation: &ControlOperation,
        validated: &ValidatedImage,
        preview: Vec<u8>,
        source_artifacts: &StoreConfig,
    ) -> Result<PreparedImagePublication, Error> {
        self.prepare_image_archived(
            operation,
            validated,
            Some(preview),
            Some(source_artifacts),
        )
    }

    fn prepare_image_archived(
        &self,
        operation: &ControlOperation,
        validated: &ValidatedImage,
        preview: Option<Vec<u8>>,
        source_artifacts: Option<&StoreConfig>,
    ) -> Result<PreparedImagePublication, Error> {
        let ControlIntent::AttachImage { image, .. } = operation.intent() else {
            return Err(ControlError::InvalidInput.into());
        };
        if image.operation() != operation.id() || !image.matches(validated) {
            return Err(ControlError::InvalidInput.into());
        }
        match (image.preview_digest(), &preview) {
            (Some(digest), Some(bytes))
                if !bytes.is_empty()
                    && bytes.len() <= 16 * 1024
                    && peritus_codec::sha256(bytes) == digest => {}
            (None, None) => {}
            _ => return Err(ControlError::InvalidInput.into()),
        }
        let store = ArtifactStore::open(self.reply_artifact_config().clone())
            .map_err(artifact_error)?;
        let artifact =
            spool_image_artifact(&store, image, validated, source_artifacts)?;
        let descriptor = descriptor_bytes(image);
        let mut immutable_root = descriptor.clone();
        if let Some(preview) = &preview {
            immutable_root.extend_from_slice(preview);
        }
        let claim = publication_claim(
            operation,
            IMAGE_DESCRIPTOR_NAMESPACE,
            *operation.id().as_bytes(),
            &immutable_root,
            PublicationPurpose::ImageDescriptor,
        )?;
        let publication = PublicationPlan::new(claim, descriptor_owner(image), vec![artifact])?;
        Ok(PreparedImagePublication {
            operation: operation.clone(),
            descriptor,
            preview,
            publication,
        })
    }
}

impl ControlStore {
    /// Imports already authorized and inspected bytes atomically with their conversation input.
    /// Compatibility callers may supply in-memory media; workbench production uses an exact
    /// durable source artifact through accept_previewed_image.
    pub fn accept_image(
        &mut self,
        operation: &ControlOperation,
        validated: &ValidatedImage,
    ) -> Result<ControlReceipt, Error> {
        let prepared = self.generation().prepare_image(operation, validated)?;
        self.accept_prepared_image(prepared)
    }

    /// Publishes the exact confirmed preview beside an immutable image descriptor.
    pub fn accept_previewed_image(
        &mut self,
        operation: &ControlOperation,
        validated: &ValidatedImage,
        preview: Vec<u8>,
        source_artifacts: &StoreConfig,
    ) -> Result<ControlReceipt, Error> {
        let prepared = self.generation().prepare_previewed_image(
            operation,
            validated,
            preview,
            source_artifacts,
        )?;
        self.accept_prepared_image(prepared)
    }

    pub(crate) fn accept_prepared_image(
        &mut self,
        prepared: PreparedImagePublication,
    ) -> Result<ControlReceipt, Error> {
        if let Some(receipt) = self.resolve(&prepared.operation)? {
            return Ok(receipt);
        }
        let mut installs = vec![StateInstall::new(
            IMAGE_DESCRIPTOR_NAMESPACE,
            prepared.operation.id().as_bytes().to_vec(),
            None,
            1,
            prepared.descriptor,
        )?];
        if let Some(preview) = prepared.preview {
            installs.push(StateInstall::new(
                PREVIEW_NAMESPACE,
                prepared.operation.id().as_bytes().to_vec(),
                None,
                1,
                preview,
            )?);
        }
        let mut append = self.prepare_installs(&prepared.operation, installs)?;
        append.publications.add_plan(prepared.publication);
        self.commit_prepared(append).map(|committed| committed.into_receipt())
    }

    pub(super) fn verify_image_archive(
        &self,
        image: &ImageAttachment,
        position: u64,
    ) -> Result<(), Error> {
        if let Some(descriptor) = self
            .journal
            .state_record(IMAGE_DESCRIPTOR_NAMESPACE, image.operation().as_bytes())?
        {
            if descriptor.revision() != 1 || descriptor.producing_position() != position {
                return Err(Error::Corrupt("image descriptor was not published atomically"));
            }
            verify_descriptor(image, descriptor.bytes())?;
        } else {
            let artifact = self
                .journal
                .state_record(IMAGE_NAMESPACE, image.operation().as_bytes())?
                .ok_or(Error::Corrupt("image artifact missing"))?;
            if artifact.revision() != 1 || artifact.producing_position() != position {
                return Err(Error::Corrupt("image was not published with its input reference"));
            }
            verify_bytes(image, artifact.bytes())?;
        }
        if let Some(digest) = image.preview_digest() {
            let preview = self
                .journal
                .state_record(PREVIEW_NAMESPACE, image.operation().as_bytes())?
                .ok_or(Error::Corrupt("confirmed image preview missing"))?;
            if preview.revision() != 1
                || preview.producing_position() != position
                || preview.bytes().is_empty()
                || preview.bytes().len() > 16 * 1024
                || peritus_codec::sha256(preview.bytes()) != digest
            {
                return Err(Error::Corrupt(
                    "image preview differs from its atomic consent binding",
                ));
            }
        }
        Ok(())
    }

    // Called only after capture authenticates the owner and verifies immutable history.
    pub(in crate::product_control) fn image_media(
        &self,
        image: &ImageAttachment,
    ) -> Result<MediaInput, Error> {
        self.open_image_artifact(image).map(drop)?;
        Ok(MediaInput::artifact(
            MediaKind::Image,
            ProtocolMediaType::new(image.format().media_type().to_owned())
                .map_err(|_| Error::Corrupt("invalid image format"))?,
            image_artifact_id(image)?,
            image.digest(),
        ))
    }

    /// Resolves an already-admitted artifact reference for the selected provider only.
    pub(crate) fn materialize_image_media(
        &self,
        start: &ControlOperation,
        artifact_id: ArtifactId,
        digest: Sha256Digest,
        maximum_bytes: u64,
    ) -> Result<Vec<u8>, Error> {
        let record = self.execution_record(start)?;
        let image = record
            .images()
            .entries()
            .iter()
            .map(peritus_product_runner::control::ImageSelection::image)
            .find(|image| {
                image.artifact_bytes() == artifact_id.as_bytes() && image.digest() == digest
            })
            .ok_or(ControlError::ScopeMismatch)?;
        if maximum_bytes == 0 || image.bytes() > maximum_bytes {
            return Err(ControlError::Capacity.into());
        }
        let mut reader = self.open_image_artifact(image)?;
        let capacity = usize::try_from(image.bytes()).map_err(|_| ControlError::Capacity)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(capacity).map_err(|_| ControlError::Capacity)?;
        let mut offset = 0_u64;
        while let Some(chunk) =
            reader.read_chunk_at(offset, 64 * 1024).map_err(artifact_error)?
        {
            if chunk.bytes().is_empty() {
                return Err(Error::Corrupt("image artifact reader made no progress"));
            }
            let next = offset
                .checked_add(
                    u64::try_from(chunk.bytes().len()).map_err(|_| ControlError::Capacity)?,
                )
                .ok_or(ControlError::Capacity)?;
            if next > image.bytes() {
                return Err(Error::Corrupt(
                    "image artifact exceeded its immutable byte length",
                ));
            }
            bytes.extend_from_slice(chunk.bytes());
            offset = next;
        }
        verify_bytes(image, &bytes)?;
        Ok(bytes)
    }

    fn open_image_artifact(
        &self,
        image: &ImageAttachment,
    ) -> Result<ArtifactReadHandle, Error> {
        if let Some(descriptor) = self
            .journal
            .state_record(IMAGE_DESCRIPTOR_NAMESPACE, image.operation().as_bytes())?
        {
            verify_descriptor(image, descriptor.bytes())?;
            verify_retained_image(&self.checkpoint_artifacts, image)?;
            adopt_image_owner(&self.checkpoint_artifacts, image)?;
        } else {
            let legacy = self
                .journal
                .state_record(IMAGE_NAMESPACE, image.operation().as_bytes())?
                .ok_or(Error::Corrupt("selected image artifact missing"))?;
            verify_bytes(image, legacy.bytes())?;
            retain_legacy_image(&self.checkpoint_artifacts, image, legacy.bytes())?;
        }
        ArtifactStore::open_existing(
            &self.checkpoint_config,
            ArtifactDigest::from_sha256(image.digest()),
        )
        .map_err(artifact_error)
    }
}

fn spool_image_artifact(
    store: &ArtifactStore,
    image: &ImageAttachment,
    validated: &ValidatedImage,
    source_config: Option<&StoreConfig>,
) -> Result<ArtifactDigest, Error> {
    let digest = ArtifactDigest::from_sha256(image.digest());
    let existing = store.metadata(digest).map_err(artifact_error)?;
    if existing.as_ref().is_none_or(|metadata| !metadata.is_referenceable()) {
        let request = WriteRequest::new(
            digest,
            image.bytes(),
            image.bytes(),
            MediaType::new(image.format().media_type()).map_err(artifact_error)?,
            EncryptionMetadata::unencrypted(),
            EventId::new(*image.operation().as_bytes())
                .map_err(|_| Error::Corrupt("invalid image event identity"))?,
        );
        let mut writer = store.begin_write(request).map_err(artifact_error)?;
        if let Some(bytes) = validated.media().inline_bytes_for_wire() {
            verify_bytes(image, bytes)?;
            writer.write_chunk(bytes).map_err(artifact_error)?;
        } else {
            if validated.artifact().is_none_or(|artifact| {
                artifact.as_bytes() != image.artifact_bytes()
            }) {
                return Err(ControlError::ScopeMismatch.into());
            }
            let source = source_config
                .ok_or(Error::Corrupt("durable image source artifact configuration missing"))?;
            let mut reader =
                ArtifactStore::open_existing(source, digest).map_err(artifact_error)?;
            if reader.metadata().size() != image.bytes() || reader.metadata().digest() != digest {
                return Err(Error::Corrupt(
                    "source image artifact differs from its immutable reference",
                ));
            }
            while let Some(chunk) = reader.read_chunk(64 * 1024).map_err(artifact_error)? {
                if chunk.bytes().is_empty() {
                    return Err(Error::Corrupt("source image artifact reader made no progress"));
                }
                writer.write_chunk(chunk.bytes()).map_err(artifact_error)?;
            }
        }
        writer.finalize().map_err(artifact_error)?;
    }
    let metadata = store.verify(digest).map_err(artifact_error)?;
    if metadata.size() != image.bytes() || metadata.digest() != digest {
        return Err(Error::Corrupt("image artifact metadata differs from its control reference"));
    }
    Ok(digest)
}

fn retain_legacy_image(
    store: &ArtifactStore,
    image: &ImageAttachment,
    bytes: &[u8],
) -> Result<(), Error> {
    verify_bytes(image, bytes)?;
    let digest = ArtifactDigest::from_sha256(image.digest());
    let existing = store.metadata(digest).map_err(artifact_error)?;
    if existing.as_ref().is_none_or(|metadata| !metadata.is_referenceable()) {
        let request = WriteRequest::new(
            digest,
            image.bytes(),
            image.bytes(),
            MediaType::new(image.format().media_type()).map_err(artifact_error)?,
            EncryptionMetadata::unencrypted(),
            EventId::new(*image.operation().as_bytes())
                .map_err(|_| Error::Corrupt("invalid legacy image event identity"))?,
        );
        let mut writer = store.begin_write(request).map_err(artifact_error)?;
        writer.write_chunk(bytes).map_err(artifact_error)?;
        writer.finalize().map_err(artifact_error)?;
    }
    verify_retained_image(store, image)?;
    adopt_image_owner(store, image)
}

fn verify_retained_image(store: &ArtifactStore, image: &ImageAttachment) -> Result<(), Error> {
    let digest = ArtifactDigest::from_sha256(image.digest());
    let metadata = store.verify(digest).map_err(artifact_error)?;
    if metadata.size() != image.bytes() || metadata.digest() != digest {
        return Err(Error::Corrupt("image artifact metadata differs from its control reference"));
    }
    Ok(())
}

fn adopt_image_owner(store: &ArtifactStore, image: &ImageAttachment) -> Result<(), Error> {
    let digest = ArtifactDigest::from_sha256(image.digest());
    let owner = descriptor_owner(image);
    store.add_reference(owner, digest).map_err(artifact_error)?;
    store
        .migrate_reference_owner(legacy_owner(image), owner)
        .map_err(artifact_error)?;
    Ok(())
}

fn descriptor_bytes(image: &ImageAttachment) -> Vec<u8> {
    // Complete-pixel records retain the exact v1 descriptor produced since v0.0.2. Only the new
    // container-structure evidence requires v2, so historical recovery remains byte-identical.
    let mut bytes = match image.validation() {
        ImageValidation::CompletePixels => IMAGE_DESCRIPTOR_MAGIC_V1.to_vec(),
        ImageValidation::ContainerStructure => IMAGE_DESCRIPTOR_MAGIC_V2.to_vec(),
    };
    bytes.extend_from_slice(image.artifact_bytes());
    bytes.extend_from_slice(image.digest().as_bytes());
    bytes.extend_from_slice(&image.bytes().to_be_bytes());
    bytes.push(match image.format() {
        ImageFormat::Png => 1,
        ImageFormat::Jpeg => 2,
        ImageFormat::Gif => 3,
        ImageFormat::Webp => 4,
    });
    bytes.extend_from_slice(&image.dimensions().0.to_be_bytes());
    bytes.extend_from_slice(&image.dimensions().1.to_be_bytes());
    bytes.extend_from_slice(&image.frames().to_be_bytes());
    if image.validation() == ImageValidation::ContainerStructure {
        bytes.push(2);
    }
    bytes
}

fn verify_descriptor(image: &ImageAttachment, bytes: &[u8]) -> Result<(), Error> {
    if bytes != descriptor_bytes(image) {
        return Err(Error::Corrupt("image descriptor differs from its control reference"));
    }
    Ok(())
}

fn verify_bytes(image: &ImageAttachment, bytes: &[u8]) -> Result<(), Error> {
    let length = u64::try_from(bytes.len())
        .map_err(|_| Error::Corrupt("image byte length is not representable"))?;
    if bytes.is_empty()
        || length != image.bytes()
        || peritus_codec::sha256(bytes) != image.digest()
    {
        return Err(Error::Corrupt("image bytes differ from their immutable reference"));
    }
    Ok(())
}

fn image_artifact_id(image: &ImageAttachment) -> Result<ArtifactId, Error> {
    ArtifactId::new(*image.artifact_bytes()).map_err(|_| ControlError::InvalidInput.into())
}

fn descriptor_owner(image: &ImageAttachment) -> ReferenceOwner {
    super::checkpoints::snapshot::reference_owner(
        IMAGE_DESCRIPTOR_NAMESPACE,
        image.operation().as_bytes(),
    )
}

fn legacy_owner(image: &ImageAttachment) -> ReferenceOwner {
    super::checkpoints::snapshot::reference_owner(IMAGE_NAMESPACE, image.operation().as_bytes())
}

fn artifact_error(error: peritus_artifact_store::ArtifactStoreError) -> Error {
    Error::Io(std::io::Error::other(error))
}

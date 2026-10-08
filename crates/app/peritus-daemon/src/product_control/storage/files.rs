//! Atomic file-version bytes, source metadata and exact preview/admission proof retention.

use super::{
    ControlError, ControlGeneration, ControlOperation, ControlReceipt, ControlStore, Error,
    FILE_DESCRIPTOR_NAMESPACE,
};
use peritus_artifact_store::{
    ArtifactDigest, ArtifactReadHandle, ArtifactStore, EncryptionMetadata, MediaType,
    ReferenceOwner, WriteRequest,
};
use peritus_journal::StateInstall;
use peritus_product_runner::{
    attachment::{ValidatedFileText, ValidatedFileTextSource},
    control::{ControlIntent, FileVersion},
};
use peritus_types::EventId;
use std::{
    io::{Read, Seek, SeekFrom},
    sync::Mutex,
};

use super::checkpoints::snapshot::{
    PublicationPlan, PublicationPurpose, publication_claim,
};

const FILE_NAMESPACE: u16 = 3409;
const CONSENT_NAMESPACE: u16 = 3410;
const FILE_DESCRIPTOR_MAGIC: &[u8; 8] = b"pcfile01";

/// Finalized selected bytes and immutable proof, still free of marker/reference/C0 effects.
pub(crate) struct PreparedFilePublication {
    operation: ControlOperation,
    consent: Vec<u8>,
    descriptor: Vec<u8>,
    publication: PublicationPlan,
}

impl ControlGeneration {
    pub(crate) fn prepare_file(
        &self,
        operation: &ControlOperation,
        text: &ValidatedFileText,
        consent: Vec<u8>,
    ) -> Result<PreparedFilePublication, Error> {
        let mut reader = std::io::Cursor::new(text.text().as_bytes());
        self.prepare_file_reader(
            operation,
            &mut reader,
            text.digest(),
            text.bytes(),
            consent,
        )
    }

    pub(crate) fn prepare_file_stream<R: Read + Seek>(
        &self,
        operation: &ControlOperation,
        text: &mut ValidatedFileTextSource<R>,
        consent: Vec<u8>,
    ) -> Result<PreparedFilePublication, Error> {
        let digest = text.digest();
        let bytes = text.bytes();
        self.prepare_file_reader(operation, text.reader_mut(), digest, bytes, consent)
    }

    fn prepare_file_reader<R: Read + Seek>(
        &self,
        operation: &ControlOperation,
        reader: &mut R,
        digest: peritus_types::Sha256Digest,
        bytes: u64,
        consent: Vec<u8>,
    ) -> Result<PreparedFilePublication, Error> {
        let version = version(operation)?;
        if version.external_artifact()
            || version.operation() != operation.id()
            || !version.observation().matches_identity(digest, bytes)
        {
            return Err(ControlError::InvalidInput.into());
        }
        verify_consent(version, &consent)?;
        let store = ArtifactStore::open(self.reply_artifact_config().clone())
            .map_err(artifact_error)?;
        let artifact = spool_file_artifact(&store, version, reader)?;
        let descriptor = descriptor_bytes(version);
        let mut immutable_root = descriptor.clone();
        immutable_root.extend_from_slice(&consent);
        let claim = publication_claim(
            operation,
            FILE_DESCRIPTOR_NAMESPACE,
            *operation.id().as_bytes(),
            &immutable_root,
            PublicationPurpose::FileDescriptor,
        )?;
        let publication = PublicationPlan::new(claim, descriptor_owner(version), vec![artifact])?;
        Ok(PreparedFilePublication {
            operation: operation.clone(),
            consent,
            descriptor,
            publication,
        })
    }
}

impl ControlStore {
    /// Requires a retried captioned import to carry the exact preview bytes retained with the
    /// accepted canonical operation. The caller still resolves that immutable operation through
    /// C0 before returning its original receipt.
    pub(crate) fn verify_file_import_consent(
        &self,
        operation: &ControlOperation,
        expected: &[u8],
    ) -> Result<(), Error> {
        let version = version(operation)?;
        let consent = self
            .journal
            .state_record(CONSENT_NAMESPACE, version.operation().as_bytes())?
            .ok_or(Error::Corrupt("file version consent archive missing"))?;
        if consent.revision() != 1 {
            return Err(Error::Corrupt("invalid file version consent revision"));
        }
        verify_consent(version, consent.bytes())?;
        if consent.bytes() != expected {
            return Err(ControlError::IdempotencyConflict.into());
        }
        Ok(())
    }

    /// Publishes authorized exact selected bytes and their preview/admission proof together.
    /// The caller must verify selected workspace, path policy and consent before this operation.
    /// This function never opens a path, starts inference, or grants future read authority.
    pub fn accept_file(
        &mut self,
        operation: &ControlOperation,
        text: &ValidatedFileText,
        consent: Vec<u8>,
    ) -> Result<ControlReceipt, Error> {
        let prepared = self.generation().prepare_file(operation, text, consent)?;
        self.accept_prepared_file(prepared)
    }

    /// Streams one already validated selection into immutable content-addressed storage and
    /// atomically publishes only its exact descriptor and preview/admission proof in C0.
    pub fn accept_file_stream<R: Read + Seek>(
        &mut self,
        operation: &ControlOperation,
        text: &mut ValidatedFileTextSource<R>,
        consent: Vec<u8>,
    ) -> Result<ControlReceipt, Error> {
        let prepared = self.generation().prepare_file_stream(operation, text, consent)?;
        self.accept_prepared_file(prepared)
    }

    pub(crate) fn accept_prepared_file(
        &mut self,
        prepared: PreparedFilePublication,
    ) -> Result<ControlReceipt, Error> {
        if let Some(receipt) = self.resolve(&prepared.operation)? {
            return Ok(receipt);
        }
        let installs = vec![
            StateInstall::new(
                FILE_DESCRIPTOR_NAMESPACE,
                prepared.operation.id().as_bytes().to_vec(),
                None,
                1,
                prepared.descriptor,
            )?,
            StateInstall::new(
                CONSENT_NAMESPACE,
                prepared.operation.id().as_bytes().to_vec(),
                None,
                1,
                prepared.consent,
            )?,
        ];
        let mut append = self.prepare_installs(&prepared.operation, installs)?;
        append.publications.add_plan(prepared.publication);
        self.commit_prepared(append).map(|committed| committed.into_receipt())
    }

    /// Publishes consent and an immutable descriptor for bytes retained in the shared artifact
    /// store. Exact content validation and scoped authorization occur before entering C0.
    pub fn accept_file_source(
        &mut self,
        operation: &ControlOperation,
        consent: Vec<u8>,
    ) -> Result<ControlReceipt, Error> {
        let version = version(operation)?;
        if !version.external_artifact()
            || version.operation() != operation.id()
        {
            return Err(ControlError::InvalidInput.into());
        }
        verify_consent(version, &consent)?;
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        let proof = StateInstall::new(
            CONSENT_NAMESPACE,
            operation.id().as_bytes().to_vec(),
            None,
            1,
            consent,
        )?;
        self.accept_installs(operation, vec![proof])
    }

    pub(super) fn verify_file_archive(
        &self,
        operation: &ControlOperation,
        position: u64,
    ) -> Result<(), Error> {
        let version = version(operation)?;
        let consent = self
            .journal
            .state_record(CONSENT_NAMESPACE, version.operation().as_bytes())?
            .ok_or(Error::Corrupt("file version consent archive missing"))?;
        if consent.revision() != 1 || consent.producing_position() != position {
            return Err(Error::Corrupt("file version was not published atomically with consent"));
        }
        if version.external_artifact() {
            if self
                    .journal
                    .state_record(FILE_NAMESPACE, version.operation().as_bytes())?
                    .is_some()
                || self
                    .journal
                    .state_record(FILE_DESCRIPTOR_NAMESPACE, version.operation().as_bytes())?
                    .is_some()
            {
                return Err(Error::Corrupt("external file source has an invalid artifact binding"));
            }
        } else if let Some(descriptor) = self
            .journal
            .state_record(FILE_DESCRIPTOR_NAMESPACE, version.operation().as_bytes())?
        {
            if descriptor.revision() != 1 || descriptor.producing_position() != position {
                return Err(Error::Corrupt(
                    "file version descriptor was not published atomically with consent",
                ));
            }
            verify_descriptor(version, descriptor.bytes())?;
            verify_retained_artifact(&self.checkpoint_artifacts, version)?;
            if self
                .journal
                .state_record(FILE_NAMESPACE, version.operation().as_bytes())?
                .is_some()
            {
                return Err(Error::Corrupt(
                    "file version has both descriptor and legacy body archives",
                ));
            }
        } else {
            let artifact = self
                .journal
                .state_record(FILE_NAMESPACE, version.operation().as_bytes())?
                .ok_or(Error::Corrupt("file version artifact missing"))?;
            if artifact.revision() != 1 || artifact.producing_position() != position {
                return Err(Error::Corrupt(
                    "file version was not published atomically with consent",
                ));
            }
            verify_text(version, artifact.bytes())?;
        }
        verify_consent(version, consent.bytes())
    }

    /// Reads one authenticated offset page from a descriptor-backed text artifact. Historical
    /// inline records are decoded exactly and adopted into the artifact store on first use.
    pub(in crate::product_control) fn file_text_slice(
        &self,
        version: &FileVersion,
        reader: &Mutex<Option<ArtifactReadHandle>>,
        offset: u64,
        maximum_bytes: usize,
    ) -> Result<(String, Option<u64>), Error> {
        if maximum_bytes == 0 || version.external_artifact() {
            return Err(ControlError::InvalidInput.into());
        }
        let total = version.observation().bytes();
        if offset > total || (total != 0 && offset == total) {
            return Err(ControlError::InvalidInput.into());
        }
        let mut reader = reader
            .lock()
            .map_err(|_| Error::Corrupt("file artifact reader lock was poisoned"))?;
        if reader.is_none() {
            *reader = Some(self.open_file_artifact(version)?);
        }
        let reader = reader
            .as_mut()
            .ok_or(Error::Corrupt("file artifact reader was not retained"))?;
        verify_reader(version, reader)?;
        if total == 0 {
            return Ok((String::new(), None));
        }
        let requested = usize::try_from(
            (total - offset).min(
                u64::try_from(maximum_bytes).map_err(|_| ControlError::Capacity)?,
            ),
        )
        .map_err(|_| ControlError::Capacity)?;
        let chunk = reader
            .read_chunk_at(offset, requested)
            .map_err(artifact_error)?
            .ok_or(Error::Corrupt("file artifact ended before its immutable byte length"))?;
        if chunk.offset() != offset || chunk.bytes().is_empty() || chunk.bytes().len() > requested {
            return Err(Error::Corrupt("file artifact reader returned an invalid page"));
        }
        let length = validated_page_length(chunk.bytes())?;
        let text = std::str::from_utf8(&chunk.bytes()[..length])
            .map_err(|_| Error::Corrupt("file artifact contains invalid UTF-8"))?
            .to_owned();
        let end = offset
            .checked_add(u64::try_from(length).map_err(|_| ControlError::Capacity)?)
            .ok_or(ControlError::Capacity)?;
        let next = (end < total).then_some(end);
        Ok((text, next))
    }

    fn open_file_artifact(&self, version: &FileVersion) -> Result<ArtifactReadHandle, Error> {
        if version.external_artifact() {
            return Err(ControlError::InvalidInput.into());
        }
        if let Some(descriptor) = self
            .journal
            .state_record(FILE_DESCRIPTOR_NAMESPACE, version.operation().as_bytes())?
        {
            verify_descriptor(version, descriptor.bytes())?;
            verify_retained_artifact(&self.checkpoint_artifacts, version)?;
            adopt_file_owner(&self.checkpoint_artifacts, version)?;
        } else {
            let legacy = self
                .journal
                .state_record(FILE_NAMESPACE, version.operation().as_bytes())?
                .ok_or(Error::Corrupt("selected file version artifact missing"))?;
            verify_text(version, legacy.bytes())?;
            retain_legacy_file(&self.checkpoint_artifacts, version, legacy.bytes())?;
        }
        let reader = ArtifactStore::open_existing(
            &self.checkpoint_config,
            ArtifactDigest::from_sha256(version.observation().digest()),
        )
        .map_err(artifact_error)?;
        verify_reader(version, &reader)?;
        Ok(reader)
    }
}

fn version(operation: &ControlOperation) -> Result<&FileVersion, Error> {
    match operation.intent() {
        ControlIntent::AttachFile { file, .. } | ControlIntent::AttachFileSource { file } => {
            Ok(file.initial())
        }
        ControlIntent::RefreshFile { version, .. } => Ok(version),
        _ => Err(ControlError::InvalidInput.into()),
    }
}
fn validated_page_length(bytes: &[u8]) -> Result<usize, Error> {
    match std::str::from_utf8(bytes) {
        Ok(_) => Ok(bytes.len()),
        Err(error) if error.error_len().is_none() && error.valid_up_to() != 0 => {
            Ok(error.valid_up_to())
        }
        Err(error) if error.valid_up_to() == 0 => Err(ControlError::InvalidInput.into()),
        Err(_) => Err(Error::Corrupt("file artifact contains invalid UTF-8")),
    }
}
fn verify_text(version: &FileVersion, bytes: &[u8]) -> Result<ValidatedFileText, Error> {
    let text = ValidatedFileText::new(bytes.to_vec())?;
    if !version.observation().matches(&text) {
        return Err(Error::Corrupt("file artifact differs from its exact selected-byte binding"));
    }
    Ok(text)
}
fn spool_file_artifact<R: Read + Seek>(
    store: &ArtifactStore,
    version: &FileVersion,
    reader: &mut R,
) -> Result<ArtifactDigest, Error> {
    let digest = ArtifactDigest::from_sha256(version.observation().digest());
    let expected = version.observation().bytes();
    let existing = store.metadata(digest).map_err(artifact_error)?;
    if existing.as_ref().is_none_or(|metadata| !metadata.is_referenceable()) {
        let request = WriteRequest::new(
            digest,
            expected,
            expected.max(1),
            MediaType::new("text/plain;charset=utf-8").map_err(artifact_error)?,
            EncryptionMetadata::unencrypted(),
            EventId::new(*version.operation().as_bytes())
                .map_err(|_| Error::Corrupt("invalid file artifact event identity"))?,
        );
        let mut writer = store.begin_write(request).map_err(artifact_error)?;
        reader.seek(SeekFrom::Start(0))?;
        let mut total = 0_u64;
        let mut chunk = [0_u8; 64 * 1024];
        loop {
            let count = reader.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(u64::try_from(count).map_err(|_| ControlError::Capacity)?)
                .ok_or(ControlError::Capacity)?;
            if total > expected {
                return Err(ControlError::InvalidInput.into());
            }
            writer.write_chunk(&chunk[..count]).map_err(artifact_error)?;
        }
        if total != expected {
            return Err(ControlError::InvalidInput.into());
        }
        writer.finalize().map_err(artifact_error)?;
    }
    verify_retained_artifact(store, version)?;
    Ok(digest)
}

fn retain_legacy_file(
    store: &ArtifactStore,
    version: &FileVersion,
    bytes: &[u8],
) -> Result<(), Error> {
    verify_text(version, bytes)?;
    let digest = ArtifactDigest::from_sha256(version.observation().digest());
    let expected = version.observation().bytes();
    let existing = store.metadata(digest).map_err(artifact_error)?;
    if existing.as_ref().is_none_or(|metadata| !metadata.is_referenceable()) {
        let request = WriteRequest::new(
            digest,
            expected,
            expected.max(1),
            MediaType::new("text/plain;charset=utf-8").map_err(artifact_error)?,
            EncryptionMetadata::unencrypted(),
            EventId::new(*version.operation().as_bytes())
                .map_err(|_| Error::Corrupt("invalid legacy file event identity"))?,
        );
        let mut writer = store.begin_write(request).map_err(artifact_error)?;
        if !bytes.is_empty() {
            writer.write_chunk(bytes).map_err(artifact_error)?;
        }
        writer.finalize().map_err(artifact_error)?;
    }
    verify_retained_artifact(store, version)?;
    adopt_file_owner(store, version)
}

fn adopt_file_owner(store: &ArtifactStore, version: &FileVersion) -> Result<(), Error> {
    let digest = ArtifactDigest::from_sha256(version.observation().digest());
    let owner = descriptor_owner(version);
    store.add_reference(owner, digest).map_err(artifact_error)?;
    store
        .migrate_reference_owner(legacy_owner(version), owner)
        .map_err(artifact_error)?;
    Ok(())
}

fn verify_retained_artifact(
    store: &ArtifactStore,
    version: &FileVersion,
) -> Result<(), Error> {
    let digest = ArtifactDigest::from_sha256(version.observation().digest());
    let metadata = store.verify(digest).map_err(artifact_error)?;
    if metadata.digest() != digest || metadata.size() != version.observation().bytes() {
        return Err(Error::Corrupt(
            "file artifact metadata differs from its exact selected-byte binding",
        ));
    }
    Ok(())
}

fn verify_reader(version: &FileVersion, reader: &ArtifactReadHandle) -> Result<(), Error> {
    let digest = ArtifactDigest::from_sha256(version.observation().digest());
    if reader.metadata().digest() != digest
        || reader.metadata().size() != version.observation().bytes()
    {
        return Err(Error::Corrupt(
            "file artifact reader differs from its exact selected-byte binding",
        ));
    }
    Ok(())
}

fn descriptor_bytes(version: &FileVersion) -> Vec<u8> {
    let mut bytes = FILE_DESCRIPTOR_MAGIC.to_vec();
    bytes.extend_from_slice(version.artifact_bytes());
    bytes.extend_from_slice(version.observation().digest().as_bytes());
    bytes.extend_from_slice(&version.observation().bytes().to_be_bytes());
    bytes
}

fn verify_descriptor(version: &FileVersion, bytes: &[u8]) -> Result<(), Error> {
    if bytes != descriptor_bytes(version) {
        return Err(Error::Corrupt(
            "file descriptor differs from its exact selected-byte binding",
        ));
    }
    Ok(())
}

fn descriptor_owner(version: &FileVersion) -> ReferenceOwner {
    super::checkpoints::snapshot::reference_owner(
        FILE_DESCRIPTOR_NAMESPACE,
        version.operation().as_bytes(),
    )
}

fn legacy_owner(version: &FileVersion) -> ReferenceOwner {
    super::checkpoints::snapshot::reference_owner(FILE_NAMESPACE, version.operation().as_bytes())
}

fn artifact_error(error: peritus_artifact_store::ArtifactStoreError) -> Error {
    Error::Io(std::io::Error::other(error))
}
fn verify_consent(version: &FileVersion, bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty() || peritus_codec::sha256(bytes) != version.consent_digest() {
        return Err(Error::Corrupt("file version proof differs from its immutable binding"));
    }
    Ok(())
}

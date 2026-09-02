//! Validation and conversion of stored artifact values.

use peritus_types::{EventId, Sha256Digest};

use crate::{
    ArtifactDigest, ArtifactMetadata, ArtifactStoreError, CollectionGeneration, EncryptionMetadata,
    ErrorCode, FinalizationState, IntegrityState, MediaType, QuarantineState, RecoveryClass,
};

pub(super) struct RawMetadata {
    pub(super) size: i64,
    pub(super) media_type: String,
    pub(super) algorithm: Option<String>,
    pub(super) key_reference: Option<Vec<u8>>,
    pub(super) parameters_digest: Option<Vec<u8>>,
    pub(super) finalization: i64,
    pub(super) creating_event: Vec<u8>,
    pub(super) quarantine: i64,
    pub(super) quarantine_generation: Option<i64>,
    pub(super) integrity: i64,
}

impl RawMetadata {
    pub(super) fn validate(
        self,
        digest: ArtifactDigest,
    ) -> Result<ArtifactMetadata, ArtifactStoreError> {
        let media_type = MediaType::new(self.media_type)
            .map_err(|_| corrupt_catalog("invalid stored media type"))?;
        let encryption = match (self.algorithm, self.key_reference, self.parameters_digest) {
            (None, None, None) => EncryptionMetadata::unencrypted(),
            (Some(algorithm), Some(key), Some(parameters)) => EncryptionMetadata::envelope(
                algorithm,
                Sha256Digest::new(array::<32>(&key)?),
                Sha256Digest::new(array::<32>(&parameters)?),
            )
            .map_err(|_| corrupt_catalog("invalid stored encryption metadata"))?,
            _ => return Err(corrupt_catalog("incomplete stored encryption metadata")),
        };
        let finalization = match self.finalization {
            1 => FinalizationState::Partial,
            2 => FinalizationState::Finalized,
            _ => return Err(corrupt_catalog("unknown finalization state")),
        };
        let quarantine = decode_quarantine(self.quarantine, self.quarantine_generation)?;
        let integrity = match self.integrity {
            1 => IntegrityState::Healthy,
            2 => IntegrityState::Corrupt,
            _ => return Err(corrupt_catalog("unknown integrity state")),
        };
        let creating_event = EventId::new(array::<16>(&self.creating_event)?)
            .map_err(|_| corrupt_catalog("zero creating event identity"))?;
        Ok(ArtifactMetadata::new(
            digest,
            u64::try_from(self.size).map_err(|_| corrupt_catalog("negative artifact size"))?,
            media_type,
            encryption,
            finalization,
            creating_event,
            quarantine,
        )
        .with_integrity(integrity))
    }
}

pub(super) fn decode_quarantine(
    tag: i64,
    generation: Option<i64>,
) -> Result<QuarantineState, ArtifactStoreError> {
    match (tag, generation) {
        (1, None) => Ok(QuarantineState::Active),
        (2, Some(value)) => Ok(QuarantineState::Quarantined {
            since: CollectionGeneration::new(
                u64::try_from(value)
                    .map_err(|_| corrupt_catalog("invalid quarantine generation"))?,
            )?,
        }),
        _ => Err(corrupt_catalog("inconsistent quarantine state")),
    }
}

pub(super) fn encode_quarantine(
    state: QuarantineState,
) -> Result<(i64, Option<i64>), ArtifactStoreError> {
    match state {
        QuarantineState::Active => Ok((1, None)),
        QuarantineState::Quarantined { since } => Ok((2, Some(sqlite_integer(since.get())?))),
    }
}

pub(super) fn array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], ArtifactStoreError> {
    bytes.try_into().map_err(|_| corrupt_catalog("invalid fixed-width metadata field"))
}

pub(super) fn sqlite_integer(value: u64) -> Result<i64, ArtifactStoreError> {
    i64::try_from(value).map_err(|_| {
        ArtifactStoreError::message(
            ErrorCode::ArithmeticOverflow,
            RecoveryClass::CorrectRequest,
            "value exceeds durable SQLite integer capacity",
        )
    })
}

pub(super) const fn missing_artifact() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::MissingArtifact,
        RecoveryClass::CorrectRequest,
        "artifact metadata does not exist",
    )
}

pub(super) const fn corrupt_catalog(message: &'static str) -> ArtifactStoreError {
    ArtifactStoreError::message(ErrorCode::CorruptObject, RecoveryClass::TerminalIntegrity, message)
}

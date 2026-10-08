//! Canonical credential-registry history construction and commit.

use peritus_approval::decode_credential_registry;
use peritus_codec::{CodecLimits, decode_frame, encode_frame, sha256};
use peritus_types::{CommandId, EventId, EventSequence, OneBasedNumberError, Sha256Digest};

use crate::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommittedBatch, CommittedRecord,
    CredentialRegistryInstall, EventDraft, ExactFrame, HeadExpectation, JournalError,
    JournalErrorKind, SqliteJournal, StoreId,
};

/// Canonical journal-owned credential-registry installation event family.
pub const CREDENTIAL_REGISTRY_EVENT_FAMILY: u16 = 94;
/// Historical schema understood by the credential-registry replay decoder.
pub const CREDENTIAL_REGISTRY_EVENT_SCHEMA: u16 = 1;
const EVENT_PAYLOAD_DOMAIN: &[u8] = b"PERITUS-C0-CREDENTIAL-REGISTRY-INSTALL\0";
const AGGREGATE_ID_DOMAIN: &[u8] = b"peritus.c0.credential-registry.aggregate.v1\0";
const COMMAND_ID_DOMAIN: &[u8] = b"peritus.c0.credential-registry.command.v1\0";
const EVENT_ID_DOMAIN: &[u8] = b"peritus.c0.credential-registry.event.v1\0";
const REQUEST_DIGEST_DOMAIN: &[u8] = b"peritus.c0.credential-registry.request.v1\0";

/// Decoded immutable credential-registry installation observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialRegistryEvent {
    store_id: StoreId,
    expected_revision: Option<u64>,
    revision: u64,
    generation: u64,
    snapshot_digest: Sha256Digest,
    snapshot: ExactFrame,
}

impl CredentialRegistryEvent {
    /// Returns the journal owner bound into the canonical event payload and identities.
    #[must_use]
    pub const fn store_id(&self) -> StoreId {
        self.store_id
    }

    /// Returns the exact prior registry revision, or absence at genesis.
    #[must_use]
    pub const fn expected_revision(&self) -> Option<u64> {
        self.expected_revision
    }

    /// Returns the installed exact-successor registry revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the strictly increasing registry lineage generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the digest of the exact canonical public snapshot payload.
    #[must_use]
    pub const fn snapshot_digest(&self) -> Sha256Digest {
        self.snapshot_digest
    }

    /// Borrows the exact historical snapshot frame without reserialization.
    #[must_use]
    pub fn snapshot_bytes(&self) -> &[u8] {
        self.snapshot.bytes()
    }
}

impl SqliteJournal {
    /// Commits one credential-registry install through the store's canonical registry history.
    ///
    /// Aggregate, command, and event identities are deterministically derived from the bound store
    /// and exact install. The event frame contains the complete canonical public snapshot frame,
    /// so registry history never depends on caller-chosen identities or opaque event bytes.
    ///
    /// # Errors
    ///
    /// Returns typed input, history-currentness, idempotency, or storage failures.
    pub fn commit_credential_registry(
        &mut self,
        install: CredentialRegistryInstall,
    ) -> Result<CommittedBatch, JournalError> {
        let aggregate = registry_aggregate(self.store_id)?;
        let observed_head = self.head(aggregate)?;
        let head = match (install.expected_revision(), observed_head) {
            (None, None) => HeadExpectation::Absent(aggregate),
            (Some(revision), Some(head)) if head.sequence().get() == revision => {
                HeadExpectation::Present(head)
            }
            _ => {
                return Err(JournalError::new(
                    JournalErrorKind::StaleRegistry,
                    "plan credential registry commit",
                    "canonical registry history differs from the install precondition",
                ));
            }
        };
        let frame = registry_event_frame(self.store_id, &install)?;
        let command_id = command_id(self.store_id, &install)?;
        let event_id = event_id(self.store_id, &install)?;
        let sequence = EventSequence::new(install.revision()).map_err(sequence_error)?;
        let event = EventDraft::new(
            aggregate,
            sequence,
            event_id,
            observed_head.map(crate::AggregateHead::event_id),
            frame.clone(),
            install.digest(),
            Vec::new(),
        )?;
        let request_digest = request_digest(self.store_id, &frame);
        let request = AppendRequest::new(
            self.store_id,
            command_id,
            request_digest,
            vec![head],
            vec![event],
            Vec::new(),
            Vec::new(),
            None,
            Some(install),
            Vec::new(),
        );
        self.append(request.plan()?)
    }
}

fn registry_aggregate(store_id: StoreId) -> Result<AggregateKey, JournalError> {
    let bytes = derived_identifier(AGGREGATE_ID_DOMAIN, store_id, &[]);
    Ok(AggregateKey::new(AggregateKind::CredentialRegistry, AggregateId::new(bytes)?))
}

fn command_id(
    store_id: StoreId,
    install: &CredentialRegistryInstall,
) -> Result<CommandId, JournalError> {
    CommandId::new(install_identifier(
        COMMAND_ID_DOMAIN,
        store_id,
        install.expected_revision(),
        install.revision(),
        install.generation(),
        install.digest(),
    ))
        .map_err(|_| identity_error())
}

fn event_id(
    store_id: StoreId,
    install: &CredentialRegistryInstall,
) -> Result<EventId, JournalError> {
    EventId::new(install_identifier(
        EVENT_ID_DOMAIN,
        store_id,
        install.expected_revision(),
        install.revision(),
        install.generation(),
        install.digest(),
    ))
        .map_err(|_| identity_error())
}

fn install_identifier(
    domain: &[u8],
    store_id: StoreId,
    expected_revision: Option<u64>,
    revision: u64,
    generation: u64,
    digest: Sha256Digest,
) -> [u8; 16] {
    let mut binding = Vec::with_capacity(domain.len() + 80);
    binding.extend_from_slice(&expected_revision_bytes(expected_revision));
    binding.extend_from_slice(&revision.to_be_bytes());
    binding.extend_from_slice(&generation.to_be_bytes());
    binding.extend_from_slice(digest.as_bytes());
    derived_identifier(domain, store_id, &binding)
}

fn derived_identifier(domain: &[u8], store_id: StoreId, binding: &[u8]) -> [u8; 16] {
    let mut preimage = Vec::with_capacity(domain.len() + 16 + binding.len());
    preimage.extend_from_slice(domain);
    preimage.extend_from_slice(store_id.as_bytes());
    preimage.extend_from_slice(binding);
    let digest = sha256(&preimage);
    let mut identifier = [0_u8; 16];
    identifier.copy_from_slice(&digest.as_bytes()[..16]);
    identifier[0] |= 0x80;
    identifier
}

fn registry_event_frame(
    store_id: StoreId,
    install: &CredentialRegistryInstall,
) -> Result<ExactFrame, JournalError> {
    let snapshot_length = u64::try_from(install.snapshot_bytes().len()).map_err(|_| {
        JournalError::new(
            JournalErrorKind::SequenceOverflow,
            "encode credential registry event",
            "registry snapshot length cannot be represented",
        )
    })?;
    let mut payload =
        Vec::with_capacity(EVENT_PAYLOAD_DOMAIN.len() + 105 + install.snapshot_bytes().len());
    payload.extend_from_slice(EVENT_PAYLOAD_DOMAIN);
    payload.extend_from_slice(store_id.as_bytes());
    payload.extend_from_slice(&expected_revision_bytes(install.expected_revision()));
    payload.extend_from_slice(&install.revision().to_be_bytes());
    payload.extend_from_slice(&install.generation().to_be_bytes());
    payload.extend_from_slice(install.digest().as_bytes());
    payload.extend_from_slice(&snapshot_length.to_be_bytes());
    payload.extend_from_slice(install.snapshot_bytes());
    let bytes = encode_frame(
        CREDENTIAL_REGISTRY_EVENT_FAMILY,
        CREDENTIAL_REGISTRY_EVENT_SCHEMA,
        &payload,
        CodecLimits::PRODUCTION,
    )
    .map_err(|_| {
                JournalError::new(
                    JournalErrorKind::InvalidInput,
                    "encode credential registry event",
                    "canonical registry event exceeds the production frame bound",
                )
            })?;
    ExactFrame::new(bytes)
}

/// Decodes and validates one exact historical credential-registry installation event.
///
/// The decoder is versioned independently of the mutable protocol-family catalog. It validates
/// the immutable payload, public snapshot, journal owner, derived aggregate/command/event
/// identities, revision binding, predecessor shape, and empty causal roots without re-encoding
/// either historical frame.
///
/// # Errors
///
/// Returns unsupported schema for a non-v1 family-94 frame, invalid input for a malformed typed
/// payload, or corrupt journal when envelope identities disagree with the decoded event.
pub fn decode_credential_registry_event(
    record: &CommittedRecord,
    expected_store_id: StoreId,
) -> Result<CredentialRegistryEvent, JournalError> {
    let frame = decode_frame(record.frame_bytes(), CodecLimits::PRODUCTION)
        .map_err(|_| invalid_event("credential registry event frame is not canonical"))?;
    if frame.header().family() != CREDENTIAL_REGISTRY_EVENT_FAMILY
        || frame.header().schema_version() != CREDENTIAL_REGISTRY_EVENT_SCHEMA
    {
        return Err(unsupported_event());
    }
    let event = decode_v1(frame.payload())?;
    let expected_aggregate = registry_aggregate(event.store_id)?;
    let expected_command = CommandId::new(install_identifier(
        COMMAND_ID_DOMAIN,
        event.store_id,
        event.expected_revision,
        event.revision,
        event.generation,
        event.snapshot_digest,
    ))
    .map_err(|_| corrupt_event("credential registry command identity is invalid"))?;
    let expected_event = EventId::new(install_identifier(
        EVENT_ID_DOMAIN,
        event.store_id,
        event.expected_revision,
        event.revision,
        event.generation,
        event.snapshot_digest,
    ))
    .map_err(|_| corrupt_event("credential registry event identity is invalid"))?;
    if event.store_id != expected_store_id
        || record.aggregate() != expected_aggregate
        || record.command_id() != expected_command
        || record.event_id() != expected_event
        || record.sequence().get() != event.revision
        || record.previous_event_id().is_some() != event.expected_revision.is_some()
        || record.revision_digest() != event.snapshot_digest
        || !record.causal_parents().is_empty()
    {
        return Err(corrupt_event(
            "credential registry event disagrees with its journal envelope",
        ));
    }
    Ok(event)
}

fn decode_v1(mut payload: &[u8]) -> Result<CredentialRegistryEvent, JournalError> {
    payload = payload
        .strip_prefix(EVENT_PAYLOAD_DOMAIN)
        .ok_or_else(|| invalid_event("credential registry event domain is invalid"))?;
    let store_id = StoreId::new(take::<16>(&mut payload)?)
        .map_err(|_| invalid_event("credential registry event store identity is invalid"))?;
    let expected_revision = decode_expected_revision(take::<9>(&mut payload)?)?;
    let revision = u64::from_be_bytes(take::<8>(&mut payload)?);
    let generation = u64::from_be_bytes(take::<8>(&mut payload)?);
    let snapshot_digest = Sha256Digest::new(take::<32>(&mut payload)?);
    let snapshot_length = usize::try_from(u64::from_be_bytes(take::<8>(&mut payload)?))
        .map_err(|_| invalid_event("credential registry snapshot length is unrepresentable"))?;
    if payload.len() != snapshot_length
        || !crate::verified::cas_successor(expected_revision, revision)
        || generation == 0
    {
        return Err(invalid_event(
            "credential registry event revision, generation, or snapshot length is invalid",
        ));
    }
    let snapshot = ExactFrame::new(payload.to_vec())
        .map_err(|_| invalid_event("credential registry event snapshot frame is invalid"))?;
    let snapshot_payload = super::credential_registry_payload(&snapshot)
        .map_err(|_| invalid_event("credential registry event snapshot schema is invalid"))?;
    let decoded = decode_credential_registry(snapshot_payload)
        .map_err(|_| invalid_event("credential registry event snapshot payload is invalid"))?;
    let decoded_digest = decoded
        .digest()
        .map_err(|_| invalid_event("credential registry event snapshot digest is invalid"))?;
    if decoded.revision().get() != revision || decoded_digest != snapshot_digest {
        return Err(invalid_event(
            "credential registry event snapshot disagrees with its revision or digest",
        ));
    }
    Ok(CredentialRegistryEvent {
        store_id,
        expected_revision,
        revision,
        generation,
        snapshot_digest,
        snapshot,
    })
}

fn take<const N: usize>(input: &mut &[u8]) -> Result<[u8; N], JournalError> {
    if input.len() < N {
        return Err(invalid_event("credential registry event payload is truncated"));
    }
    let (head, tail) = input.split_at(N);
    let mut value = [0_u8; N];
    value.copy_from_slice(head);
    *input = tail;
    Ok(value)
}

fn decode_expected_revision(bytes: [u8; 9]) -> Result<Option<u64>, JournalError> {
    let mut revision_bytes = [0_u8; 8];
    revision_bytes.copy_from_slice(&bytes[1..]);
    let revision = u64::from_be_bytes(revision_bytes);
    match (bytes[0], revision) {
        (0, 0) => Ok(None),
        (1, value) if value != 0 => Ok(Some(value)),
        _ => Err(invalid_event(
            "credential registry event predecessor revision is not canonical",
        )),
    }
}

fn request_digest(store_id: StoreId, frame: &ExactFrame) -> Sha256Digest {
    let mut binding = Vec::with_capacity(REQUEST_DIGEST_DOMAIN.len() + 16 + 32);
    binding.extend_from_slice(REQUEST_DIGEST_DOMAIN);
    binding.extend_from_slice(store_id.as_bytes());
    binding.extend_from_slice(frame.digest().as_bytes());
    sha256(&binding)
}

fn expected_revision_bytes(expected: Option<u64>) -> [u8; 9] {
    let mut bytes = [0_u8; 9];
    if let Some(revision) = expected {
        bytes[0] = 1;
        bytes[1..].copy_from_slice(&revision.to_be_bytes());
    }
    bytes
}

const fn identity_error() -> JournalError {
    JournalError::new(
        JournalErrorKind::InvalidInput,
        "derive credential registry identity",
        "derived credential registry identity is invalid",
    )
}

const fn invalid_event(detail: &'static str) -> JournalError {
    JournalError::new(JournalErrorKind::InvalidInput, "decode credential registry event", detail)
}

const fn corrupt_event(detail: &'static str) -> JournalError {
    JournalError::new(
        JournalErrorKind::CorruptJournal,
        "decode credential registry event",
        detail,
    )
}

const fn unsupported_event() -> JournalError {
    JournalError::new(
        JournalErrorKind::UnsupportedSchema,
        "decode credential registry event",
        "credential registry event family or schema is unsupported",
    )
}

const fn sequence_error(_error: OneBasedNumberError) -> JournalError {
    JournalError::new(
        JournalErrorKind::SequenceOverflow,
        "plan credential registry commit",
        "credential registry revision cannot be represented as an event sequence",
    )
}

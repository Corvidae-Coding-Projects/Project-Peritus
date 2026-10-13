//! Canonical credential-registry history construction and commit.

use peritus_codec::{CanonicalReader, CodecLimits, decode_frame, encode_frame, sha256};
use peritus_types::{CommandId, EventId, EventSequence, OneBasedNumberError, Sha256Digest};

use crate::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommittedBatch,
    CredentialRegistryInstall, EventDraft, ExactFrame, HeadExpectation, JournalError,
    JournalErrorKind, SqliteJournal, StoreId,
};

const EVENT_FRAME_FAMILY: u16 = 94;
const EVENT_FRAME_SCHEMA: u16 = 1;
const EVENT_PAYLOAD_DOMAIN: &[u8] = b"PERITUS-C0-CREDENTIAL-REGISTRY-INSTALL\0";
const AGGREGATE_ID_DOMAIN: &[u8] = b"peritus.c0.credential-registry.aggregate.v1\0";
const COMMAND_ID_DOMAIN: &[u8] = b"peritus.c0.credential-registry.command.v1\0";
const EVENT_ID_DOMAIN: &[u8] = b"peritus.c0.credential-registry.event.v1\0";
const REQUEST_DIGEST_DOMAIN: &[u8] = b"peritus.c0.credential-registry.request.v1\0";

impl SqliteJournal {
    /// Validates the complete canonical registry lineage against the current durable snapshot.
    ///
    /// Each install changes the snapshot digest used as its journal revision. Replay checks the
    /// owning store, canonical public snapshot, exact successor, increasing generation, derived
    /// identities and current row; it does not infer approval authority from event bytes.
    ///
    /// # Errors
    /// Returns a corruption or storage error for incomplete, noncanonical or contradictory history.
    pub fn validate_credential_registry_history(&self) -> Result<(), JournalError> {
        let aggregate = registry_aggregate(self.store_id)?;
        let identities = self.aggregate_ids(AggregateKind::CredentialRegistry, 1)?;
        if identities.is_empty() {
            return match self.current_credential_registry() {
                Err(error) if error.kind() == JournalErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
                Ok(_) => Err(history_error()),
            };
        }
        if identities[0] != aggregate.id() {
            return Err(history_error());
        }
        let records = self.records_for_aggregate(aggregate)?;
        let mut previous: Option<CredentialRegistryInstall> = None;
        for record in &records {
            let frame = decode_frame(record.frame_bytes(), CodecLimits::PRODUCTION)
                .map_err(|_| history_error())?;
            if frame.header().family() != EVENT_FRAME_FAMILY
                || frame.header().schema_version() != EVENT_FRAME_SCHEMA
            {
                return Err(history_error());
            }
            let payload = frame.payload();
            let mut reader = CanonicalReader::new(payload, CodecLimits::PRODUCTION);
            if reader
                .read_fixed::<{ EVENT_PAYLOAD_DOMAIN.len() }>()
                .map_err(|_| history_error())?
                .as_slice()
                != EVENT_PAYLOAD_DOMAIN
                || reader.read_fixed::<16>().map_err(|_| history_error())?
                    != *self.store_id.as_bytes()
                || reader.read_fixed::<9>().map_err(|_| history_error())?
                    != expected_revision_bytes(
                        previous.as_ref().map(CredentialRegistryInstall::revision),
                    )
            {
                return Err(history_error());
            }
            let revision = reader.read_u64().map_err(|_| history_error())?;
            let generation = reader.read_u64().map_err(|_| history_error())?;
            let digest = reader.read_fixed::<32>().map_err(|_| history_error())?;
            let length = usize::try_from(reader.read_u64().map_err(|_| history_error())?)
                .map_err(|_| history_error())?;
            if length != reader.remaining() {
                return Err(history_error());
            }
            let snapshot = ExactFrame::new(payload[reader.offset()..].to_vec())
                .map_err(|_| history_error())?;
            let snapshot = peritus_approval::decode_credential_registry(
                crate::authority::credential_registry_payload(&snapshot)?,
            )
            .map_err(|_| history_error())?;
            let install = CredentialRegistryInstall::new(
                previous.as_ref().map(CredentialRegistryInstall::revision),
                generation,
                &snapshot,
            )
            .map_err(|_| history_error())?;
            if previous.as_ref().is_some_and(|prior| prior.generation() >= generation)
                || revision != install.revision()
                || digest != *install.digest().as_bytes()
                || record.sequence().get() != revision
                || record.revision_digest() != install.digest()
                || record.event_id() != event_id(self.store_id, &install)?
                || record.command_id() != command_id(self.store_id, &install)?
                || record.frame_bytes() != registry_event_frame(self.store_id, &install)?.bytes()
            {
                return Err(history_error());
            }
            previous = Some(install);
        }
        let last = previous.ok_or_else(history_error)?;
        let position = records.last().ok_or_else(history_error)?.global_position();
        let current = self.current_credential_registry()?;
        if current.revision() != last.revision()
            || current.generation() != last.generation()
            || current.digest() != last.digest()
            || current.snapshot_bytes() != last.snapshot_bytes()
            || current.producing_position() != position
        {
            return Err(history_error());
        }
        Ok(())
    }
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

const fn history_error() -> JournalError {
    JournalError::new(
        JournalErrorKind::CorruptJournal,
        "replay credential registry history",
        "canonical registry lineage differs from its owning store, successor, snapshot or current row",
    )
}

fn registry_aggregate(store_id: StoreId) -> Result<AggregateKey, JournalError> {
    let bytes = derived_identifier(AGGREGATE_ID_DOMAIN, store_id, &[]);
    Ok(AggregateKey::new(AggregateKind::CredentialRegistry, AggregateId::new(bytes)?))
}

fn command_id(
    store_id: StoreId,
    install: &CredentialRegistryInstall,
) -> Result<CommandId, JournalError> {
    CommandId::new(install_identifier(COMMAND_ID_DOMAIN, store_id, install))
        .map_err(|_| identity_error())
}

fn event_id(
    store_id: StoreId,
    install: &CredentialRegistryInstall,
) -> Result<EventId, JournalError> {
    EventId::new(install_identifier(EVENT_ID_DOMAIN, store_id, install))
        .map_err(|_| identity_error())
}

fn install_identifier(
    domain: &[u8],
    store_id: StoreId,
    install: &CredentialRegistryInstall,
) -> [u8; 16] {
    let mut binding = Vec::with_capacity(domain.len() + 80);
    binding.extend_from_slice(&expected_revision_bytes(install.expected_revision()));
    binding.extend_from_slice(&install.revision().to_be_bytes());
    binding.extend_from_slice(&install.generation().to_be_bytes());
    binding.extend_from_slice(install.digest().as_bytes());
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
    let bytes =
        encode_frame(EVENT_FRAME_FAMILY, EVENT_FRAME_SCHEMA, &payload, CodecLimits::PRODUCTION)
            .map_err(|_| {
                JournalError::new(
                    JournalErrorKind::InvalidInput,
                    "encode credential registry event",
                    "canonical registry event exceeds the production frame bound",
                )
            })?;
    ExactFrame::new(bytes)
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

const fn sequence_error(_error: OneBasedNumberError) -> JournalError {
    JournalError::new(
        JournalErrorKind::SequenceOverflow,
        "plan credential registry commit",
        "credential registry revision cannot be represented as an event sequence",
    )
}

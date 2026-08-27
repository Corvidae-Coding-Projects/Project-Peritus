//! Canonical credential-registry history construction and commit.

use peritus_codec::{CodecLimits, encode_frame, sha256};
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

//! Deterministic C0 fact recorded before an uploaded object is finalized.

use peritus_app_protocol::ArtifactMetadata;
use peritus_codec::{CodecLimits, encode_frame, sha256};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommandResolution, EventDraft,
    ExactFrame, HeadExpectation, SqliteJournal,
};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub const ARTIFACT_UPLOAD_ACCEPTED_FAMILY: u16 = 65_000;

pub fn record(
    journal: &mut SqliteJournal,
    metadata: &ArtifactMetadata,
) -> Result<peritus_journal::CommittedBatch, DaemonError> {
    let frame = frame(metadata)?;
    let command_id = command_id(metadata);
    let request_digest = digest_with_domain(b"peritus/g0/artifact-upload-request/v1\0", metadata);
    match journal.resolve_command(command_id, request_digest).map_err(journal_error)? {
        CommandResolution::Committed(batch) => return Ok(batch),
        CommandResolution::Conflict { .. } => {
            return Err(DaemonError::new(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::Operator,
                "record artifact upload acceptance",
                "derived artifact command identity is bound to different metadata",
            ));
        }
        CommandResolution::DefinitelyAbsent => {}
    }
    let aggregate = AggregateKey::new(AggregateKind::Application, aggregate_id(metadata));
    let event = EventDraft::new(
        aggregate,
        EventSequence::first(),
        event_id(metadata),
        None,
        frame,
        digest_with_domain(b"peritus/g0/artifact-upload-revision/v1\0", metadata),
        Vec::new(),
    )
    .map_err(journal_error)?;
    let plan = AppendRequest::new(
        journal.store_id(),
        command_id,
        request_digest,
        vec![HeadExpectation::Absent(aggregate)],
        vec![event],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .map_err(journal_error)?;
    journal.append(plan).map_err(journal_error)
}

pub fn event_id(metadata: &ArtifactMetadata) -> EventId {
    EventId::new(id_with_domain(b"peritus/g0/artifact-upload-event/v1\0", metadata))
        .expect("domain-separated SHA-256 identifier is nonzero")
}

fn command_id(metadata: &ArtifactMetadata) -> CommandId {
    CommandId::new(id_with_domain(b"peritus/g0/artifact-upload-command/v1\0", metadata))
        .expect("domain-separated SHA-256 identifier is nonzero")
}

fn aggregate_id(metadata: &ArtifactMetadata) -> AggregateId {
    AggregateId::new(id_with_domain(b"peritus/g0/artifact-upload-aggregate/v1\0", metadata))
        .expect("domain-separated SHA-256 identifier is nonzero")
}

fn frame(metadata: &ArtifactMetadata) -> Result<ExactFrame, DaemonError> {
    let payload = payload(metadata)?;
    let bytes = encode_frame(ARTIFACT_UPLOAD_ACCEPTED_FAMILY, 1, &payload, CodecLimits::PRODUCTION)
        .map_err(|_| invalid("artifact publication frame exceeds canonical codec limits"))?;
    ExactFrame::new(bytes).map_err(journal_error)
}

fn payload(metadata: &ArtifactMetadata) -> Result<Vec<u8>, DaemonError> {
    let media = metadata.media_type().as_str().as_bytes();
    let media_length = u16::try_from(media.len())
        .map_err(|_| invalid("artifact media type length cannot be represented"))?;
    let mut bytes = Vec::with_capacity(16 + 16 + 8 + 32 + 2 + media.len());
    bytes.extend_from_slice(metadata.transfer_id().as_bytes());
    bytes.extend_from_slice(metadata.artifact_id().as_bytes());
    bytes.extend_from_slice(&metadata.byte_size().to_be_bytes());
    bytes.extend_from_slice(metadata.digest().as_bytes());
    bytes.extend_from_slice(&media_length.to_be_bytes());
    bytes.extend_from_slice(media);
    Ok(bytes)
}

fn digest_with_domain(domain: &[u8], metadata: &ArtifactMetadata) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(domain.len() + 80);
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(metadata.artifact_id().as_bytes());
    bytes.extend_from_slice(&metadata.byte_size().to_be_bytes());
    bytes.extend_from_slice(metadata.digest().as_bytes());
    bytes.extend_from_slice(metadata.media_type().as_str().as_bytes());
    sha256(&bytes)
}

fn id_with_domain(domain: &[u8], metadata: &ArtifactMetadata) -> [u8; 16] {
    let digest = digest_with_domain(domain, metadata);
    let mut identifier = [0_u8; 16];
    identifier.copy_from_slice(&digest.as_bytes()[..16]);
    identifier
}

fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "record artifact upload acceptance",
        detail,
    )
}

fn journal_error(error: peritus_journal::JournalError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        error.operation(),
        error.to_string(),
        error,
    )
}

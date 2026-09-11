//! Durable private attachment claims, created before accepting scoped upload bytes.
//!
//! A claim is not attachment inclusion or permission to execute. The authenticated workbench
//! boundary must validate the selected conversation before constructing this host-only scope.

use peritus_app_protocol::{ArtifactMetadata, WorkbenchQuery};
use peritus_codec::{CodecLimits, decode_frame, encode_frame, sha256};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, CommandResolution, EventDraft,
    ExactFrame, HeadExpectation, SqliteJournal, StateInstall,
};
use peritus_protocol::schema::ARTIFACT_SCOPE_CLAIM_FAMILY as FRAME_FAMILY;
use peritus_types::{ActorId, ArtifactId, CommandId, EventId, EventSequence, Sha256Digest};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

const NAMESPACE: u16 = 3500;
const CLAIM_BYTES: usize = 80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactScope {
    actor: ActorId,
    query: WorkbenchQuery,
}

impl ArtifactScope {
    pub(crate) const fn new(actor: ActorId, query: WorkbenchQuery) -> Self {
        Self { actor, query }
    }
    pub(crate) const fn actor(self) -> ActorId {
        self.actor
    }
    fn bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(CLAIM_BYTES);
        bytes.extend_from_slice(self.actor.as_bytes());
        bytes.extend_from_slice(self.query.conversation().as_bytes());
        bytes.extend_from_slice(self.query.workspace().as_bytes());
        bytes
    }
}

pub(super) fn claimed_scope(
    journal: &SqliteJournal,
    artifact: ArtifactId,
) -> Result<Option<ArtifactScope>, DaemonError> {
    let Some(row) = read_claim(journal, artifact)? else {
        return Ok(None);
    };
    if row.revision() != 1 || row.bytes().len() != CLAIM_BYTES {
        return Err(corrupt());
    }
    let bytes = row.bytes();
    let actor = ActorId::new(array(&bytes[..16])?).map_err(|_| corrupt())?;
    let conversation =
        peritus_app_protocol::ConversationId::new(array(&bytes[16..32])?).map_err(|_| corrupt())?;
    let workspace =
        peritus_types::WorkspaceId::new(array(&bytes[32..48])?).map_err(|_| corrupt())?;
    Ok(Some(ArtifactScope::new(actor, WorkbenchQuery::new(conversation, workspace))))
}

pub(super) fn claim(
    journal: &mut SqliteJournal,
    scope: ArtifactScope,
    metadata: &ArtifactMetadata,
) -> Result<(), DaemonError> {
    let mut claim = scope.bytes();
    claim.extend_from_slice(
        fingerprint(
            metadata.artifact_id(),
            metadata.byte_size(),
            metadata.digest(),
            metadata.media_type().as_str(),
        )
        .as_bytes(),
    );
    if let Some(row) = read_claim(journal, metadata.artifact_id())? {
        if row.revision() != 1 || row.bytes() != claim {
            return Err(unauthorized());
        }
        return Ok(());
    }
    // An older, unscoped object may belong to another operation. Do not adopt it by guessing ID.
    if journal.application_artifact(metadata.artifact_id()).map_err(storage)?.is_some() {
        return Err(unauthorized());
    }
    let mut payload = metadata.artifact_id().as_bytes().to_vec();
    payload.extend_from_slice(&claim);
    let command =
        CommandId::new(identity(b"command", metadata.artifact_id())).map_err(|_| corrupt())?;
    let request_digest = sha256(&payload);
    match journal.resolve_command(command, request_digest).map_err(storage)? {
        CommandResolution::Committed(_) => return Err(corrupt()), // Committed claim must have its state row.
        CommandResolution::Conflict { .. } => return Err(unauthorized()),
        CommandResolution::DefinitelyAbsent => {}
    }
    let aggregate = AggregateKey::new(
        AggregateKind::Application,
        AggregateId::new(identity(b"aggregate", metadata.artifact_id())).map_err(|_| corrupt())?,
    );
    let frame = ExactFrame::new(
        encode_frame(FRAME_FAMILY, 1, &payload, CodecLimits::PRODUCTION).map_err(|_| corrupt())?,
    )
    .map_err(storage)?;
    let event = EventDraft::new(
        aggregate,
        EventSequence::first(),
        EventId::new(identity(b"event", metadata.artifact_id())).map_err(|_| corrupt())?,
        None,
        frame,
        request_digest,
        Vec::new(),
    )
    .map_err(storage)?;
    let install = StateInstall::new(
        NAMESPACE,
        metadata.artifact_id().as_bytes().to_vec(),
        None,
        1,
        claim.clone(),
    )
    .map_err(storage)?;
    let plan = AppendRequest::new(
        journal.store_id(),
        command,
        request_digest,
        vec![HeadExpectation::Absent(aggregate)],
        vec![event],
        vec![install],
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .plan()
    .map_err(storage)?;
    // Resolve an indeterminate append by the exact claim identity before returning acceptance.
    let append = journal.append(plan);
    if let CommandResolution::Committed(batch) =
        journal.resolve_command(command, request_digest).map_err(storage)?
    {
        let row = journal
            .state_record(NAMESPACE, metadata.artifact_id().as_bytes())
            .map_err(storage)?
            .ok_or_else(corrupt)?;
        if row.revision() == 1
            && row.bytes() == claim
            && row.producing_position() == batch.last_position()
        {
            return Ok(());
        }
        return Err(corrupt());
    }
    append.map_err(storage)?;
    Err(corrupt())
}

pub(super) fn authorize(
    journal: &SqliteJournal,
    scope: ArtifactScope,
    catalog: &peritus_journal::ApplicationArtifact,
) -> Result<(), DaemonError> {
    if claimed_scope(journal, catalog.artifact_id())? != Some(scope) {
        return Err(unauthorized());
    }
    let row = read_claim(journal, catalog.artifact_id())?.ok_or_else(corrupt)?;
    let digest = fingerprint(
        catalog.artifact_id(),
        catalog.byte_size(),
        catalog.digest(),
        catalog.media_type(),
    );
    if row.bytes().get(48..) != Some(digest.as_bytes().as_slice()) {
        return Err(corrupt());
    }
    Ok(())
}

fn read_claim(
    journal: &SqliteJournal,
    artifact: ArtifactId,
) -> Result<Option<peritus_journal::DurableStateRecord>, DaemonError> {
    let aggregate = AggregateKey::new(
        AggregateKind::Application,
        AggregateId::new(identity(b"aggregate", artifact)).map_err(|_| corrupt())?,
    );
    let (records, row) = journal
        .aggregate_checkpoint_snapshot(aggregate, NAMESPACE, artifact.as_bytes())
        .map_err(storage)?
        .into_parts();
    let Some(row) = row else {
        if records.is_empty() {
            return Ok(None);
        }
        return Err(corrupt());
    };
    if records.len() != 1 || row.revision() != 1 || row.bytes().len() != CLAIM_BYTES {
        return Err(corrupt());
    }
    let record = &records[0];
    let frame =
        decode_frame(record.frame_bytes(), CodecLimits::PRODUCTION).map_err(|_| corrupt())?;
    if row.producing_position() != record.global_position()
        || frame.header().family() != FRAME_FAMILY
        || frame.header().schema_version() != 1
        || frame.payload().get(..16) != Some(artifact.as_bytes().as_slice())
        || frame.payload().get(16..) != Some(row.bytes())
    {
        return Err(corrupt());
    }
    Ok(Some(row))
}

fn fingerprint(artifact: ArtifactId, size: u64, digest: Sha256Digest, media: &str) -> Sha256Digest {
    let mut bytes = b"peritus/workbench/artifact-metadata/v1\0".to_vec();
    bytes.extend_from_slice(artifact.as_bytes());
    bytes.extend_from_slice(&size.to_be_bytes());
    bytes.extend_from_slice(digest.as_bytes());
    bytes.extend_from_slice(media.as_bytes());
    sha256(&bytes)
}
fn identity(kind: &[u8], artifact: ArtifactId) -> [u8; 16] {
    let mut bytes = b"peritus/workbench/artifact-scope/v1\0".to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(artifact.as_bytes());
    let digest = sha256(&bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 1;
    id
}
fn array(bytes: &[u8]) -> Result<[u8; 16], DaemonError> {
    bytes.try_into().map_err(|_| corrupt())
}
pub(super) fn unauthorized() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::Unauthorized,
        DaemonRecovery::CorrectRequest,
        "authorize attachment scope",
        "artifact is not owned by the selected actor, conversation and workspace",
    )
}
fn corrupt() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "read attachment scope",
        "durable artifact scope claim is inconsistent",
    )
}
fn storage(error: peritus_journal::JournalError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "retain attachment scope",
        "artifact scope journal is unavailable",
        error,
    )
}

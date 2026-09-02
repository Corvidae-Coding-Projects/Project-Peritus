//! Real lease persistence recovery on both sides of the journal commit.

use peritus_codec::{CodecLimits, encode_frame};
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, LeaseCommitRequest, SqliteJournal, StoreId,
};
use peritus_leases::{LeaseAggregate, LeasePhase, LeaseScope, MintLease};
use peritus_policy::AuthorityInstant;
use peritus_types::{
    CommandId, EnvironmentId, EventId, EventSequence, Generation, ResourceId, Sha256Digest,
    WorkspaceId,
};

use crate::{DaemonConfig, DaemonError};

use super::{
    acquire_instance, digest, digest_hex, identifier, journal_error, open_journal,
    qualification_error,
};

const LEASE_NAMESPACE: u16 = 103;
const FRAME_FAMILY: u16 = 65_004;

/// Checkpoint holding an exact production lease commit request only in process memory.
pub struct LeaseBeforeCheckpoint {
    request_sha256: String,
    _unsubmitted: LeaseCommitRequest,
}

impl LeaseBeforeCheckpoint {
    pub(crate) fn request_sha256(&self) -> &str {
        &self.request_sha256
    }
}

/// Durable facts observed immediately after the lease transition commit.
pub struct LeaseAfterCheckpoint {
    request_sha256: String,
    state_sha256: String,
    state_revision: u64,
    producing_position: u64,
}

impl LeaseAfterCheckpoint {
    pub(crate) fn request_sha256(&self) -> &str {
        &self.request_sha256
    }

    pub(crate) fn state_sha256(&self) -> &str {
        &self.state_sha256
    }

    pub(crate) const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    pub(crate) const fn producing_position(&self) -> u64 {
        self.producing_position
    }
}

/// Exact lease projection and journal facts recovered by a fresh daemon process.
pub struct LeaseCrashQualification {
    request_sha256: String,
    state_sha256: Option<String>,
    state_revision: Option<u64>,
    producing_position: Option<u64>,
    committed_events: u64,
    aggregate_heads: u64,
}

impl LeaseCrashQualification {
    pub(crate) fn request_sha256(&self) -> &str {
        &self.request_sha256
    }

    pub(crate) fn state_sha256(&self) -> Option<&str> {
        self.state_sha256.as_deref()
    }

    pub(crate) const fn state_revision(&self) -> Option<u64> {
        self.state_revision
    }

    pub(crate) const fn producing_position(&self) -> Option<u64> {
        self.producing_position
    }

    pub(crate) const fn committed_events(&self) -> u64 {
        self.committed_events
    }

    pub(crate) const fn aggregate_heads(&self) -> u64 {
        self.aggregate_heads
    }

    pub(crate) const fn journal_verified(&self) -> bool {
        true
    }
}

/// Builds a real move-only lease commit request but does not submit it.
pub fn stage_lease_before_crash(
    config: &DaemonConfig,
) -> Result<LeaseBeforeCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let identity = LeaseIdentity::new(store_id)?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    require_empty(&mut journal, &identity)?;
    let unsubmitted = identity.commit_request(store_id)?;
    Ok(LeaseBeforeCheckpoint {
        request_sha256: digest_hex(identity.request_digest),
        _unsubmitted: unsubmitted,
    })
}

/// Commits the production lease transition and returns before caller acknowledgement.
pub fn stage_lease_after_crash(config: &DaemonConfig) -> Result<LeaseAfterCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let identity = LeaseIdentity::new(store_id)?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    require_empty(&mut journal, &identity)?;
    let committed = journal
        .commit_lease_transition(identity.commit_request(store_id)?)
        .map_err(journal_error)?;
    if committed.transition().record().command_id() != identity.command_id
        || committed.transition().next().scope() != identity.scope
        || committed.transition().next().phase() != LeasePhase::Available
        || committed.state_revision() != 1
        || committed.batch().first_position() != 1
        || committed.batch().last_position() != 1
    {
        return Err(qualification_error("committed lease receipt differs from the mint request"));
    }
    Ok(LeaseAfterCheckpoint {
        request_sha256: digest_hex(identity.request_digest),
        state_sha256: digest_hex(committed.state_digest()),
        state_revision: committed.state_revision(),
        producing_position: committed.batch().last_position(),
    })
}

/// Proves that a killed pre-commit lease request left no durable journal or state row.
pub fn recover_lease_before_crash(
    config: &DaemonConfig,
) -> Result<LeaseCrashQualification, DaemonError> {
    recover(config, false)
}

/// Reopens and verifies the exact lease state installed before the killed acknowledgement.
pub fn recover_lease_after_crash(
    config: &DaemonConfig,
) -> Result<LeaseCrashQualification, DaemonError> {
    recover(config, true)
}

fn recover(config: &DaemonConfig, committed: bool) -> Result<LeaseCrashQualification, DaemonError> {
    let store_id = config.store_identity()?;
    let identity = LeaseIdentity::new(store_id)?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let report = journal.integrity_scan().map_err(journal_error)?;
    let state =
        journal.state_record(LEASE_NAMESPACE, &identity.state_key).map_err(journal_error)?;
    let expected = u64::from(committed);
    if report.event_count() != expected
        || report.aggregate_count() != expected
        || report.last_position() != expected
        || journal.head(identity.aggregate).map_err(journal_error)?.is_some() != committed
        || state.is_some() != committed
    {
        return Err(qualification_error("recovered lease state differs from the commit boundary"));
    }
    if let Some(record) = &state
        && (record.namespace() != LEASE_NAMESPACE
            || record.key() != identity.state_key
            || record.revision() != 1
            || record.producing_position() != 1
            || record.bytes().is_empty())
    {
        return Err(qualification_error("recovered lease projection identity is inconsistent"));
    }
    Ok(LeaseCrashQualification {
        request_sha256: digest_hex(identity.request_digest),
        state_sha256: state.as_ref().map(|record| digest_hex(record.digest())),
        state_revision: state.as_ref().map(peritus_journal::DurableStateRecord::revision),
        producing_position: state
            .as_ref()
            .map(peritus_journal::DurableStateRecord::producing_position),
        committed_events: report.event_count(),
        aggregate_heads: report.aggregate_count(),
    })
}

fn require_empty(journal: &mut SqliteJournal, identity: &LeaseIdentity) -> Result<(), DaemonError> {
    let report = journal.integrity_scan().map_err(journal_error)?;
    let state =
        journal.state_record(LEASE_NAMESPACE, &identity.state_key).map_err(journal_error)?;
    if report.event_count() != 0
        || report.aggregate_count() != 0
        || report.last_position() != 0
        || journal.head(identity.aggregate).map_err(journal_error)?.is_some()
        || state.is_some()
    {
        return Err(qualification_error("lease qualification journal is not empty"));
    }
    Ok(())
}

struct LeaseIdentity {
    aggregate: AggregateKey,
    command_id: CommandId,
    event_id: EventId,
    request_digest: Sha256Digest,
    revision_digest: Sha256Digest,
    scope: LeaseScope,
    state_key: Vec<u8>,
}

impl LeaseIdentity {
    fn new(store_id: StoreId) -> Result<Self, DaemonError> {
        let workspace = WorkspaceId::new(identifier(b"peritus/h1/lease-workspace/v1\0", store_id))
            .map_err(|_| qualification_error("derive lease workspace identity"))?;
        let resource = ResourceId::new(identifier(b"peritus/h1/lease-resource/v1\0", store_id))
            .map_err(|_| qualification_error("derive lease resource identity"))?;
        let environment =
            EnvironmentId::new(identifier(b"peritus/h1/lease-environment/v1\0", store_id))
                .map_err(|_| qualification_error("derive lease environment identity"))?;
        let scope = LeaseScope::new(workspace, resource, environment);
        let mut state_key = Vec::with_capacity(48);
        state_key.extend_from_slice(workspace.as_bytes());
        state_key.extend_from_slice(resource.as_bytes());
        state_key.extend_from_slice(environment.as_bytes());
        Ok(Self {
            aggregate: AggregateKey::new(
                AggregateKind::Lease,
                AggregateId::new(identifier(b"peritus/h1/lease-aggregate/v1\0", store_id))
                    .map_err(journal_error)?,
            ),
            command_id: CommandId::new(identifier(b"peritus/h1/lease-command/v1\0", store_id))
                .map_err(|_| qualification_error("derive lease command identity"))?,
            event_id: EventId::new(identifier(b"peritus/h1/lease-event/v1\0", store_id))
                .map_err(|_| qualification_error("derive lease event identity"))?,
            request_digest: digest(b"peritus/h1/lease-request/v1\0", store_id),
            revision_digest: digest(b"peritus/h1/lease-revision/v1\0", store_id),
            scope,
            state_key,
        })
    }

    fn commit_request(&self, store_id: StoreId) -> Result<LeaseCommitRequest, DaemonError> {
        let frame = ExactFrame::new(
            encode_frame(FRAME_FAMILY, 1, &self.state_key, CodecLimits::PRODUCTION)
                .map_err(|_| qualification_error("encode lease qualification event"))?,
        )
        .map_err(journal_error)?;
        let event = EventDraft::new(
            self.aggregate,
            EventSequence::first(),
            self.event_id,
            None,
            frame,
            self.revision_digest,
            Vec::new(),
        )
        .map_err(journal_error)?;
        let append = AppendRequest::new(
            store_id,
            self.command_id,
            self.request_digest,
            vec![HeadExpectation::Absent(self.aggregate)],
            vec![event],
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
        );
        let transition = LeaseAggregate::mint(MintLease::new(
            self.command_id,
            self.scope,
            AuthorityInstant::new(Generation::first(), 1),
        ))
        .map_err(|_| qualification_error("construct lease mint transition"))?;
        LeaseCommitRequest::new(append, transition).map_err(journal_error)
    }
}

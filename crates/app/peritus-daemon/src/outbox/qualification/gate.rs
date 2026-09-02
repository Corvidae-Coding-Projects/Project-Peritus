//! Real D1 gate-state recovery on both sides of its atomic C0 commit.

mod fixture;

use peritus_codec::{CodecLimits, encode_message};
use peritus_gates::{
    GATE_STATE_NAMESPACE, GateCommandFrame, GatePlan, gate_aggregate_key, gate_state_key,
    load_gate_replay,
};
use peritus_journal::{CommittedBatch, SqliteJournal};

use crate::instance::InstanceGuard;
use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{acquire_instance, digest_hex, journal_error, open_journal, qualification_error};
use fixture::{GateFixture, build};

/// Checkpoint holding an accepted D1 transition only in the killed process.
pub struct GateBeforeCheckpoint {
    request_sha256: String,
    plan_sha256: String,
    successor_sha256: String,
    _instance: InstanceGuard,
    _fixture: GateFixture,
}

impl GateBeforeCheckpoint {
    pub(crate) fn request_sha256(&self) -> &str {
        &self.request_sha256
    }
    pub(crate) fn plan_sha256(&self) -> &str {
        &self.plan_sha256
    }
    pub(crate) fn successor_sha256(&self) -> &str {
        &self.successor_sha256
    }
}

/// Exact D1 receipt observed after the event and complete checkpoint committed atomically.
pub struct GateAfterCheckpoint {
    request_sha256: String,
    plan_sha256: String,
    successor_sha256: String,
    checkpoint_sha256: String,
    state_revision: u64,
    producing_position: u64,
    _instance: InstanceGuard,
    _committed: CommittedBatch,
}

impl GateAfterCheckpoint {
    pub(crate) fn request_sha256(&self) -> &str {
        &self.request_sha256
    }
    pub(crate) fn plan_sha256(&self) -> &str {
        &self.plan_sha256
    }
    pub(crate) fn successor_sha256(&self) -> &str {
        &self.successor_sha256
    }
    pub(crate) fn checkpoint_sha256(&self) -> &str {
        &self.checkpoint_sha256
    }
    pub(crate) const fn state_revision(&self) -> u64 {
        self.state_revision
    }
    pub(crate) const fn producing_position(&self) -> u64 {
        self.producing_position
    }
}

/// Exact gate journal and checkpoint facts recovered by a fresh daemon process.
pub struct GateCrashQualification {
    request_sha256: String,
    plan_sha256: String,
    successor_sha256: Option<String>,
    checkpoint_sha256: Option<String>,
    state_revision: Option<u64>,
    producing_position: Option<u64>,
    committed_events: u64,
    aggregate_heads: u64,
}

impl GateCrashQualification {
    pub(crate) fn request_sha256(&self) -> &str {
        &self.request_sha256
    }
    pub(crate) fn plan_sha256(&self) -> &str {
        &self.plan_sha256
    }
    pub(crate) fn successor_sha256(&self) -> Option<&str> {
        self.successor_sha256.as_deref()
    }
    pub(crate) fn checkpoint_sha256(&self) -> Option<&str> {
        self.checkpoint_sha256.as_deref()
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

/// Reduces the exact start command but does not submit its accepted transition.
pub fn stage_gate_before_crash(config: &DaemonConfig) -> Result<GateBeforeCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let fixture = build(store_id)?;
    let instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    require_empty(&mut journal, &fixture.plan)?;
    Ok(GateBeforeCheckpoint {
        request_sha256: request_digest(&fixture)?,
        plan_sha256: digest_hex(fixture.plan.digest()),
        successor_sha256: digest_hex(fixture.transition.state().state_digest()),
        _instance: instance,
        _fixture: fixture,
    })
}

/// Commits the exact production D1 event and checkpoint before caller acknowledgement.
pub fn stage_gate_after_crash(config: &DaemonConfig) -> Result<GateAfterCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let fixture = build(store_id)?;
    let instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    require_empty(&mut journal, &fixture.plan)?;
    let committed =
        peritus_gates::commit_gate_transition(&mut journal, &fixture.command, &fixture.transition)
            .map_err(gate_error)?;
    let observation = observe(&mut journal, &fixture.plan, true)?;
    if committed.records().len() != 1
        || committed.first_position() != 1
        || committed.last_position() != 1
        || observation.successor_sha256.as_deref()
            != Some(digest_hex(fixture.transition.state().state_digest()).as_str())
    {
        return Err(qualification_error("committed gate receipt differs from its transition"));
    }
    Ok(GateAfterCheckpoint {
        request_sha256: request_digest(&fixture)?,
        plan_sha256: digest_hex(fixture.plan.digest()),
        successor_sha256: observation
            .successor_sha256
            .ok_or_else(|| qualification_error("committed gate successor is absent"))?,
        checkpoint_sha256: observation
            .checkpoint_sha256
            .ok_or_else(|| qualification_error("committed gate checkpoint is absent"))?,
        state_revision: observation
            .state_revision
            .ok_or_else(|| qualification_error("committed gate revision is absent"))?,
        producing_position: observation
            .producing_position
            .ok_or_else(|| qualification_error("committed gate position is absent"))?,
        _instance: instance,
        _committed: committed,
    })
}

pub fn recover_gate_before_crash(
    config: &DaemonConfig,
) -> Result<GateCrashQualification, DaemonError> {
    recover(config, false)
}

pub fn recover_gate_after_crash(
    config: &DaemonConfig,
) -> Result<GateCrashQualification, DaemonError> {
    recover(config, true)
}

fn recover(config: &DaemonConfig, committed: bool) -> Result<GateCrashQualification, DaemonError> {
    let store_id = config.store_identity()?;
    let fixture = build(store_id)?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let observation = observe(&mut journal, &fixture.plan, committed)?;
    Ok(GateCrashQualification {
        request_sha256: request_digest(&fixture)?,
        plan_sha256: digest_hex(fixture.plan.digest()),
        successor_sha256: observation.successor_sha256,
        checkpoint_sha256: observation.checkpoint_sha256,
        state_revision: observation.state_revision,
        producing_position: observation.producing_position,
        committed_events: observation.committed_events,
        aggregate_heads: observation.aggregate_heads,
    })
}

struct Observation {
    successor_sha256: Option<String>,
    checkpoint_sha256: Option<String>,
    state_revision: Option<u64>,
    producing_position: Option<u64>,
    committed_events: u64,
    aggregate_heads: u64,
}

fn observe(
    journal: &mut SqliteJournal,
    plan: &GatePlan,
    committed: bool,
) -> Result<Observation, DaemonError> {
    let report = journal.integrity_scan().map_err(journal_error)?;
    let state = journal
        .state_record(GATE_STATE_NAMESPACE, &gate_state_key(plan.run_id()))
        .map_err(journal_error)?;
    let head = journal
        .head(gate_aggregate_key(plan.run_id()).map_err(gate_error)?)
        .map_err(journal_error)?;
    let replay = load_gate_replay(journal, plan.run_id()).map_err(gate_error)?;
    let rebuilt = replay.rebuild(plan).map_err(gate_error)?;
    let expected = u64::from(committed);
    if report.event_count() != expected
        || report.aggregate_count() != expected
        || report.last_position() != expected
        || state.is_some() != committed
        || head.is_some() != committed
        || rebuilt.is_some() != committed
    {
        return Err(qualification_error("recovered gate state differs from the commit boundary"));
    }
    if let (Some(record), Some(state)) = (&state, &rebuilt)
        && (record.revision() != state.sequence().get()
            || record.producing_position() != report.last_position()
            || state.plan_digest() != plan.digest())
    {
        return Err(qualification_error("recovered gate checkpoint identity is inconsistent"));
    }
    Ok(Observation {
        successor_sha256: rebuilt.as_ref().map(|state| digest_hex(state.state_digest())),
        checkpoint_sha256: state.as_ref().map(|record| digest_hex(record.digest())),
        state_revision: state.as_ref().map(peritus_journal::DurableStateRecord::revision),
        producing_position: state
            .as_ref()
            .map(peritus_journal::DurableStateRecord::producing_position),
        committed_events: report.event_count(),
        aggregate_heads: report.aggregate_count(),
    })
}

fn require_empty(journal: &mut SqliteJournal, plan: &GatePlan) -> Result<(), DaemonError> {
    observe(journal, plan, false).map(|_| ())
}

fn request_digest(fixture: &GateFixture) -> Result<String, DaemonError> {
    let bytes =
        encode_message(&GateCommandFrame::from_command(&fixture.command), CodecLimits::PRODUCTION)
            .map_err(|_| qualification_error("encode gate qualification command"))?;
    Ok(digest_hex(peritus_codec::sha256(&bytes)))
}

fn gate_error(error: peritus_gates::GateError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "qualify gate commit recovery",
        error.to_string(),
        error,
    )
}

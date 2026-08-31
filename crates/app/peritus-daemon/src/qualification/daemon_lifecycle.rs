//! Real staged-daemon kill checkpoints over durable E0/C0 lifecycle state.

use peritus_orchestrator::qualification::{LifecycleFixture, LifecyclePhase};
use peritus_orchestrator::{OrchestratorPhase, commit_orchestrator_transition};
use peritus_types::RunId;

use crate::instance::InstanceGuard;
use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

use super::{acquire_instance, journal_error, open_journal};

/// Durable checkpoint retained while the staged daemon waits to be killed.
pub struct DaemonLifecycleCheckpoint {
    phase: LifecyclePhase,
    run_id: RunId,
    state_sha256: String,
    committed_events: u64,
    active_children: u16,
    _instance: InstanceGuard,
}

impl DaemonLifecycleCheckpoint {
    pub(crate) const fn phase(&self) -> LifecyclePhase {
        self.phase
    }
    pub(crate) const fn run_id(&self) -> RunId {
        self.run_id
    }
    pub(crate) fn state_sha256(&self) -> &str {
        &self.state_sha256
    }
    pub(crate) const fn committed_events(&self) -> u64 {
        self.committed_events
    }
    pub(crate) const fn active_children(&self) -> u16 {
        self.active_children
    }
}

/// Fresh-process replay facts for one daemon lifecycle checkpoint.
pub struct DaemonLifecycleQualification {
    phase: LifecyclePhase,
    run_id: RunId,
    state_sha256: String,
    committed_events: u64,
    aggregate_heads: u64,
    ownership: LifecycleOwnership,
    checkpoint: LifecycleCheckpointTruth,
}

struct LifecycleOwnership {
    active_children: u16,
    pending_directive: bool,
    open_handoff: bool,
}

struct LifecycleCheckpointTruth {
    proposed_candidate: bool,
    acceptance_certificate: bool,
}

impl DaemonLifecycleQualification {
    pub(crate) const fn phase(&self) -> LifecyclePhase {
        self.phase
    }
    pub(crate) const fn run_id(&self) -> RunId {
        self.run_id
    }
    pub(crate) fn state_sha256(&self) -> &str {
        &self.state_sha256
    }
    pub(crate) const fn committed_events(&self) -> u64 {
        self.committed_events
    }
    pub(crate) const fn aggregate_heads(&self) -> u64 {
        self.aggregate_heads
    }
    pub(crate) const fn active_children(&self) -> u16 {
        self.ownership.active_children
    }
    pub(crate) const fn pending_directive(&self) -> bool {
        self.ownership.pending_directive
    }
    pub(crate) const fn open_handoff(&self) -> bool {
        self.ownership.open_handoff
    }
    pub(crate) const fn proposed_candidate(&self) -> bool {
        self.checkpoint.proposed_candidate
    }
    pub(crate) const fn acceptance_certificate(&self) -> bool {
        self.checkpoint.acceptance_certificate
    }
}

/// Commits the shortest production reducer prefix to `phase` and retains instance ownership.
pub fn stage_daemon_lifecycle(
    config: &DaemonConfig,
    phase: LifecyclePhase,
) -> Result<DaemonLifecycleCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let fixture = LifecycleFixture::build(phase).map_err(fixture_error)?;
    for (command, transition) in fixture.steps() {
        commit_orchestrator_transition(&mut journal, command, transition)
            .map_err(orchestrator_error)?;
    }
    let report = journal.integrity_scan().map_err(journal_error)?;
    let state = fixture.state();
    let committed_events = u64::try_from(fixture.steps().len())
        .map_err(|_| fixture_error("lifecycle event count exceeds the journal range"))?;
    if report.event_count() != committed_events || report.aggregate_count() != 1 {
        return Err(fixture_error("committed lifecycle prefix differs from C0 integrity facts"));
    }
    Ok(DaemonLifecycleCheckpoint {
        phase,
        run_id: state.binding().run_id(),
        state_sha256: digest_hex(state.state_digest().as_bytes()),
        committed_events,
        active_children: child_count(state.active_children().len())?,
        _instance: instance,
    })
}

/// Reopens and exactly replays one killed daemon's durable E0 lifecycle prefix.
pub fn recover_daemon_lifecycle(
    config: &DaemonConfig,
    phase: LifecyclePhase,
) -> Result<DaemonLifecycleQualification, DaemonError> {
    let store_id = config.store_identity()?;
    let _instance = acquire_instance(config, store_id)?;
    let mut journal = open_journal(config, store_id)?;
    let expected = LifecycleFixture::build(phase).map_err(fixture_error)?;
    let run_id = expected.state().binding().run_id();
    let replay = peritus_orchestrator::load_orchestrator_replay(&journal, run_id)
        .map_err(orchestrator_error)?;
    let state = replay
        .rebuild()
        .map_err(orchestrator_error)?
        .ok_or_else(|| fixture_error("killed daemon lifecycle has no durable state"))?;
    let report = journal.integrity_scan().map_err(journal_error)?;
    if state != *expected.state()
        || state.phase() != OrchestratorPhase::Active(phase_active(phase))
        || report.event_count() != state.sequence().get()
        || report.aggregate_count() != 1
    {
        return Err(fixture_error(
            "fresh daemon replay differs from the exact killed lifecycle checkpoint",
        ));
    }
    Ok(DaemonLifecycleQualification {
        phase,
        run_id,
        state_sha256: digest_hex(state.state_digest().as_bytes()),
        committed_events: report.event_count(),
        aggregate_heads: report.aggregate_count(),
        ownership: LifecycleOwnership {
            active_children: child_count(state.active_children().len())?,
            pending_directive: state.pending_directive().is_some(),
            open_handoff: state.open_handoff().is_some(),
        },
        checkpoint: LifecycleCheckpointTruth {
            proposed_candidate: state.proposed_candidate().is_some(),
            acceptance_certificate: state.acceptance_certificate().is_some(),
        },
    })
}

const fn phase_active(phase: LifecyclePhase) -> peritus_orchestrator::ActivePhase {
    use peritus_orchestrator::ActivePhase;
    match phase {
        LifecyclePhase::WriterPending => ActivePhase::WriterPending,
        LifecyclePhase::WriterActive => ActivePhase::WriterActive,
        LifecyclePhase::GatesPending => ActivePhase::GatesPending,
        LifecyclePhase::GatesActive => ActivePhase::GatesActive,
        LifecyclePhase::ReviewPending => ActivePhase::ReviewPending,
        LifecyclePhase::ReviewActive => ActivePhase::ReviewActive,
        LifecyclePhase::FixerPending => ActivePhase::FixerPending,
        LifecyclePhase::FixerActive => ActivePhase::FixerActive,
        LifecyclePhase::RevisionAdvancing => ActivePhase::RevisionAdvancing,
        LifecyclePhase::EvaluatingAcceptance => ActivePhase::EvaluatingAcceptance,
        LifecyclePhase::KernelAcceptancePending => ActivePhase::KernelAcceptancePending,
    }
}

fn child_count(value: usize) -> Result<u16, DaemonError> {
    u16::try_from(value).map_err(|_| fixture_error("active child count exceeds H1 range"))
}

fn digest_hex(bytes: &[u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes {
        use core::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn fixture_error(detail: impl Into<String>) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "qualify daemon lifecycle replay",
        detail,
    )
}

fn orchestrator_error(error: peritus_orchestrator::OrchestratorError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::ReadOnly,
        "qualify daemon lifecycle replay",
        error.detail(),
        error,
    )
}

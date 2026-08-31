//! Real-host reboot checkpoints around one durable C0 outbox delivery.

use std::path::{Path, PathBuf};

use peritus_journal::SqliteJournal;

use super::{
    QualificationDestination, QualificationIdentity, qualification_error, seed, validate_claim,
};
use crate::qualification::{acquire_instance, journal_error, open_journal};
use crate::{DaemonConfig, DaemonError};

const INITIAL_NOW: u64 = 1;
const INITIAL_LEASE: u64 = 2;
const RECOVERY_NOW: u64 = 2;
const RECOVERY_LEASE: u64 = 3;
const FINAL_NOW: u64 = 3;
const FINAL_LEASE: u64 = 4;

/// Exact production phase interrupted by a disposable host reboot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostRebootPhase {
    /// C0 owns a live delivery claim, but its external effect has not run.
    OutstandingEffect,
    /// The effect exists durably, but its live claim is not acknowledged.
    DurableBeforeAck,
    /// Startup has reclaimed and reconciled the effect, but has not acknowledged it.
    StartupReconciliation,
}

impl HostRebootPhase {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::OutstandingEffect => "outstanding-effect",
            Self::DurableBeforeAck => "durable-before-ack",
            Self::StartupReconciliation => "startup-reconciliation",
        }
    }
}

/// Durable checkpoint emitted immediately before the external host cuts power.
pub struct HostRebootCheckpoint {
    phase: HostRebootPhase,
    effect_path: PathBuf,
    claim_fence: u64,
    external_effects: u64,
}

impl HostRebootCheckpoint {
    pub(crate) const fn phase(&self) -> HostRebootPhase {
        self.phase
    }
    pub(crate) fn effect_path(&self) -> &Path {
        &self.effect_path
    }
    pub(crate) const fn claim_fence(&self) -> u64 {
        self.claim_fence
    }
    pub(crate) const fn external_effects(&self) -> u64 {
        self.external_effects
    }
}

/// Fresh-boot recovery facts after the exact interrupted delivery is settled.
pub struct HostRebootObservation {
    phase: HostRebootPhase,
    destination_reconciled: bool,
    external_effects: u64,
    duplicate_effects: u64,
    exact_fence_acknowledged: bool,
    pending_claims: u64,
}

impl HostRebootObservation {
    pub(crate) const fn phase(&self) -> HostRebootPhase {
        self.phase
    }
    pub(crate) const fn destination_reconciled(&self) -> bool {
        self.destination_reconciled
    }
    pub(crate) const fn external_effects(&self) -> u64 {
        self.external_effects
    }
    pub(crate) const fn duplicate_effects(&self) -> u64 {
        self.duplicate_effects
    }
    pub(crate) const fn exact_fence_acknowledged(&self) -> bool {
        self.exact_fence_acknowledged
    }
    pub(crate) const fn pending_claims(&self) -> u64 {
        self.pending_claims
    }
}

/// Seeds and claims one production delivery at the selected pre-reboot checkpoint.
pub fn stage_host_reboot(
    config: &DaemonConfig,
    phase: HostRebootPhase,
) -> Result<HostRebootCheckpoint, DaemonError> {
    if phase == HostRebootPhase::StartupReconciliation {
        return stage_initial_reconciliation(config);
    }
    let store_id = config.store_identity()?;
    let identity = QualificationIdentity::new(store_id)?;
    let _instance = acquire_instance(config, store_id)?;
    let destination = QualificationDestination::prepare(
        config.paths().state_root(),
        identity.outbox_id,
        &identity.payload,
    )?;
    let mut journal = open_journal(config, store_id)?;
    seed(&mut journal, &identity)?;
    let fence = claim(&mut journal, &identity, INITIAL_NOW, INITIAL_LEASE, 1)?;
    let external_effects = if phase == HostRebootPhase::DurableBeforeAck {
        destination.apply_once(&identity.payload)?;
        require_one_effect(&destination, &identity)?;
        1
    } else {
        0
    };
    Ok(HostRebootCheckpoint {
        phase,
        effect_path: destination.effect_path().to_path_buf(),
        claim_fence: fence,
        external_effects,
    })
}

/// Reclaims and reconciles startup work, then pauses before acknowledgement for a second reboot.
pub fn stage_startup_reconciliation(
    config: &DaemonConfig,
) -> Result<HostRebootCheckpoint, DaemonError> {
    let store_id = config.store_identity()?;
    let identity = QualificationIdentity::new(store_id)?;
    let _instance = acquire_instance(config, store_id)?;
    let destination = QualificationDestination::prepare(
        config.paths().state_root(),
        identity.outbox_id,
        &identity.payload,
    )?;
    let mut journal = open_journal(config, store_id)?;
    let fence = claim(&mut journal, &identity, RECOVERY_NOW, RECOVERY_LEASE, 2)?;
    require_one_effect(&destination, &identity)?;
    Ok(HostRebootCheckpoint {
        phase: HostRebootPhase::StartupReconciliation,
        effect_path: destination.effect_path().to_path_buf(),
        claim_fence: fence,
        external_effects: 1,
    })
}

/// Reopens the production stores after reboot and settles the exact outstanding delivery.
pub fn recover_host_reboot(
    config: &DaemonConfig,
    phase: HostRebootPhase,
) -> Result<HostRebootObservation, DaemonError> {
    let store_id = config.store_identity()?;
    let identity = QualificationIdentity::new(store_id)?;
    let _instance = acquire_instance(config, store_id)?;
    let destination = QualificationDestination::prepare(
        config.paths().state_root(),
        identity.outbox_id,
        &identity.payload,
    )?;
    let mut journal = open_journal(config, store_id)?;
    let (now, lease, attempt) = if phase == HostRebootPhase::StartupReconciliation {
        (FINAL_NOW, FINAL_LEASE, 3)
    } else {
        (RECOVERY_NOW, RECOVERY_LEASE, 2)
    };
    let fence = claim(&mut journal, &identity, now, lease, attempt)?;
    if phase == HostRebootPhase::OutstandingEffect {
        destination.apply_once(&identity.payload)?;
    }
    let observed = destination.reconcile(&identity.payload)?;
    if observed.external_effects() != 1 || observed.duplicate_effects() != 0 {
        return Err(qualification_error("reboot recovery did not conserve the exact effect"));
    }
    journal.acknowledge_outbox(identity.outbox_id, fence).map_err(journal_error)?;
    let pending_claims =
        u64::from(journal.claim_outbox(lease, lease + 1).map_err(journal_error)?.is_some());
    Ok(HostRebootObservation {
        phase,
        destination_reconciled: true,
        external_effects: observed.external_effects(),
        duplicate_effects: observed.duplicate_effects(),
        exact_fence_acknowledged: true,
        pending_claims,
    })
}

fn stage_initial_reconciliation(
    config: &DaemonConfig,
) -> Result<HostRebootCheckpoint, DaemonError> {
    let mut checkpoint = stage_host_reboot(config, HostRebootPhase::DurableBeforeAck)?;
    checkpoint.phase = HostRebootPhase::StartupReconciliation;
    Ok(checkpoint)
}

fn claim(
    journal: &mut SqliteJournal,
    identity: &QualificationIdentity,
    now: u64,
    lease: u64,
    attempt: u16,
) -> Result<u64, DaemonError> {
    let message = journal
        .claim_outbox(now, lease)
        .map_err(journal_error)?
        .ok_or_else(|| qualification_error("reboot qualification delivery was not claimable"))?;
    validate_claim(&message, identity, attempt)
}

fn require_one_effect(
    destination: &QualificationDestination,
    identity: &QualificationIdentity,
) -> Result<(), DaemonError> {
    let observed = destination.reconcile(&identity.payload)?;
    if observed.external_effects() == 1 && observed.duplicate_effects() == 0 {
        Ok(())
    } else {
        Err(qualification_error("reboot checkpoint does not contain the exact effect"))
    }
}

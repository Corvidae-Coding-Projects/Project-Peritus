//! Durable one-use authorization consumption and process registry.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use peritus_leases::LeaseClaim;
use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    ExecutionPlan, LifecyclePhase, OsExitObservation, ProcessControl, ProcessError, StopTrigger,
    platform::ProcessTreeIdentity,
    recovery::{claim::ConsumptionClaim, manifest::ExecutionManifest},
    registry_storage::{
        create_checked_directory, hex, load_claims, load_manifests, load_quarantine, persist_claim,
        restore_backups, write_manifest,
    },
};

mod errors;
mod retention;
mod state;
mod terminal_store;
#[cfg(test)]
mod tests;
mod watchdog;

use errors::{overlap_error, reused, store_error};
use retention::{execution_record_count, retire_settled_records};
use watchdog::configured_crash_watchdog;

const MAX_EXECUTION_RECORDS: usize = 16_384;
const RETIRE_BATCH: usize = 1_024;

struct StoreState {
    manifests: BTreeMap<ProcessId, ExecutionManifest>,
    claims: BTreeMap<ProcessId, ConsumptionClaim>,
    quarantined_records: Vec<PathBuf>,
    controls: BTreeMap<ProcessId, ProcessControl>,
}

struct StoreInner {
    root: PathBuf,
    manifests: PathBuf,
    claims: PathBuf,
    spools: PathBuf,
    crash_watchdog: Option<PathBuf>,
    state: Mutex<StoreState>,
}

/// Cloneable handle to the protected durable execution registry.
#[derive(Clone)]
pub struct ProcessStore {
    inner: Arc<StoreInner>,
}

impl ProcessStore {
    /// Opens or initializes a registry outside the named agent-visible workspace.
    ///
    /// Corrupt manifests are moved into the registry's quarantine directory. A durable claim with
    /// no manifest remains consumed and is reported as an indeterminate recovery record.
    ///
    /// # Errors
    ///
    /// Returns a typed error when roots overlap, the layout is unsafe, or durable records cannot
    /// be inspected.
    pub fn open(
        root: impl AsRef<Path>,
        agent_workspace_root: impl AsRef<Path>,
    ) -> Result<Self, ProcessError> {
        Self::open_configured(root.as_ref(), agent_workspace_root.as_ref(), None)
    }

    /// Opens a registry whose launched process groups are guarded by an installed crash watchdog.
    ///
    /// The watchdog is an independent process that retains only an owner-liveness channel and an
    /// exact process birth identity. If this process dies without disarming it, the watchdog
    /// terminates the exact owned process group before exiting.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the watchdog is missing, aliased, non-executable, or overlaps
    /// the protected process registry or agent-visible workspace.
    #[cfg(target_os = "linux")]
    pub fn open_with_crash_watchdog(
        root: impl AsRef<Path>,
        agent_workspace_root: impl AsRef<Path>,
        crash_watchdog: impl AsRef<Path>,
    ) -> Result<Self, ProcessError> {
        Self::open_configured(
            root.as_ref(),
            agent_workspace_root.as_ref(),
            Some(crash_watchdog.as_ref()),
        )
    }

    fn open_configured(
        root: &Path,
        agent_workspace_root: &Path,
        crash_watchdog: Option<&Path>,
    ) -> Result<Self, ProcessError> {
        std::fs::create_dir_all(root)
            .map_err(|_| store_error("process registry root cannot be created"))?;
        let root = std::fs::canonicalize(root)
            .map_err(|_| store_error("process registry root cannot be canonicalized"))?;
        let workspace = std::fs::canonicalize(agent_workspace_root)
            .map_err(|_| store_error("agent workspace root cannot be canonicalized"))?;
        if root.starts_with(&workspace) || workspace.starts_with(&root) {
            return Err(overlap_error());
        }
        let manifests = root.join("manifests-v1");
        let claims = root.join("claims-v1");
        let spools = root.join("spools-v1");
        let quarantine = root.join("quarantine-v1");
        for directory in [&manifests, &claims, &spools, &quarantine] {
            create_checked_directory(&root, directory)?;
        }
        restore_backups(&manifests)?;
        let mut state = StoreState {
            manifests: BTreeMap::new(),
            claims: BTreeMap::new(),
            quarantined_records: load_quarantine(&quarantine)?,
            controls: BTreeMap::new(),
        };
        load_claims(&claims, &quarantine, &mut state.claims, &mut state.quarantined_records)?;
        load_manifests(
            &manifests,
            &quarantine,
            &mut state.manifests,
            &mut state.quarantined_records,
        )?;
        retire_settled_records(
            &claims,
            &manifests,
            &spools,
            &mut state,
            MAX_EXECUTION_RECORDS.saturating_sub(RETIRE_BATCH),
        )?;
        if execution_record_count(&state) > MAX_EXECUTION_RECORDS {
            return Err(store_error("process registry exceeds its record bound"));
        }
        let crash_watchdog = configured_crash_watchdog(crash_watchdog, &root, &workspace)?;
        Ok(Self {
            inner: Arc::new(StoreInner {
                root,
                manifests,
                claims,
                spools,
                crash_watchdog,
                state: Mutex::new(state),
            }),
        })
    }

    /// Returns the canonical protected registry root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    /// Returns a live control handle only for one exact durable process owner.
    ///
    /// The process, action, and run identities must all match the retained manifest and its
    /// consumption claim. Reopened stores do not synthesize controls for processes they do not
    /// still own in memory.
    #[must_use]
    pub fn control_exact(
        &self,
        run_id: peritus_types::RunId,
        action_id: peritus_types::ActionId,
        process_id: ProcessId,
    ) -> Option<ProcessControl> {
        let (manifests, mut claims) = self.recovery_records();
        let manifest =
            manifests.into_iter().find(|manifest| manifest.identity.process_id() == process_id)?;
        let claim_matches =
            claims.remove(&process_id).is_some_and(|claim| claim.matches_manifest(&manifest));
        if !claim_matches
            || manifest.identity.run_id() != run_id
            || manifest.identity.action_id() != action_id
        {
            return None;
        }
        self.lock_state().controls.get(&process_id).cloned()
    }

    pub(crate) fn retain_control(&self, process_id: ProcessId, control: ProcessControl) {
        self.lock_state().controls.insert(process_id, control);
    }

    pub(crate) fn release_control(&self, process_id: ProcessId) {
        self.lock_state().controls.remove(&process_id);
    }

    pub(crate) fn crash_watchdog(&self) -> Option<&Path> {
        self.inner.crash_watchdog.as_deref()
    }

    pub(crate) fn consume(
        &self,
        plan: &ExecutionPlan,
        action_digest: Sha256Digest,
        lease: Option<LeaseClaim>,
    ) -> Result<(), ProcessError> {
        let identity = plan.identity();
        let process_id = identity.process_id();
        let mut state = self.lock_state();
        if state.manifests.contains_key(&process_id)
            || state.claims.contains_key(&process_id)
            || self.claim_path(process_id).exists()
        {
            return Err(reused());
        }
        if execution_record_count(&state) >= MAX_EXECUTION_RECORDS {
            retire_settled_records(
                &self.inner.claims,
                &self.inner.manifests,
                &self.inner.spools,
                &mut state,
                MAX_EXECUTION_RECORDS.saturating_sub(RETIRE_BATCH),
            )?;
            if execution_record_count(&state) >= MAX_EXECUTION_RECORDS {
                return Err(store_error(
                    "process registry is full of records that still require recovery",
                ));
            }
        }
        let claim = persist_claim(&self.inner.claims, &identity, action_digest, plan.digest())?;
        state.claims.insert(process_id, claim);
        let manifest = ExecutionManifest::authorized(plan, action_digest, lease);
        write_manifest(&self.inner.manifests, &manifest)?;
        state.manifests.insert(process_id, manifest);
        drop(state);
        Ok(())
    }

    pub(crate) fn spool_directory(&self, process_id: ProcessId) -> Result<PathBuf, ProcessError> {
        let directory = self.inner.spools.join(hex(process_id.as_bytes()));
        create_checked_directory(&self.inner.root, &directory)?;
        Ok(directory)
    }

    pub(crate) fn record_phase(
        &self,
        process_id: ProcessId,
        phase: LifecyclePhase,
    ) -> Result<(), ProcessError> {
        self.update(process_id, |manifest| {
            if !legal_manifest_advance(manifest.phase, phase) {
                return Err(store_error("durable process lifecycle transition is illegal"));
            }
            manifest.phase = phase;
            Ok(())
        })
    }

    pub(crate) fn record_started(
        &self,
        process_id: ProcessId,
        tree: ProcessTreeIdentity,
    ) -> Result<(), ProcessError> {
        self.update(process_id, |manifest| {
            if manifest.phase != LifecyclePhase::Starting || manifest.tree.is_some() {
                return Err(store_error("process startup observation is out of sequence"));
            }
            manifest.tree = Some(tree);
            manifest.phase = LifecyclePhase::Running;
            Ok(())
        })
    }

    pub(crate) fn record_stopping(
        &self,
        process_id: ProcessId,
        trigger: StopTrigger,
    ) -> Result<(), ProcessError> {
        self.update(process_id, |manifest| {
            if manifest.trigger.is_none()
                && matches!(manifest.phase, LifecyclePhase::Starting | LifecyclePhase::Running)
            {
                manifest.trigger = Some(trigger);
                manifest.phase = LifecyclePhase::Stopping;
            }
            Ok(())
        })
    }

    pub(crate) fn record_exit(
        &self,
        process_id: ProcessId,
        exit: OsExitObservation,
    ) -> Result<(), ProcessError> {
        self.update(process_id, |manifest| {
            if !matches!(manifest.phase, LifecyclePhase::Running | LifecyclePhase::Stopping) {
                return Err(store_error("process exit observation is out of sequence"));
            }
            manifest.exit = Some(exit);
            manifest.phase = LifecyclePhase::Exited;
            Ok(())
        })
    }

    pub(crate) fn record_spawn_failed(
        &self,
        process_id: ProcessId,
        support_tasks_joined: bool,
    ) -> Result<(), ProcessError> {
        self.update(process_id, |manifest| {
            if manifest.phase != LifecyclePhase::Starting {
                return Err(store_error("spawn failure is out of sequence"));
            }
            manifest.exit = Some(OsExitObservation::Unavailable);
            manifest.tree_quiescent = true;
            manifest.support_tasks_joined = support_tasks_joined;
            manifest.phase = LifecyclePhase::Closed;
            Ok(())
        })
    }

    pub(crate) fn record_closed(
        &self,
        process_id: ProcessId,
        observed: u64,
        retained: u64,
        dropped: u64,
        tree_quiescent: bool,
        support_tasks_joined: bool,
    ) -> Result<(), ProcessError> {
        self.update(process_id, |manifest| {
            if manifest.phase != LifecyclePhase::Exited
                || retained > observed
                || dropped != observed - retained
            {
                return Err(store_error("closed process accounting is inconsistent"));
            }
            manifest.observed_output = observed;
            manifest.retained_output = retained;
            manifest.dropped_output = dropped;
            manifest.tree_quiescent = tree_quiescent;
            manifest.support_tasks_joined = support_tasks_joined;
            manifest.phase = LifecyclePhase::Closed;
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_failed_closed(
        &self,
        process_id: ProcessId,
        exit: OsExitObservation,
        observed: u64,
        retained: u64,
        dropped: u64,
        tree_quiescent: bool,
        support_tasks_joined: bool,
    ) -> Result<(), ProcessError> {
        self.update(process_id, |manifest| {
            if !matches!(
                manifest.phase,
                LifecyclePhase::Starting
                    | LifecyclePhase::Running
                    | LifecyclePhase::Stopping
                    | LifecyclePhase::Exited
            ) || retained > observed
                || dropped != observed - retained
            {
                return Err(store_error(
                    "failed process closure is out of sequence or inconsistent",
                ));
            }
            manifest.exit = Some(exit);
            manifest.observed_output = observed;
            manifest.retained_output = retained;
            manifest.dropped_output = dropped;
            manifest.tree_quiescent = tree_quiescent;
            manifest.support_tasks_joined = support_tasks_joined;
            manifest.phase = LifecyclePhase::Closed;
            Ok(())
        })
    }
}

const fn legal_manifest_advance(before: LifecyclePhase, after: LifecyclePhase) -> bool {
    matches!(
        (before, after),
        (LifecyclePhase::Authorized, LifecyclePhase::Starting)
            | (LifecyclePhase::Closed, LifecyclePhase::Terminal)
    )
}

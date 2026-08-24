//! Durable one-use authorization consumption and process registry.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use peritus_leases::LeaseClaim;
use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    ErrorCode, ExecutionPlan, LifecyclePhase, OsExitObservation, ProcessError, ProcessOperation,
    RecoveryClass, StopTrigger,
    platform::ProcessTreeIdentity,
    recovery::{claim::ConsumptionClaim, manifest::ExecutionManifest},
    registry_storage::{
        create_checked_directory, hex, load_claims, load_manifests, persist_claim, restore_backups,
        write_manifest,
    },
};

mod terminal_store;
#[cfg(test)]
mod tests;

const MAX_EXECUTION_RECORDS: usize = 16_384;

struct StoreState {
    manifests: BTreeMap<ProcessId, ExecutionManifest>,
    claims: BTreeMap<ProcessId, ConsumptionClaim>,
    quarantined_records: Vec<PathBuf>,
}

struct StoreInner {
    root: PathBuf,
    manifests: PathBuf,
    claims: PathBuf,
    spools: PathBuf,
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
        std::fs::create_dir_all(root.as_ref())
            .map_err(|_| store_error("process registry root cannot be created"))?;
        let root = std::fs::canonicalize(root.as_ref())
            .map_err(|_| store_error("process registry root cannot be canonicalized"))?;
        let workspace = std::fs::canonicalize(agent_workspace_root.as_ref())
            .map_err(|_| store_error("agent workspace root cannot be canonicalized"))?;
        if root.starts_with(&workspace) || workspace.starts_with(&root) {
            return Err(ProcessError::new(
                ErrorCode::InvalidInput,
                ProcessOperation::OpenStore,
                RecoveryClass::CorrectRequest,
                "process registry and agent-visible workspace roots overlap",
            ));
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
            quarantined_records: Vec::new(),
        };
        load_claims(&claims, &quarantine, &mut state.claims, &mut state.quarantined_records)?;
        load_manifests(
            &manifests,
            &quarantine,
            &mut state.manifests,
            &mut state.quarantined_records,
        )?;
        if execution_record_count(&state) > MAX_EXECUTION_RECORDS {
            return Err(store_error("process registry exceeds its record bound"));
        }
        Ok(Self {
            inner: Arc::new(StoreInner {
                root,
                manifests,
                claims,
                spools,
                state: Mutex::new(state),
            }),
        })
    }

    /// Returns the canonical protected registry root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    /// Returns paths quarantined while opening this registry.
    #[must_use]
    pub fn quarantined_records(&self) -> Vec<PathBuf> {
        self.lock_state().quarantined_records.clone()
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
            return Err(store_error("process registry exceeds its record bound"));
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

    pub(crate) fn record_spawn_failed(&self, process_id: ProcessId) -> Result<(), ProcessError> {
        self.update(process_id, |manifest| {
            if manifest.phase != LifecyclePhase::Starting {
                return Err(store_error("spawn failure is out of sequence"));
            }
            manifest.exit = Some(OsExitObservation::Unavailable);
            manifest.tree_quiescent = true;
            manifest.support_tasks_joined = true;
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

    pub(crate) fn manifests(&self) -> Vec<ExecutionManifest> {
        self.lock_state().manifests.values().cloned().collect()
    }

    pub(crate) fn recovery_records(
        &self,
    ) -> (Vec<ExecutionManifest>, BTreeMap<ProcessId, ConsumptionClaim>) {
        let state = self.lock_state();
        (state.manifests.values().cloned().collect(), state.claims.clone())
    }

    pub(crate) fn reconcile_manifest(
        &self,
        process_id: ProcessId,
        tree_quiescent: bool,
    ) -> Result<(), ProcessError> {
        self.update(process_id, |manifest| {
            if manifest.phase == LifecyclePhase::Terminal {
                return Ok(());
            }
            manifest.exit = Some(OsExitObservation::Unavailable);
            manifest.tree_quiescent = tree_quiescent;
            manifest.support_tasks_joined = true;
            manifest.phase = LifecyclePhase::Closed;
            Ok(())
        })
    }

    fn update(
        &self,
        process_id: ProcessId,
        update: impl FnOnce(&mut ExecutionManifest) -> Result<(), ProcessError>,
    ) -> Result<(), ProcessError> {
        let mut state = self.lock_state();
        let manifest = state
            .manifests
            .get_mut(&process_id)
            .ok_or_else(|| store_error("process manifest is missing"))?;
        let mut next = manifest.clone();
        update(&mut next)?;
        write_manifest(&self.inner.manifests, &next)?;
        *manifest = next;
        drop(state);
        Ok(())
    }

    fn claim_path(&self, process_id: ProcessId) -> PathBuf {
        self.inner.claims.join(format!("{}.claim", hex(process_id.as_bytes())))
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, StoreState> {
        self.inner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

const fn legal_manifest_advance(before: LifecyclePhase, after: LifecyclePhase) -> bool {
    matches!(
        (before, after),
        (LifecyclePhase::Authorized, LifecyclePhase::Starting)
            | (LifecyclePhase::Closed, LifecyclePhase::Terminal)
    )
}

fn execution_record_count(state: &StoreState) -> usize {
    state.claims.len()
        + state.manifests.keys().filter(|process_id| !state.claims.contains_key(process_id)).count()
}

const fn reused() -> ProcessError {
    ProcessError::new(
        ErrorCode::ReceiptReused,
        ProcessOperation::Authorize,
        RecoveryClass::Reauthorize,
        "action/process authority was already durably consumed",
    )
}

const fn store_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Persistence,
        ProcessOperation::Persist,
        RecoveryClass::ReopenAndReconcile,
        detail,
    )
}

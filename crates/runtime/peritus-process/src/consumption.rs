//! Durable one-use authorization consumption and process registry.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use peritus_leases::LeaseClaim;
use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    ExecutionPlan, LifecyclePhase, OsExitObservation, ProcessError, StopTrigger,
    platform::ProcessTreeIdentity,
    recovery::{claim::RetainedClaimBinding, manifest::ExecutionManifest},
    retained_owner::RetainedOwnerBinding,
    registry_storage::{
        RetainedOwnerTransaction, acquire_retained_owner_transaction,
        backfill_quarantined_identities, create_checked_directory,
        discard_unclaimed_retained_owner_request, hex, load_retained_owner_request,
        load_unclaimed_retained_owner_request, persist_claim, persist_retained_claim,
        restore_backups, restore_retained_consumptions, stage_retained_owner_request,
        write_manifest,
    },
};

mod errors;
mod completion;
mod index;
mod native_observation;
mod retention;
mod state;
mod terminal_store;
#[cfg(test)]
mod tests;
mod watchdog;

use errors::{overlap_error, reused, store_error};
pub(crate) use errors::store_cause;
use index::RegistryIndex;
pub(crate) use index::retirable;
use retention::retire_settled_records;
use watchdog::configured_crash_watchdog;

pub use completion::{
    RetainedCompletionBinding, RetainedOwnerCompletionStatus, RetainedStreamSnapshot,
};
pub use state::{ProcessClaimState, ProcessReceipt};

struct StoreState {
    index: RegistryIndex,
    unresolved_io: bool,
    revision: u64,
    quarantined_records: Vec<PathBuf>,
}

struct StoreInner {
    root: PathBuf,
    manifests: PathBuf,
    claims: PathBuf,
    spools: PathBuf,
    tombstones: PathBuf,
    native_observations: PathBuf,
    retained_owners: PathBuf,
    completion_receipts: PathBuf,
    quarantined_identities: PathBuf,
    crash_watchdog: Option<PathBuf>,
    retained_owner_transport: Option<Arc<dyn crate::RetainedProcessTransport>>,
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
        let tombstones = root.join("tombstones-v1");
        let native_observations = root.join("native-observations-v1");
        let terminal_evidence = root.join("terminal-evidence-v1");
        let retained_owners = root.join("retained-owners-v1");
        let completion_receipts = root.join("owner-completions-v1");
        let quarantined_identities = root.join("quarantined-identities-v1");
        for directory in [
            &manifests,
            &claims,
            &spools,
            &quarantine,
            &tombstones,
            &native_observations,
            &terminal_evidence,
            &retained_owners,
            &completion_receipts,
            &quarantined_identities,
        ] {
            create_checked_directory(&root, directory)?;
        }
        backfill_quarantined_identities(
            &quarantine,
            &quarantined_identities,
            &retained_owners,
        )?;
        restore_backups(&manifests, &retained_owners, &quarantined_identities)?;
        restore_retained_consumptions(
            &retained_owners,
            &claims,
            &manifests,
            &tombstones,
            &quarantine,
            &quarantined_identities,
        )?;
        let (index, quarantined_records) = RegistryIndex::rebuild(&root)?;
        let mut state = StoreState {
            index,
            unresolved_io: false,
            revision: 0,
            quarantined_records,
        };
        // Optional maintenance cannot revoke admission for an unrelated identity.
        if let Err(error) = retire_settled_records(
            &root,
            &claims,
            &manifests,
            &spools,
            &mut state,
            0,
        ) {
            eprintln!("process registry retirement deferred: {error}");
        }
        let crash_watchdog = configured_crash_watchdog(crash_watchdog, &root, &workspace)?;
        Ok(Self {
            inner: Arc::new(StoreInner {
                root,
                manifests,
                claims,
                spools,
                tombstones,
                native_observations,
                retained_owners,
                completion_receipts,
                quarantined_identities,
                crash_watchdog,
                retained_owner_transport: None,
                state: Mutex::new(state),
            }),
        })
    }

    /// Returns the canonical protected registry root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    pub(crate) fn crash_watchdog(&self) -> Option<&Path> {
        self.inner.crash_watchdog.as_deref()
    }

    /// Attaches the independently retained service owner before this store is shared.
    ///
    /// # Errors
    /// Returns a typed invariant failure if a clone already escaped and the transport could not
    /// be installed for every gateway using this store.
    pub fn with_retained_owner(
        mut self,
        transport: Arc<dyn crate::RetainedProcessTransport>,
    ) -> Result<Self, ProcessError> {
        let inner = Arc::get_mut(&mut self.inner).ok_or_else(|| {
            store_error("retained process owner must be attached before the store is shared")
        })?;
        inner.retained_owner_transport = Some(transport);
        Ok(self)
    }

    pub(crate) fn retained_owner_transport(
        &self,
    ) -> Option<Arc<dyn crate::RetainedProcessTransport>> {
        self.inner.retained_owner_transport.clone()
    }

    /// Loads one claim-linked canonical retained-owner request without changing its lifecycle.
    ///
    /// # Errors
    /// Returns a typed persistence or corruption failure when the immutable request, consumption
    /// claim, staged authorization, and current manifest do not identify the same execution.
    pub fn retained_owner_reservation(
        &self,
        process_id: ProcessId,
    ) -> Result<Option<crate::RetainedOwnerReservation>, ProcessError> {
        let transaction = self.retained_owner_transaction(process_id)?;
        self.retained_owner_reservation_in_transaction(&transaction)
    }

    pub(crate) fn retained_owner_reservation_in_transaction(
        &self,
        transaction: &RetainedOwnerTransaction,
    ) -> Result<Option<crate::RetainedOwnerReservation>, ProcessError> {
        let process_id = transaction.process_id();
        let state = self.lock_authoritative_identity(transaction)?;
        let Some(record) = state.index.get(process_id)? else {
            return Ok(None);
        };
        if record.retired || record.claim.is_none() || record.manifest.is_none() {
            return Ok(None);
        }
        load_retained_owner_request(
            &self.inner.retained_owners,
            &self.inner.claims,
            &self.inner.manifests,
            process_id,
        )
        .map(|value| {
            value.map(|(operation, request_digest, request, phase)| {
                crate::RetainedOwnerReservation::new(operation, request_digest, request, phase)
            })
        })
    }

    pub(crate) fn retained_owner_transaction(
        &self,
        process_id: ProcessId,
    ) -> Result<RetainedOwnerTransaction, ProcessError> {
        acquire_retained_owner_transaction(&self.inner.retained_owners, process_id)
    }

    pub(crate) fn unclaimed_retained_owner_request(
        &self,
        transaction: &RetainedOwnerTransaction,
        plan: &ExecutionPlan,
        action_digest: Sha256Digest,
        lease: Option<LeaseClaim>,
    ) -> Result<Option<Vec<u8>>, ProcessError> {
        let state = self.lock_authoritative_identity(transaction)?;
        if state.index.get(transaction.process_id())?.is_some() {
            return Ok(None);
        }
        let expected = ExecutionManifest::authorized(plan, action_digest, lease);
        load_unclaimed_retained_owner_request(
            transaction,
            &self.inner.retained_owners,
            &self.inner.claims,
            &self.inner.manifests,
            &self.inner.tombstones,
            &self.inner.quarantined_identities,
            &expected,
        )
    }

    pub(crate) fn discard_unclaimed_retained_owner_request(
        &self,
        transaction: &RetainedOwnerTransaction,
    ) -> Result<(), ProcessError> {
        let state = self.lock_authoritative_identity(transaction)?;
        if state.index.get(transaction.process_id())?.is_some() {
            return Err(store_error(
                "recorded retained owner staging cannot be discarded",
            ));
        }
        discard_unclaimed_retained_owner_request(
            transaction,
            &self.inner.retained_owners,
            &self.inner.claims,
            &self.inner.manifests,
            &self.inner.tombstones,
            &self.inner.quarantined_identities,
        )
    }

    pub(crate) fn consume(
        &self,
        plan: &ExecutionPlan,
        action_digest: Sha256Digest,
        lease: Option<LeaseClaim>,
    ) -> Result<(), ProcessError> {
        let identity = plan.identity();
        let process_id = identity.process_id();
        let transaction = self.retained_owner_transaction(process_id)?;
        let mut state = self.lock_authoritative_identity(&transaction)?;
        if state.index.get(process_id)?.is_some() {
            return Err(reused());
        }
        let claim = persist_claim(&self.inner.claims, &identity, action_digest, plan.digest())
            .inspect_err(|_| state.unresolved_io = true)?;
        state.index.claim(claim).inspect_err(|_| state.unresolved_io = true)?;
        let manifest = ExecutionManifest::authorized(plan, action_digest, lease);
        write_manifest(&self.inner.manifests, &manifest)
            .inspect_err(|_| state.unresolved_io = true)?;
        state.index.manifest(&manifest).inspect_err(|_| state.unresolved_io = true)?;
        drop(state);
        Ok(())
    }

    pub(crate) fn consume_retained(
        &self,
        transaction: &RetainedOwnerTransaction,
        plan: &ExecutionPlan,
        action_digest: Sha256Digest,
        lease: Option<LeaseClaim>,
        binding: RetainedOwnerBinding,
        request: &[u8],
        request_digest: Sha256Digest,
    ) -> Result<(), ProcessError> {
        let identity = plan.identity();
        let process_id = identity.process_id();
        if transaction.process_id() != process_id
            || binding.process_id() != process_id
            || binding.action_digest() != action_digest
            || binding.execution_plan_digest() != plan.digest()
            || binding.sandbox_plan_digest() != plan.sandbox_digest()
            || binding.backend_descriptor_digest() != plan.backend().descriptor_digest()
            || binding.backend_support_digest() != plan.backend().support_digest()
            || binding.backend_preparation_digest() != plan.backend().preparation_digest()
            || peritus_codec::sha256(request) != request_digest
        {
            return Err(store_error("retained owner request differs from consumed execution"));
        }
        let mut state = self.lock_authoritative_identity(transaction)?;
        if state.index.get(process_id)?.is_some() {
            return Err(reused());
        }
        let manifest = ExecutionManifest::authorized(plan, action_digest, lease);
        stage_retained_owner_request(
            transaction,
            &self.inner.retained_owners,
            request,
            request_digest,
            &manifest,
        )
        .inspect_err(|_| state.unresolved_io = true)?;
        let claim_binding =
            RetainedClaimBinding::new(binding.operation_digest(), request_digest);
        let claim = persist_retained_claim(
            &self.inner.claims,
            &identity,
            action_digest,
            plan.digest(),
            claim_binding,
        )
        .inspect_err(|_| state.unresolved_io = true)?;
        state.index.claim(claim).inspect_err(|_| state.unresolved_io = true)?;
        write_manifest(&self.inner.manifests, &manifest)
            .inspect_err(|_| state.unresolved_io = true)?;
        state.index.manifest(&manifest).inspect_err(|_| state.unresolved_io = true)?;
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
            if manifest.phase != LifecyclePhase::Starting
                || manifest.tree.is_some_and(|recorded| recorded != tree)
            {
                return Err(store_error("process startup observation is out of sequence"));
            }
            manifest.tree = Some(tree);
            manifest.phase = LifecyclePhase::Running;
            Ok(())
        })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn record_spawned(
        &self,
        process_id: ProcessId,
        tree: ProcessTreeIdentity,
    ) -> Result<(), ProcessError> {
        self.update(process_id, |manifest| {
            if manifest.phase != LifecyclePhase::Starting || manifest.tree.is_some() {
                return Err(store_error("spawned process identity is out of sequence"));
            }
            manifest.tree = Some(tree);
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
            if manifest.phase != LifecyclePhase::Starting || !support_tasks_joined {
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
                || !tree_quiescent
                || !support_tasks_joined
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
                || !tree_quiescent
                || !support_tasks_joined
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

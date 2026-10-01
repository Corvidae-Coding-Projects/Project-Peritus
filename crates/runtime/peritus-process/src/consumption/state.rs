//! Canonical in-memory access to durable process ownership state.

use std::{collections::BTreeMap, path::PathBuf};

use peritus_types::ProcessId;

use crate::{
    LifecyclePhase, OsExitObservation, ProcessError,
    recovery::{claim::ConsumptionClaim, manifest::ExecutionManifest},
    registry_storage::{hex, write_manifest},
};

use super::{ProcessStore, StoreState, store_error};

impl ProcessStore {
    /// Returns paths quarantined while opening this registry.
    #[must_use]
    pub fn quarantined_records(&self) -> Vec<PathBuf> {
        self.lock_state().quarantined_records.clone()
    }

    /// Returns the exact number of durable executions whose owned resources are not settled.
    ///
    /// A result may remain honestly unknown after exact process absence while ownership is fully
    /// settled. Such a record does not make daemon shutdown unclean. Orphan claims and manifests
    /// without both tree quiescence and joined support tasks remain unsettled. This method performs
    /// no platform observation and therefore cannot manufacture quiescence.
    #[must_use]
    pub fn unsettled_ownership_count(&self) -> usize {
        let state = self.lock_state();
        let unsettled =
            state.manifests.values().filter(|manifest| !manifest.ownership_settled()).count();
        let orphan_claims = state
            .claims
            .keys()
            .filter(|process_id| !state.manifests.contains_key(process_id))
            .count();
        drop(state);
        unsettled + orphan_claims
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

    pub(crate) fn reconcile_ownership(
        &self,
        process_id: ProcessId,
        tree_quiescent: bool,
    ) -> Result<(), ProcessError> {
        self.update(process_id, |manifest| {
            if manifest.ownership_settled() {
                return Ok(());
            }
            manifest.tree_quiescent = tree_quiescent;
            manifest.support_tasks_joined = true;
            if manifest.phase != LifecyclePhase::Terminal {
                manifest.exit = Some(OsExitObservation::Unavailable);
                manifest.phase = LifecyclePhase::Closed;
            }
            Ok(())
        })
    }

    pub(super) fn update(
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

    pub(super) fn claim_path(&self, process_id: ProcessId) -> PathBuf {
        self.inner.claims.join(format!("{}.claim", hex(process_id.as_bytes())))
    }

    pub(super) fn lock_state(&self) -> std::sync::MutexGuard<'_, StoreState> {
        self.inner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

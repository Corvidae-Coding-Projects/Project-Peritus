//! Canonical in-memory access to durable process ownership state.

use std::path::PathBuf;

use peritus_types::{ActionId, ProcessId, Sha256Digest};

use crate::{
    ExecutionIdentity, LifecyclePhase, OsExitObservation, ProcessError, ProcessTreeIdentity,
    recovery::manifest::ExecutionManifest,
    registry_storage::{
        RetainedOwnerTransaction, acquire_retained_owner_transaction, hex,
        load_manifest_record, write_manifest,
    },
};

use super::{ProcessStore, StoreState, index::IndexedRecord, retention::retire_record, store_error};

/// Relation between the durable one-use claim and its current process manifest.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProcessClaimState {
    /// No claim is currently readable for this consumed process identity.
    Missing,
    /// A claim remains consumed but its manifest is absent or quarantined.
    Orphaned,
    /// Claim and manifest bind the same action, process, authorization, and plan.
    Matching,
    /// Both records exist but their immutable bindings differ.
    Conflicting,
}

/// Read-only authoritative receipt for one durable process-registry identity.
///
/// The receipt preserves claim and manifest facts separately. Consumers must require
/// [`ProcessClaimState::Matching`] before treating the manifest as the execution selected by the
/// one-use claim. A missing retained owner is only an observation; it never grants redispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessReceipt {
    process_id: ProcessId,
    claim_action_id: Option<ActionId>,
    claim_action_digest: Option<Sha256Digest>,
    claim_plan_digest: Option<Sha256Digest>,
    identity: Option<ExecutionIdentity>,
    manifest_action_digest: Option<Sha256Digest>,
    manifest_plan_digest: Option<Sha256Digest>,
    claim_state: ProcessClaimState,
    phase: Option<LifecyclePhase>,
    tree: Option<ProcessTreeIdentity>,
    manifest_ownership_settled: bool,
    terminal_available: bool,
    retained_owner_claimed: bool,
    retired: bool,
}

impl ProcessReceipt {
    fn from_indexed(record: IndexedRecord) -> Self {
        let claim_state = match (&record.claim, &record.manifest) {
            (None, _) => ProcessClaimState::Missing,
            (Some(_), None) => ProcessClaimState::Orphaned,
            (Some(claim), Some(manifest)) if claim.matches_manifest(manifest) => {
                ProcessClaimState::Matching
            }
            (Some(_), Some(_)) => ProcessClaimState::Conflicting,
        };
        Self {
            process_id: record.process_id,
            claim_action_id: record.claim.map(|claim| claim.action_id()),
            claim_action_digest: record.claim.map(|claim| claim.action_digest()),
            claim_plan_digest: record.claim.map(|claim| claim.plan_digest()),
            identity: record.manifest.as_ref().map(|manifest| manifest.identity),
            manifest_action_digest: record
                .manifest
                .as_ref()
                .map(|manifest| manifest.action_digest),
            manifest_plan_digest: record
                .manifest
                .as_ref()
                .map(|manifest| manifest.plan_digest),
            phase: record.manifest.as_ref().map(|manifest| manifest.phase),
            tree: record.manifest.as_ref().and_then(|manifest| manifest.tree),
            manifest_ownership_settled: record
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.ownership_settled()),
            terminal_available: record
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.terminal.is_some()),
            retained_owner_claimed: record
                .claim
                .is_some_and(|claim| claim.retained_owner().is_some()),
            retired: record.retired,
            claim_state,
        }
    }

    /// Returns the stable process-registry key.
    #[must_use]
    pub const fn process_id(&self) -> ProcessId { self.process_id }

    /// Returns the action consumed by the one-use claim, when that claim is readable.
    #[must_use]
    pub const fn claim_action_id(&self) -> Option<ActionId> { self.claim_action_id }

    /// Returns the authorization digest retained by the one-use claim.
    #[must_use]
    pub const fn claim_action_digest(&self) -> Option<Sha256Digest> {
        self.claim_action_digest
    }

    /// Returns the execution-plan digest retained by the one-use claim.
    #[must_use]
    pub const fn claim_plan_digest(&self) -> Option<Sha256Digest> { self.claim_plan_digest }

    /// Returns the complete authenticated manifest identity, when its manifest is readable.
    #[must_use]
    pub const fn identity(&self) -> Option<ExecutionIdentity> { self.identity }

    /// Returns the authorization digest retained by the manifest.
    #[must_use]
    pub const fn manifest_action_digest(&self) -> Option<Sha256Digest> {
        self.manifest_action_digest
    }

    /// Returns the execution-plan digest retained by the manifest.
    #[must_use]
    pub const fn manifest_plan_digest(&self) -> Option<Sha256Digest> {
        self.manifest_plan_digest
    }

    /// Returns the exact relation between the claim and manifest.
    #[must_use]
    pub const fn claim_state(&self) -> ProcessClaimState { self.claim_state }

    /// Returns the current durable lifecycle frontier, when its manifest is readable.
    #[must_use]
    pub const fn phase(&self) -> Option<LifecyclePhase> { self.phase }

    /// Returns the exact durable native tree identity, when one has been recorded.
    #[must_use]
    pub const fn tree_identity(&self) -> Option<ProcessTreeIdentity> { self.tree }

    /// Returns whether the manifest itself records complete tree and support-task settlement.
    ///
    /// Callers must also require [`Self::claim_state`] to be matching before attributing this
    /// settlement to the consumed claim.
    #[must_use]
    pub const fn manifest_ownership_settled(&self) -> bool {
        self.manifest_ownership_settled
    }

    /// Returns whether the manifest retains a complete process terminal result.
    #[must_use]
    pub const fn terminal_available(&self) -> bool { self.terminal_available }

    /// Returns whether the claim binds an independently retained owner request.
    #[must_use]
    pub const fn retained_owner_claimed(&self) -> bool { self.retained_owner_claimed }

    /// Returns whether this receipt is an immutable settled tombstone.
    #[must_use]
    pub const fn retired(&self) -> bool { self.retired }
}

impl ProcessStore {
    /// Returns paths quarantined while opening this registry.
    #[must_use]
    pub fn quarantined_records(&self) -> Vec<PathBuf> {
        self.lock_state().quarantined_records.clone()
    }

    /// Returns the number of durable executions whose owned resources are not settled.
    ///
    /// A result may remain honestly unknown after exact process absence while ownership is fully
    /// settled. Such a record does not make daemon shutdown unclean. Orphan claims and manifests
    /// without both tree quiescence and joined support tasks remain unsettled. This method performs
    /// no platform observation and therefore cannot manufacture quiescence. An unavailable index
    /// conservatively reports `usize::MAX`; use the fallible variant for the actual diagnosis.
    #[must_use]
    pub fn unsettled_ownership_count(&self) -> usize {
        self.try_unsettled_ownership_count().unwrap_or(usize::MAX)
    }

    /// Counts unsettled durable ownership without collecting the lifetime registry in memory.
    ///
    /// # Errors
    /// Returns an index or prior publication error rather than inventing settled ownership.
    pub fn try_unsettled_ownership_count(&self) -> Result<usize, ProcessError> {
        let mut count = 0_usize;
        self.visit_registry(|record| {
            let settled = record.manifest.as_ref().is_some_and(|manifest| {
                manifest.ownership_settled()
                    && record
                        .claim
                        .as_ref()
                        .is_some_and(|claim| claim.matches_manifest(manifest))
            });
            if !settled {
                count = count
                    .checked_add(1)
                    .ok_or_else(|| store_error("process ownership count overflow"))?;
            }
            Ok(())
        })?;
        Ok(count)
    }

    pub(crate) fn registry_page(
        &self,
        after: Option<ProcessId>,
    ) -> Result<Vec<IndexedRecord>, ProcessError> {
        let state = self.lock_current_state()?;
        state.index.page(after)
    }

    /// Returns whether any durable claim, manifest, tombstone, or quarantined receipt consumes
    /// this process identity in the store's current authoritative frontier.
    ///
    /// # Errors
    /// Returns a typed persistence error when the registry projection cannot be inspected.
    pub fn process_identity_recorded(
        &self,
        process_id: ProcessId,
    ) -> Result<bool, ProcessError> {
        self.authoritative_record(process_id)
            .map(|record| record.is_some())
    }

    /// Reads one authoritative process receipt without changing its lifecycle or ownership.
    ///
    /// # Errors
    /// Returns the exact registry refresh, persistence, or corruption failure for this identity.
    pub fn process_receipt(
        &self,
        process_id: ProcessId,
    ) -> Result<Option<ProcessReceipt>, ProcessError> {
        self.authoritative_record(process_id)
            .map(|record| record.map(ProcessReceipt::from_indexed))
    }

    /// Reads the next bounded page of authoritative process receipts in process-ID order.
    ///
    /// Physical page size is an allocation unit, never a lifetime receipt limit. Supply the last
    /// returned process identity as `after` until an empty page is returned.
    ///
    /// # Errors
    /// Returns an index, persistence, or corruption failure rather than omitting that frontier.
    pub fn process_receipts_page(
        &self,
        after: Option<ProcessId>,
    ) -> Result<Vec<ProcessReceipt>, ProcessError> {
        let page = self.registry_page(after)?;
        let mut receipts = Vec::with_capacity(page.len());
        for indexed in page {
            let record = self
                .authoritative_record(indexed.process_id)?
                .ok_or_else(|| store_error("indexed process receipt disappeared during paging"))?;
            receipts.push(ProcessReceipt::from_indexed(record));
        }
        Ok(receipts)
    }

    pub(crate) fn visit_registry(
        &self,
        mut visit: impl FnMut(IndexedRecord) -> Result<(), ProcessError>,
    ) -> Result<(), ProcessError> {
        let mut after = None;
        loop {
            let page = self.registry_page(after)?;
            if page.is_empty() { break; }
            for record in page {
                let process_id = record.process_id;
                after = Some(process_id);
                let record = self
                    .authoritative_record(process_id)?
                    .ok_or_else(|| store_error("indexed process record disappeared during traversal"))?;
                visit(record)?;
            }
        }
        Ok(())
    }

    pub(crate) fn reconcile_ownership(
        &self,
        expected: &ExecutionManifest,
        tree_quiescent: bool,
    ) -> Result<Option<bool>, ProcessError> {
        let mut settled = None;
        let process_id = expected.identity.process_id();
        self.update(process_id, |manifest| {
            // A concurrent owner may have advanced or supplied a previously missing native tree
            // after the recovery snapshot. Its new ownership must never be settled by old facts.
            if manifest != expected {
                return Ok(());
            }
            if manifest.ownership_settled() {
                settled = Some(true);
                return Ok(());
            }
            if !tree_quiescent {
                settled = Some(false);
                return Ok(());
            }
            manifest.tree_quiescent = true;
            if !matches!(manifest.phase, LifecyclePhase::Closed | LifecyclePhase::Terminal) {
                manifest.support_tasks_joined = true;
                if manifest.exit.is_none() {
                    manifest.exit = Some(OsExitObservation::Unavailable);
                }
                manifest.phase = LifecyclePhase::Closed;
            }
            settled = Some(manifest.ownership_settled());
            Ok(())
        })?;
        Ok(settled)
    }

    pub(super) fn update(
        &self,
        process_id: ProcessId,
        update: impl FnOnce(&mut ExecutionManifest) -> Result<(), ProcessError>,
    ) -> Result<(), ProcessError> {
        let transaction =
            acquire_retained_owner_transaction(&self.inner.retained_owners, process_id)?;
        let mut state = self.lock_authoritative_identity(&transaction)?;
        let record = state.index.get(process_id)?
            .ok_or_else(|| store_error("process manifest is missing"))?;
        let manifest = record.manifest.ok_or_else(|| store_error("process manifest is missing"))?;
        let mut next = manifest.clone();
        update(&mut next)?;
        if record.retired {
            return if next == manifest { Ok(()) }
                else { Err(store_error("retired process receipt is immutable")) };
        }
        let written = write_manifest(&self.inner.manifests, &next);
        // A reported directory-sync or backup-cleanup failure may already have published next.
        // Re-observe the canonical file instead of claiming the predecessor is still current.
        let actual = load_manifest_record(&self.inner.manifests, process_id).and_then(|manifest| {
            manifest.ok_or_else(|| store_error("published process manifest disappeared"))
        });
        match actual {
            Ok(actual) => state.index.manifest(&actual).inspect_err(|_| state.unresolved_io = true)?,
            Err(error) => { state.unresolved_io = true; return Err(error); }
        }
        written?;
        if let Err(error) = retire_record(&self.inner.claims, &self.inner.manifests,
            &self.inner.spools, &self.inner.tombstones, &mut state, process_id)
        {
            eprintln!("process registry retirement deferred: {error}");
        }
        drop(state);
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn claim_path(&self, process_id: ProcessId) -> PathBuf {
        self.inner
            .claims
            .join(format!("{}.claim", hex(process_id.as_bytes())))
    }

    pub(super) fn lock_state(&self) -> std::sync::MutexGuard<'_, StoreState> {
        self.inner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn lock_current_state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, StoreState>, ProcessError> {
        loop {
            let state = self.lock_state();
            if !state.unresolved_io {
                return Ok(state);
            }
            let revision = state.revision;
            drop(state);
            // Never hold the in-process state mutex while taking identity file locks. A writer
            // takes those locks first, so reversing the order here would deadlock recovery.
            let (index, quarantined_records) =
                super::index::RegistryIndex::rebuild(&self.inner.root)?;
            let mut state = self.lock_state();
            if state.revision != revision {
                continue;
            }
            state.index = index;
            state.quarantined_records = quarantined_records;
            state.unresolved_io = false;
            return Ok(state);
        }
    }

    pub(super) fn lock_authoritative_identity(
        &self,
        transaction: &RetainedOwnerTransaction,
    ) -> Result<std::sync::MutexGuard<'_, StoreState>, ProcessError> {
        let mut state = self.lock_state();
        let result = {
            let StoreState { index, quarantined_records, .. } = &mut *state;
            index.reconcile_identity(
                &self.inner.root,
                transaction.process_id(),
                quarantined_records,
            )
        };
        if let Err(error) = result {
            state.unresolved_io = true;
            return Err(error);
        }
        state.revision = state
            .revision
            .checked_add(1)
            .ok_or_else(|| store_error("process registry revision overflow"))?;
        Ok(state)
    }

    pub(crate) fn refresh_authoritative_identity(
        &self,
        process_id: ProcessId,
    ) -> Result<(), ProcessError> {
        let transaction =
            acquire_retained_owner_transaction(&self.inner.retained_owners, process_id)?;
        drop(self.lock_authoritative_identity(&transaction)?);
        Ok(())
    }

    pub(crate) fn authoritative_record(
        &self,
        process_id: ProcessId,
    ) -> Result<Option<IndexedRecord>, ProcessError> {
        let transaction =
            acquire_retained_owner_transaction(&self.inner.retained_owners, process_id)?;
        self.lock_authoritative_identity(&transaction)?
            .index
            .get(process_id)
    }

}

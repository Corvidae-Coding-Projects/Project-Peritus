//! Explicit writable workspace state.

use std::collections::BTreeMap;

use peritus_leases::LeaseHolder;
use peritus_types::{ActionId, Generation, RevisionNumber, Sha256Digest};

use crate::{SnapshotIdentity, WorkspaceBinding};

/// Safety condition of the live writable worktree.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkspaceCondition {
    /// Exact durable snapshot and complete Git/filesystem observations agree.
    Clean,
    /// A complete observation found a known divergence.
    Dirty,
    /// A fence requires exact correlated reconciliation.
    Reconciling,
    /// Complete safety could not be determined.
    Indeterminate,
}

/// Durable logical state projected into one writable handle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceState {
    binding: WorkspaceBinding,
    generation: Generation,
    revision: RevisionNumber,
    current: SnapshotIdentity,
    lease_holder: LeaseHolder,
    condition: WorkspaceCondition,
    consumed_actions: BTreeMap<ActionId, Sha256Digest>,
}

impl WorkspaceState {
    /// Creates state only when the snapshot exactly belongs to the supplied lineage and counters.
    ///
    /// # Errors
    ///
    /// Returns a stale-workspace failure for any identity mismatch.
    pub fn new(
        binding: WorkspaceBinding,
        generation: Generation,
        revision: RevisionNumber,
        current: SnapshotIdentity,
        lease_holder: LeaseHolder,
        condition: WorkspaceCondition,
    ) -> Result<Self, crate::WorkspaceError> {
        if binding.workspace_id() != current.workspace_id()
            || generation != current.generation()
            || revision != current.revision()
        {
            return Err(crate::WorkspaceError::new(
                crate::ErrorCode::StaleWorkspace,
                crate::WorkspaceOperation::Open,
                crate::RecoveryClass::Reobserve,
                "snapshot identity does not match workspace state",
            ));
        }
        Ok(Self {
            binding,
            generation,
            revision,
            current,
            lease_holder,
            condition,
            consumed_actions: BTreeMap::new(),
        })
    }

    /// Returns the exact nominal/filesystem binding.
    #[must_use]
    pub const fn binding(&self) -> &WorkspaceBinding {
        &self.binding
    }
    /// Returns the current fenced generation.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }
    /// Returns the current logical revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionNumber {
        self.revision
    }
    /// Returns the current immutable snapshot.
    #[must_use]
    pub const fn current_snapshot(&self) -> &SnapshotIdentity {
        &self.current
    }
    /// Returns the durable holder of this workspace generation.
    ///
    /// During post-fence reconciliation this is the target-owned prior-holder observation; it is
    /// never copied from the caller's expected correlation.
    #[must_use]
    pub const fn lease_holder(&self) -> LeaseHolder {
        self.lease_holder
    }
    /// Returns the safety condition.
    #[must_use]
    pub const fn condition(&self) -> WorkspaceCondition {
        self.condition
    }

    /// Returns whether this revision's durable target ledger already consumed an action.
    #[must_use]
    pub fn action_consumed(&self, action_id: ActionId) -> bool {
        self.consumed_actions.contains_key(&action_id)
    }

    pub(crate) fn install(&mut self, snapshot: SnapshotIdentity) {
        self.revision = snapshot.revision();
        self.current = snapshot;
        self.condition = WorkspaceCondition::Clean;
        self.consumed_actions.clear();
    }

    pub(crate) const fn set_condition(&mut self, condition: WorkspaceCondition) {
        self.condition = condition;
    }

    pub(crate) fn record_consumed_action(
        &mut self,
        action_id: ActionId,
        action_digest: Sha256Digest,
    ) {
        self.consumed_actions.insert(action_id, action_digest);
    }

    pub(crate) fn consumed_action_count(&self) -> usize {
        self.consumed_actions.len()
    }
}

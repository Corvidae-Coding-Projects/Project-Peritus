//! Verified semantic clones for admitted and retained work.

use super::{WorkRecord, WorkSpec, WorkTerminal};
use vstd::prelude::*;

verus! {

impl WorkSpec {
    /// Relates an exact semantic clone of a work specification.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.id == right.id
            && left.owner == right.owner
            && left.revision == right.revision
            && left.class == right.class
            && left.priority == right.priority
            && left.request.spec_entries() == right.request.spec_entries()
            && left.budget_reservation == right.budget_reservation
            && left.dependencies@ == right.dependencies@
            && left.parent == right.parent
            && left.maximum_attempts == right.maximum_attempts
            && left.recovery == right.recovery
            && left.payload_digest == right.payload_digest
    }

    /// Projects admission identities from an exact semantic clone.
    pub(crate) proof fn clone_admission_fields(left: &Self, right: &Self)
        requires Self::clone_equivalent(left, right),
        ensures
            left.spec_id() == right.spec_id(),
            left.spec_dependencies() == right.spec_dependencies(),
            left.spec_parent() == right.spec_parent(),
    {
    }
}

impl Clone for WorkSpec {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            id: self.id,
            owner: self.owner,
            revision: self.revision,
            class: self.class,
            priority: self.priority,
            request: self.request.clone(),
            budget_reservation: self.budget_reservation,
            dependencies: self.dependencies.clone(),
            parent: self.parent,
            maximum_attempts: self.maximum_attempts,
            recovery: self.recovery,
            payload_digest: self.payload_digest,
        }
    }
}

impl Clone for WorkTerminal {
    fn clone(&self) -> (result: Self)
        ensures result == *self,
    {
        match self {
            Self::Succeeded { result_digest } => {
                Self::Succeeded { result_digest: *result_digest }
            },
            Self::Failed { failure_digest } => {
                Self::Failed { failure_digest: *failure_digest }
            },
            Self::DependencyFailed { dependency } => {
                Self::DependencyFailed { dependency: *dependency }
            },
            Self::Cancelled => Self::Cancelled,
            Self::Ambiguous { dispatch_id } => {
                Self::Ambiguous { dispatch_id: *dispatch_id }
            },
            Self::Exhausted { cause_digest } => {
                Self::Exhausted { cause_digest: *cause_digest }
            },
            Self::Abandoned { cause_digest } => {
                Self::Abandoned { cause_digest: *cause_digest }
            },
        }
    }
}

impl WorkRecord {
    /// Relates an exact semantic clone of a retained work record.
    pub closed spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        WorkSpec::clone_equivalent(&left.spec, &right.spec)
            && left.phase == right.phase
            && left.enqueue_ordinal == right.enqueue_ordinal
            && left.bypasses == right.bypasses
            && left.attempts_started == right.attempts_started
            && left.retry_cause == right.retry_cause
            && left.terminal == right.terminal
    }

    pub(super) proof fn clone_storage_fields(left: &Self, right: &Self)
        requires Self::clone_equivalent(left, right),
        ensures
            left.spec.id == right.spec.id,
            left.spec.owner == right.spec.owner,
            left.spec.revision == right.spec.revision,
            left.spec.request.spec_entries() == right.spec.request.spec_entries(),
            left.attempts_started == right.attempts_started,
            left.phase == right.phase,
    {
        reveal(WorkRecord::clone_equivalent);
        reveal(WorkSpec::clone_equivalent);
    }
}

impl Clone for WorkRecord {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            spec: self.spec.clone(),
            phase: self.phase,
            enqueue_ordinal: self.enqueue_ordinal,
            bypasses: self.bypasses,
            attempts_started: self.attempts_started,
            retry_cause: self.retry_cause,
            terminal: self.terminal.clone(),
        }
    }
}

} // verus!

//! Verified semantic clones for admitted and retained work.

use super::{WorkRecord, WorkSpec, WorkTerminal};

impl WorkSpec {}
impl Clone for WorkSpec {
    fn clone(&self) -> Self {
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
    fn clone(&self) -> Self {
        match self {
            Self::Succeeded { result_digest } => Self::Succeeded { result_digest: *result_digest },
            Self::Failed { failure_digest } => Self::Failed { failure_digest: *failure_digest },
            Self::DependencyFailed { dependency } => {
                Self::DependencyFailed { dependency: *dependency }
            }
            Self::Cancelled => Self::Cancelled,
            Self::Ambiguous { dispatch_id } => Self::Ambiguous { dispatch_id: *dispatch_id },
            Self::Exhausted { cause_digest } => Self::Exhausted { cause_digest: *cause_digest },
            Self::Abandoned { cause_digest } => Self::Abandoned { cause_digest: *cause_digest },
        }
    }
}
impl WorkRecord {}
impl Clone for WorkRecord {
    fn clone(&self) -> Self {
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

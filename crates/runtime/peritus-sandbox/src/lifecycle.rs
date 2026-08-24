//! Closed sandbox-session lifecycle.

use crate::{RecoveryClass, SandboxError, SandboxErrorKind, SandboxOperation};

/// Backend session phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SandboxPhase {
    /// Plan admitted but not prepared.
    Planned,
    /// Backend state prepared but not active.
    Prepared,
    /// Effects may be evaluated or executed.
    Active,
    /// Cancellation accepted and termination pending.
    Cancelling,
    /// Terminal outcome observed.
    Terminated,
    /// All owned backend state released.
    Released,
}

impl SandboxPhase {
    const fn ordinal(self) -> u8 {
        match self {
            Self::Planned => 0,
            Self::Prepared => 1,
            Self::Active => 2,
            Self::Cancelling => 3,
            Self::Terminated => 4,
            Self::Released => 5,
        }
    }

    /// Reports whether moving to `next` is a legal lifecycle edge.
    #[must_use]
    pub const fn permits(self, next: Self) -> bool {
        crate::verified::lifecycle_edge_allowed(self.ordinal(), next.ordinal())
    }

    /// Validates and returns a lifecycle transition.
    ///
    /// # Errors
    /// Returns `IllegalTransition` when the edge is not in the closed lifecycle.
    pub const fn transition(self, next: Self) -> Result<Self, SandboxError> {
        if self.permits(next) {
            Ok(next)
        } else {
            Err(SandboxError::new(
                SandboxErrorKind::IllegalTransition,
                operation_for(next),
                RecoveryClass::Reconcile,
                "illegal sandbox lifecycle transition",
            ))
        }
    }
}

const fn operation_for(phase: SandboxPhase) -> SandboxOperation {
    match phase {
        SandboxPhase::Planned | SandboxPhase::Prepared => SandboxOperation::Prepare,
        SandboxPhase::Active => SandboxOperation::Activate,
        SandboxPhase::Cancelling => SandboxOperation::Cancel,
        SandboxPhase::Terminated => SandboxOperation::Terminate,
        SandboxPhase::Released => SandboxOperation::Release,
    }
}

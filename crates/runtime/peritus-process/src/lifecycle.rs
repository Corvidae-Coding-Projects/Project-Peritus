//! Deterministic owned-process lifecycle reducer.

use crate::{CancellationReason, ProcessError, StopTrigger, error::invalid};

/// Durable lifecycle phase for one execution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LifecyclePhase {
    /// Authority was consumed and intent persisted.
    Authorized,
    /// Process creation is in progress.
    Starting,
    /// Startup was observed and the process tree is live.
    Running,
    /// A first stop trigger was accepted.
    Stopping,
    /// The operating-system root exit was observed.
    Exited,
    /// Input/output and backend teardown completed.
    Closed,
    /// Exactly one terminal result was durably published.
    Terminal,
}

/// Executable lifecycle state with immutable first-trigger precedence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LifecycleState {
    phase: LifecyclePhase,
    first_trigger: Option<StopTrigger>,
    terminal_accepted: bool,
}

impl LifecycleState {
    /// Creates a newly authorized lifecycle.
    #[must_use]
    pub const fn authorized() -> Self {
        Self { phase: LifecyclePhase::Authorized, first_trigger: None, terminal_accepted: false }
    }

    /// Returns the current durable phase.
    #[must_use]
    pub const fn phase(self) -> LifecyclePhase {
        self.phase
    }
    /// Returns the immutable first stop trigger.
    #[must_use]
    pub const fn first_trigger(self) -> Option<StopTrigger> {
        self.first_trigger
    }
    /// Returns whether the unique terminal result was accepted.
    #[must_use]
    pub const fn terminal_accepted(self) -> bool {
        self.terminal_accepted
    }

    /// Advances to the exact next ordinary lifecycle phase.
    ///
    /// # Errors
    ///
    /// Returns an error for a skipped, repeated, or backward transition.
    pub fn advance(&mut self, next: LifecyclePhase) -> Result<(), ProcessError> {
        let legal = matches!(
            (self.phase, next),
            (LifecyclePhase::Authorized, LifecyclePhase::Starting)
                | (LifecyclePhase::Starting, LifecyclePhase::Running)
                | (LifecyclePhase::Running | LifecyclePhase::Stopping, LifecyclePhase::Exited)
                | (LifecyclePhase::Exited, LifecyclePhase::Closed)
                | (LifecyclePhase::Closed, LifecyclePhase::Terminal)
        );
        if !legal {
            return Err(invalid("illegal process lifecycle transition"));
        }
        if next == LifecyclePhase::Terminal {
            if self.terminal_accepted {
                return Err(invalid("a terminal process result was already accepted"));
            }
            self.terminal_accepted = true;
        }
        self.phase = next;
        Ok(())
    }

    /// Records the first stop trigger and enters stopping idempotently.
    ///
    /// Returns `true` only for the first accepted trigger.
    #[must_use]
    pub const fn request_stop(&mut self, sequence: u64, reason: CancellationReason) -> bool {
        if self.first_trigger.is_some()
            || !matches!(self.phase, LifecyclePhase::Starting | LifecyclePhase::Running)
        {
            return false;
        }
        self.first_trigger = Some(StopTrigger::new(sequence, reason));
        self.phase = LifecyclePhase::Stopping;
        true
    }

    /// Records a spawn failure as a closed lifecycle ready for terminal publication.
    ///
    /// # Errors
    ///
    /// Returns an error unless process creation was in progress.
    pub fn spawn_failed(&mut self) -> Result<(), ProcessError> {
        if self.phase != LifecyclePhase::Starting {
            return Err(invalid("spawn failure was observed outside Starting"));
        }
        self.phase = LifecyclePhase::Closed;
        Ok(())
    }
}

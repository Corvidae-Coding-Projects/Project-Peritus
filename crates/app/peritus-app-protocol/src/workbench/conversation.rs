//! Authenticated execution discovery for a selected durable conversation.

use crate::{AppErrorCode, AppProtocolError, WorkbenchSnapshot};
use peritus_types::RunId;

/// Observed binding only; querying this state never starts or resumes execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchExecutionState {
    snapshot: WorkbenchSnapshot,
    run: Option<RunId>,
    goal: bool,
}
impl WorkbenchExecutionState {
    /// Binds current metadata to its optional original execution.
    ///
    /// # Errors
    /// Rejects a goal without its execution binding.
    pub fn new(
        snapshot: WorkbenchSnapshot,
        run: Option<RunId>,
        goal: bool,
    ) -> Result<Self, AppProtocolError> {
        if goal && run.is_none() {
            return Err(AppProtocolError::new(AppErrorCode::MalformedFrame, None));
        }
        Ok(Self { snapshot, run, goal })
    }
    /// Current authorized metadata and aggregate revision.
    #[must_use]
    pub const fn snapshot(&self) -> &WorkbenchSnapshot {
        &self.snapshot
    }
    /// Original execution, if one has been admitted.
    #[must_use]
    pub const fn run(&self) -> Option<RunId> {
        self.run
    }
    /// Whether explicit goal controls govern resumption.
    #[must_use]
    pub const fn has_goal(&self) -> bool {
        self.goal
    }
}

/// Explicit continuation semantics for an existing durable conversation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchContinuation {
    query: crate::WorkbenchQuery,
    mode: crate::ProductInteractionMode,
}
impl WorkbenchContinuation {
    /// Chooses the mode for the next execution without inventing an input.
    #[must_use]
    pub const fn new(query: crate::WorkbenchQuery, mode: crate::ProductInteractionMode) -> Self {
        Self { query, mode }
    }
    /// Exact authorized conversation scope.
    #[must_use]
    pub const fn query(self) -> crate::WorkbenchQuery {
        self.query
    }
    /// Requested execution mode; the host checks that work is idle before changing it.
    #[must_use]
    pub const fn mode(self) -> crate::ProductInteractionMode {
        self.mode
    }
}

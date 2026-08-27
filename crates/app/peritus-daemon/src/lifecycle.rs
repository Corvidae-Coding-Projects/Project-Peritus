//! Monotonic daemon startup and readiness state.

use peritus_app_protocol::{DaemonReadiness, DaemonStatus};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

/// Closed ordered startup checkpoint vocabulary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StartupPhase {
    /// Validate configuration and deployment identity.
    Validate,
    /// Acquire the exclusive daemon instance lock.
    Lock,
    /// Reconcile and apply forward migrations.
    Migrate,
    /// Open and verify the authoritative journal.
    Journal,
    /// Reconcile artifact storage.
    Artifacts,
    /// Reconcile acceptance evidence.
    Evidence,
    /// Rebuild or validate projections.
    Projections,
    /// Allocate the startup authority epoch.
    AuthorityEpoch,
    /// Recover pure domain state.
    DomainRecovery,
    /// Recover owned effect state.
    EffectRecovery,
    /// Reconcile application commands and sessions.
    AppRecovery,
    /// Start the single outbox router.
    Outbox,
    /// Bind the authenticated local endpoint.
    Ipc,
    /// All required mutation paths are ready.
    Ready,
}

/// Current monotonic startup phase and truthful A3 readiness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonLifecycle {
    phase: StartupPhase,
    readiness: DaemonReadiness,
    diagnostic: Option<String>,
}

impl DaemonLifecycle {
    /// Creates the pre-validation lifecycle.
    #[must_use]
    pub const fn starting() -> Self {
        Self {
            phase: StartupPhase::Validate,
            readiness: DaemonReadiness::Starting,
            diagnostic: None,
        }
    }

    /// Returns the last established startup phase.
    #[must_use]
    pub const fn phase(&self) -> StartupPhase {
        self.phase
    }
    /// Returns truthful admission readiness.
    #[must_use]
    pub const fn readiness(&self) -> DaemonReadiness {
        self.readiness
    }

    /// Advances to exactly the next canonical startup phase.
    ///
    /// # Errors
    ///
    /// Returns corrupt state if a caller repeats, skips, or reverses a startup phase.
    pub fn advance(&mut self, next: StartupPhase) -> Result<(), DaemonError> {
        if !crate::verified::startup_transition_exec(self.phase, next) {
            return Err(DaemonError::new(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::Operator,
                "advance daemon startup",
                "startup phase is not the exact canonical successor",
            ));
        }
        self.phase = next;
        if next == StartupPhase::Ready {
            self.readiness = DaemonReadiness::ReadyReadWrite;
            self.diagnostic = None;
        }
        Ok(())
    }

    /// Enters explicit read-only diagnostics with bounded inert detail.
    pub fn read_only(&mut self, diagnostic: impl Into<String>) {
        self.readiness = DaemonReadiness::ReadyReadOnly;
        self.diagnostic = Some(diagnostic.into());
    }

    /// Closes mutation intake while owned work drains.
    pub fn begin_draining(&mut self) {
        self.readiness = DaemonReadiness::Draining;
    }

    /// Marks the daemon unavailable after endpoint/storage closure.
    pub fn unavailable(&mut self) {
        self.readiness = DaemonReadiness::Unavailable;
    }

    /// Builds the bounded public status value.
    ///
    /// # Errors
    ///
    /// Returns an internal state error only if diagnostic bounds are violated.
    pub fn status(&self, maximum_diagnostic_bytes: usize) -> Result<DaemonStatus, DaemonError> {
        DaemonStatus::new(self.readiness, self.diagnostic.clone(), maximum_diagnostic_bytes)
            .map_err(|error| {
                DaemonError::with_source(
                    DaemonErrorCode::CorruptState,
                    DaemonRecovery::Operator,
                    "build daemon status",
                    "lifecycle diagnostic violates the A3 bound",
                    error,
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{DaemonLifecycle, StartupPhase};

    #[test]
    fn startup_rejects_repeated_skipped_and_reversed_phases() {
        let mut lifecycle = DaemonLifecycle::starting();
        assert!(lifecycle.advance(StartupPhase::Validate).is_err());
        assert!(lifecycle.advance(StartupPhase::Migrate).is_err());
        lifecycle.advance(StartupPhase::Lock).expect("exact successor");
        assert!(lifecycle.advance(StartupPhase::Validate).is_err());
        lifecycle.advance(StartupPhase::Migrate).expect("next exact successor");
    }
}

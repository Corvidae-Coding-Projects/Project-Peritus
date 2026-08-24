//! Activated-session lifecycle transitions and bound observations.

use peritus_process::{CancellationReason, OsExitObservation, ProcessTreeIdentity};
use peritus_sandbox::{EnforcementObservation, ObservationDisposition, ObservationKind};

use super::{MacosSession, SessionPhase, TerminationReason, adapter::lifecycle_error};
use crate::{
    MacosError, MacosErrorKind, MacosObservation, MacosOperation, ObservationEvent,
    ObservationStatus, RecoveryAction,
};

impl MacosSession {
    /// Activates only after C2 verified the helper handshake and complete process containment.
    ///
    /// # Errors
    /// Rejects an invalid phase, absent process group, or incomplete descendant containment.
    pub fn record_activation(&mut self, tree: ProcessTreeIdentity) -> Result<(), MacosError> {
        if self.phase != SessionPhase::Prepared {
            return Err(lifecycle_error("activation requires a prepared session"));
        }
        let root_pid = tree.root_pid();
        let owned_process_group = root_pid != 0 && tree.process_group() == Some(root_pid);
        if !crate::verified::activation_permitted(
            tree.complete_containment(),
            owned_process_group,
            true,
        ) {
            return Err(MacosError::new(
                MacosErrorKind::SupervisorFailure,
                MacosOperation::Activate,
                RecoveryAction::CancelAndReap,
                "C2 did not establish complete process-group containment",
            ));
        }
        if !self.launch.release_protected_handle(crate::EXEC_STATUS_LABEL) {
            return Err(lifecycle_error("helper exec status ownership was absent at activation"));
        }
        self.exec_status.observe(self.manifest.digest(), self.manifest.preparation_digest())?;
        self.phase = SessionPhase::Active;
        self.recovery.record_activation(root_pid, tree.process_group())?;
        self.push_lifecycle(
            ObservationKind::Activated,
            ObservationEvent::Activated,
            ObservationDisposition::Completed,
            ObservationStatus::Completed,
        )
    }

    /// Records an idempotent first cancellation request.
    ///
    /// # Errors
    /// Rejects cancellation outside an active/cancelling session or an exhausted observation bound.
    pub fn record_cancellation(&mut self, reason: CancellationReason) -> Result<(), MacosError> {
        match self.phase {
            SessionPhase::Active => {
                self.phase = SessionPhase::Cancelling;
                self.cancellation = Some(reason);
                self.push_lifecycle(
                    ObservationKind::Cancellation,
                    ObservationEvent::CancelRequested,
                    ObservationDisposition::Accepted,
                    ObservationStatus::Accepted,
                )
            }
            SessionPhase::Cancelling if self.cancellation == Some(reason) => Ok(()),
            _ => Err(lifecycle_error("cancellation is invalid in the current phase")),
        }
    }

    /// Records root termination after authenticated helper activation.
    ///
    /// # Errors
    /// Rejects termination outside an active/cancelling phase or an exhausted observation bound.
    pub fn record_termination(&mut self, exit: &OsExitObservation) -> Result<(), MacosError> {
        if !matches!(self.phase, SessionPhase::Active | SessionPhase::Cancelling) {
            return Err(lifecycle_error("termination requires an active or cancelling session"));
        }
        self.termination = Some(match exit {
            OsExitObservation::Code(code) => TerminationReason::TargetExit(*code),
            OsExitObservation::Unavailable => TerminationReason::Unavailable,
            OsExitObservation::Signal(_)
            | OsExitObservation::SignalName(_)
            | OsExitObservation::PlatformException(_) => TerminationReason::Signalled,
        });
        self.phase = SessionPhase::Terminated;
        self.push_lifecycle(
            ObservationKind::Terminated,
            ObservationEvent::Terminated,
            ObservationDisposition::Completed,
            ObservationStatus::Completed,
        )?;
        Ok(())
    }

    pub(super) fn push_lifecycle(
        &mut self,
        kind: ObservationKind,
        event: ObservationEvent,
        disposition: ObservationDisposition,
        status: ObservationStatus,
    ) -> Result<(), MacosError> {
        if self.observations.len() >= self.observation_limit {
            return Err(crate::error::limited(
                MacosOperation::Activate,
                "native observation bound is exhausted",
            ));
        }
        let sequence = u64::try_from(self.observations.len()).unwrap_or(u64::MAX).saturating_add(1);
        self.observations.push(EnforcementObservation::new(
            sequence,
            self.manifest.plan_digest(),
            self.manifest.descriptor_digest(),
            kind,
            None,
            disposition,
        ));
        let native_sequence =
            u64::try_from(self.native_observations.len()).unwrap_or(u64::MAX).saturating_add(1);
        self.native_observations.push(MacosObservation::new(
            native_sequence,
            self.manifest.plan_digest(),
            self.manifest.descriptor_digest(),
            self.manifest.preparation_digest(),
            self.manifest.profile_digest(),
            event,
            None,
            None,
            None,
            status,
        ));
        Ok(())
    }
}

//! Activated-session lifecycle transitions and bound observations.

use peritus_process::{CancellationReason, OsExitObservation, ProcessTreeIdentity};
use peritus_sandbox::{EnforcementObservation, ObservationDisposition, ObservationKind};

use super::{MacosSession, SessionPhase, TerminationReason, adapter::lifecycle_error};
use crate::{
    MacosError, MacosErrorKind, MacosObservation, ObservationEvent, ObservationStatus,
    RecoveryAction,
};

impl MacosSession {
    /// Binds the exact helper birth and starts its one-use execution observer before delivery.
    ///
    /// # Errors
    /// Rejects duplicate spawn custody or an incomplete/reusable process identity.
    pub fn record_spawned(&mut self, tree: ProcessTreeIdentity) -> Result<(), MacosError> {
        if self.phase != SessionPhase::Prepared || self.recovery.identity().root_pid().is_some() {
            return Err(lifecycle_error("helper spawn custody was already established"));
        }
        let root_pid = tree.root_pid();
        if root_pid == 0
            || tree.start_token().is_none()
            || tree.process_group() != Some(root_pid)
            || !tree.complete_containment()
        {
            return Err(MacosError::new(
                MacosErrorKind::SupervisorFailure,
                MacosOperation::Activate,
                RecoveryAction::CancelAndReap,
                "C2 did not establish an exact helper birth and process group",
            ));
        }
        self.exec_status.observe_spawned(
            tree,
            self.manifest.digest(),
            self.manifest.preparation_digest(),
        )?;
        self.cleanup.mark_support_started();
        self.recovery.record_spawned(tree, self.cleanup)
    }

    /// Activates only after C2 verified the helper handshake and complete process containment.
    ///
    /// # Errors
    /// Rejects an invalid phase, absent process group, or incomplete descendant containment.
    pub fn record_activation(&mut self, tree: ProcessTreeIdentity) -> Result<(), MacosError> {
        self.record_activation_while(tree, &mut || true)
    }

    #[cfg(all(test, unix))]
    pub(crate) fn record_test_activation(
        &mut self,
        tree: ProcessTreeIdentity,
    ) -> Result<(), MacosError> {
        self.record_spawned(tree)?;
        crate::exec_status::report_test_success(
            self.manifest.exec_status_descriptor(),
            self.manifest.digest(),
            self.manifest.preparation_digest(),
        )?;
        self.record_activation(tree)
    }

    /// Activates after an explicit target-exec frame while observing owner cancellation.
    ///
    /// # Errors
    /// Rejects mismatched custody, malformed execution status, or owner cancellation.
    pub fn record_activation_while(
        &mut self,
        tree: ProcessTreeIdentity,
        should_continue: &mut dyn FnMut() -> bool,
    ) -> Result<(), MacosError> {
        if self.phase != SessionPhase::Prepared {
            return Err(lifecycle_error("activation requires a prepared session"));
        }
        let root_pid = tree.root_pid();
        let identity = self.recovery.identity();
        let owned_process_group = root_pid != 0 && tree.process_group() == Some(root_pid);
        if !crate::verified::activation_permitted(
            tree.complete_containment(),
            owned_process_group,
            true,
        ) || identity.root_pid() != Some(root_pid)
            || identity.root_start_token() != tree.start_token()
            || identity.process_group() != tree.process_group()
        {
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
        self.exec_status.observe_while(
            self.manifest.digest(),
            self.manifest.preparation_digest(),
            should_continue,
        )?;
        self.phase = SessionPhase::Active;
        self.recovery.record_activation()?;
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
    /// Rejects cancellation outside a spawned/active/cancelling session.
    pub fn record_cancellation(&mut self, reason: CancellationReason) -> Result<(), MacosError> {
        match self.phase {
            SessionPhase::Prepared if self.recovery.identity().root_pid().is_some() => {
                self.recovery.record_cancellation(reason)?;
                self.phase = SessionPhase::Cancelling;
                self.cancellation = Some(reason);
                self.push_lifecycle(
                    ObservationKind::Cancellation,
                    ObservationEvent::CancelRequested,
                    ObservationDisposition::Accepted,
                    ObservationStatus::Accepted,
                )
            }
            SessionPhase::Active => {
                self.recovery.record_cancellation(reason)?;
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
    /// Rejects termination outside an active/cancelling phase.
    pub fn record_termination(&mut self, exit: &OsExitObservation) -> Result<(), MacosError> {
        if !matches!(self.phase, SessionPhase::Active | SessionPhase::Cancelling) {
            return Err(lifecycle_error("termination requires an active or cancelling session"));
        }
        let termination = match exit {
            OsExitObservation::Code(code) => TerminationReason::TargetExit(*code),
            OsExitObservation::Unavailable => TerminationReason::Unavailable,
            OsExitObservation::Signal(_)
            | OsExitObservation::SignalName(_)
            | OsExitObservation::PlatformException(_) => TerminationReason::Signalled,
        };
        self.recovery.record_termination(termination)?;
        self.termination = Some(termination);
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
        let sequence = self.next_observation_sequence;
        self.next_observation_sequence = sequence
            .checked_add(1)
            .ok_or_else(|| lifecycle_error("native observation sequence overflowed"))?;
        let observation = EnforcementObservation::new(
            sequence,
            self.manifest.plan_digest(),
            self.manifest.descriptor_digest(),
            kind,
            None,
            disposition,
        );
        self.pending_observations.push_back(observation);
        self.observations.push(observation);
        if let Some(next) = self.next_native_observation_sequence.checked_add(1) {
            self.native_observations.push(MacosObservation::new(
                self.next_native_observation_sequence,
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
            self.next_native_observation_sequence = next;
        }
        Ok(())
    }
}

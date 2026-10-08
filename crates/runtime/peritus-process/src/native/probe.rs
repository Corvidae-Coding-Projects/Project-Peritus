//! Production operating-system probe for durable process recovery.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;

use crate::{
    ErrorCode, ProbeObservation, ProcessError, ProcessOperation, ProcessProbe, ProcessTreeIdentity,
    ProcessTreeQuiescence, RecoveryClass,
    platform::current_start_token,
};
#[cfg(windows)]
use crate::NativeWindowsContainmentRecovery;

/// Native exact-birth-identity probe used by durable process-store reconciliation.
///
/// A live classification requires the persisted process birth token, the platform's current token,
/// and the persisted containment identity to agree. Missing or inaccessible facts remain
/// [`ProbeObservation::Unverifiable`]. Termination performs a fresh observation immediately before
/// issuing an operating-system request.
///
/// Linux tokens are `/proc/<pid>/stat` start ticks, macOS tokens are process-start microseconds,
/// and Windows tokens are creation-time `FILETIME` ticks. Unix recovery terminates only a complete
/// root-led process group. Windows root-only recovery remains observational; persisted named Job
/// containment is reopened through the containment-specific probe hooks.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeProcessProbe;

impl NativeProcessProbe {
    /// Creates a stateless native recovery probe.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Captures the operating-system birth identity of a newly spawned isolated child.
    ///
    /// Unix callers must have created the child as the leader of its own process group; that
    /// group is complete containment for recovery. Windows records the exact root birth identity,
    /// but reports incomplete containment until the caller supplies a durable Job Object owner.
    ///
    /// # Errors
    /// Returns an indeterminate recovery error when the child's birth token cannot be observed.
    pub fn capture_isolated_child(
        &mut self,
        root_pid: u32,
    ) -> Result<ProcessTreeIdentity, ProcessError> {
        let start_token = current_start_token(root_pid).ok_or_else(|| {
            indeterminate("native child birth identity could not be observed after spawn")
        })?;
        #[cfg(unix)]
        let identity = ProcessTreeIdentity::new(
            root_pid,
            Some(start_token),
            Some(root_pid),
            true,
        );
        #[cfg(windows)]
        let identity = ProcessTreeIdentity::new(root_pid, Some(start_token), None, false);
        #[cfg(unix)]
        if platform::observe(identity)? != ProbeObservation::ExactLive {
            return Err(indeterminate(
                "native child identity changed before ownership could be published",
            ));
        }
        Ok(identity)
    }
}

impl ProcessProbe for NativeProcessProbe {
    fn observe(&mut self, identity: ProcessTreeIdentity) -> Result<ProbeObservation, ProcessError> {
        platform::observe(identity)
    }

    fn observe_quiescence(
        &mut self,
        identity: ProcessTreeIdentity,
    ) -> Result<ProcessTreeQuiescence, ProcessError> {
        platform::observe_quiescence(identity)
    }

    fn terminate(&mut self, identity: ProcessTreeIdentity) -> Result<(), ProcessError> {
        platform::terminate(identity)
    }

    #[cfg(windows)]
    fn observe_windows_containment(
        &mut self,
        containment: &NativeWindowsContainmentRecovery,
    ) -> Result<ProbeObservation, ProcessError> {
        match containment {
            NativeWindowsContainmentRecovery::Intended(binding) => {
                crate::NativeWindowsProcessOwner::observe_durable_binding(binding)
            }
            NativeWindowsContainmentRecovery::Observed(identity) => {
                crate::NativeWindowsProcessOwner::observe_durable(identity)
            }
        }
    }

    #[cfg(windows)]
    fn observe_windows_containment_quiescence(
        &mut self,
        containment: &NativeWindowsContainmentRecovery,
    ) -> Result<ProcessTreeQuiescence, ProcessError> {
        match containment {
            NativeWindowsContainmentRecovery::Intended(binding) => {
                crate::NativeWindowsProcessOwner::observe_durable_binding_quiescence(binding)
            }
            NativeWindowsContainmentRecovery::Observed(identity) => {
                crate::NativeWindowsProcessOwner::observe_durable_quiescence(identity)
            }
        }
    }

    #[cfg(windows)]
    fn terminate_windows_containment(
        &mut self,
        containment: &NativeWindowsContainmentRecovery,
    ) -> Result<(), ProcessError> {
        match containment {
            NativeWindowsContainmentRecovery::Observed(identity) => {
                crate::NativeWindowsProcessOwner::terminate_durable(identity)
            }
            NativeWindowsContainmentRecovery::Intended(_) => Err(indeterminate(
                "Windows Job termination requires an adopted target birth identity",
            )),
        }
    }
}

pub(super) const fn indeterminate(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Indeterminate,
        ProcessOperation::Reconcile,
        RecoveryClass::ReopenAndReconcile,
        detail,
    )
}

pub(super) fn indeterminate_cause(
    detail: &'static str,
    cause: impl std::error::Error + Send + Sync + 'static,
) -> ProcessError {
    ProcessError::with_source(
        ErrorCode::Indeterminate,
        ProcessOperation::Reconcile,
        RecoveryClass::ReopenAndReconcile,
        detail,
        cause,
    )
}

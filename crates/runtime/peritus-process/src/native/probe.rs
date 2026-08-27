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
    RecoveryClass,
};

/// Native exact-birth-identity probe used by durable process-store reconciliation.
///
/// A live classification requires the persisted process birth token, the platform's current token,
/// and the persisted containment identity to agree. Missing or inaccessible facts remain
/// [`ProbeObservation::Unverifiable`]. Termination performs a fresh observation immediately before
/// issuing an operating-system request.
///
/// Linux tokens are `/proc/<pid>/stat` start ticks, macOS tokens are process-start microseconds,
/// and Windows tokens are creation-time `FILETIME` ticks. Unix recovery terminates only a complete
/// root-led process group. Windows can re-observe the exact root, but exact tree termination
/// remains indeterminate unless C2 later persists a reopenable job-object identity.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeProcessProbe;

impl NativeProcessProbe {
    /// Creates a stateless native recovery probe.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ProcessProbe for NativeProcessProbe {
    fn observe(&mut self, identity: ProcessTreeIdentity) -> Result<ProbeObservation, ProcessError> {
        platform::observe(identity)
    }

    fn terminate(&mut self, identity: ProcessTreeIdentity) -> Result<(), ProcessError> {
        platform::terminate(identity)
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

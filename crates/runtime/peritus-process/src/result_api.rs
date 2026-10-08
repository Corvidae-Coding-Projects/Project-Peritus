//! Public process result, recovery, quiescence, and resource observation surface.

pub use crate::quiescence::{HolderQuiescenceObservation, QuiescenceBlocker};
pub use crate::recovery::{
    ProbeObservation, ProcessProbe, ProcessTreeQuiescence, RecoveryDisposition, RecoveryEntry, RecoveryObservation,
    RecoveryReport,
};
pub use crate::resource::{
    ProcessResourceDimension, ProcessResourceObservation, ProcessResourcePolicy, ResourceFidelity,
};
pub use crate::terminal::{
    NativeFailureObservation, NativePostActivationFailure, OsExitObservation, OutputArtifact,
    OutputSummary, ProcessInstant, TerminalDisposition, TerminalRecovery, TerminalResult,
};

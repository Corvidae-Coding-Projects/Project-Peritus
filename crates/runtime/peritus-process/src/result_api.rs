//! Public process result, recovery, quiescence, and resource observation surface.

pub use crate::quiescence::{HolderQuiescenceObservation, QuiescenceBlocker};
pub use crate::recovery::{
    ProbeObservation, ProcessProbe, RecoveryDisposition, RecoveryEntry, RecoveryReport,
};
pub use crate::resource::{
    ProcessResourceDimension, ProcessResourceObservation, ProcessResourcePolicy, ResourceFidelity,
};
pub use crate::terminal::{
    OsExitObservation, OutputArtifact, OutputSummary, ProcessInstant, TerminalDisposition,
    TerminalRecovery, TerminalResult,
};

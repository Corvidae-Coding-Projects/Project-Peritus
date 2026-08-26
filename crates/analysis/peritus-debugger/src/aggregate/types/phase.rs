//! Closed durable debugger job phases.

use crate::DebuggerError;

/// Closed externally visible job phases.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DebuggerPhase {
    /// Job identity and immutable inputs are durable.
    Created,
    /// Exact trace selection is durable.
    Selected,
    /// Deterministic analysis is durable.
    DeterministicComplete,
    /// Model work is requested or waiting for a scheduled retry.
    ModelPending,
    /// One committed model attempt owns an active claim.
    ModelRunning,
    /// One strict model proposal passed complete validation.
    ModelValidated,
    /// A validated report is committed and ready for publication.
    ReportReady,
    /// The report artifact and evidence record are durable.
    Published,
    /// The job ended with a typed failure.
    Failed,
    /// Durable cancellation won.
    Cancelled,
}

impl DebuggerPhase {
    /// Returns whether no later success transition is legal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Published | Self::Failed | Self::Cancelled)
    }

    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Created => 1,
            Self::Selected => 2,
            Self::DeterministicComplete => 3,
            Self::ModelPending => 4,
            Self::ModelRunning => 5,
            Self::ModelValidated => 6,
            Self::ReportReady => 7,
            Self::Published => 8,
            Self::Failed => 9,
            Self::Cancelled => 10,
        }
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self, DebuggerError> {
        match tag {
            1 => Ok(Self::Created),
            2 => Ok(Self::Selected),
            3 => Ok(Self::DeterministicComplete),
            4 => Ok(Self::ModelPending),
            5 => Ok(Self::ModelRunning),
            6 => Ok(Self::ModelValidated),
            7 => Ok(Self::ReportReady),
            8 => Ok(Self::Published),
            9 => Ok(Self::Failed),
            10 => Ok(Self::Cancelled),
            _ => Err(super::invalid("unknown debugger phase tag")),
        }
    }
}

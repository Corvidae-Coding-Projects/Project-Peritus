//! Stable user-facing product-run phases.

/// User-facing phase of one daemon-owned coding run.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunPhase {
    /// Accepted and waiting for execution.
    Queued,
    /// The writer is preparing and applying an implementation.
    Writing,
    /// Repository gates are running.
    Checking,
    /// An independent reviewer is inspecting the result.
    Reviewing,
    /// The fixer is addressing test or review findings.
    Fixing,
    /// Final gates and review are running.
    Verifying,
    /// The run completed with passing gates and no blocking findings.
    Complete,
    /// The run stopped with an actionable failure.
    Failed,
    /// The user cancelled the run.
    Cancelled,
    /// A daemon restart interrupted work that may be retried.
    RecoveryRequired,
    /// The agent asked a material question and is waiting for a reply.
    WaitingForUser,
}

impl ProductRunPhase {
    /// Stable wire tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Queued => 1,
            Self::Writing => 2,
            Self::Checking => 3,
            Self::Reviewing => 4,
            Self::Fixing => 5,
            Self::Verifying => 6,
            Self::Complete => 7,
            Self::Failed => 8,
            Self::Cancelled => 9,
            Self::RecoveryRequired => 10,
            Self::WaitingForUser => 11,
        }
    }

    /// Decodes a stable wire tag.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Queued),
            2 => Some(Self::Writing),
            3 => Some(Self::Checking),
            4 => Some(Self::Reviewing),
            5 => Some(Self::Fixing),
            6 => Some(Self::Verifying),
            7 => Some(Self::Complete),
            8 => Some(Self::Failed),
            9 => Some(Self::Cancelled),
            10 => Some(Self::RecoveryRequired),
            11 => Some(Self::WaitingForUser),
            _ => None,
        }
    }

    /// Returns whether no more work can run without explicit user input.
    #[must_use]
    pub const fn terminal(self) -> bool {
        matches!(
            self,
            Self::Complete
                | Self::Failed
                | Self::Cancelled
                | Self::RecoveryRequired
                | Self::WaitingForUser
        )
    }

    /// Returns whether the original request may be started again unchanged.
    #[must_use]
    pub const fn retryable(self) -> bool {
        matches!(self, Self::Failed | Self::Cancelled | Self::RecoveryRequired)
    }
}

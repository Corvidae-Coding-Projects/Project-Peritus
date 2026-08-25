//! Durable B1 commit port and redaction-safe lifecycle failures.

use core::fmt;

use peritus_budget::{BudgetCommand, BudgetError, BudgetReceipt};

/// Commit boundary used by D0 to apply B1 commands before or after an external effect.
///
/// Implementations are responsible for committing the accepted B1 transition through C0 before
/// returning its receipt. A pure in-memory ledger is suitable only for tests.
pub trait AgentBudgetPort: Send {
    /// Applies and durably commits exactly one B1 command.
    ///
    /// # Errors
    ///
    /// Returns the B1 rejection or a redaction-safe persistence failure.
    fn commit(&mut self, command: BudgetCommand) -> Result<BudgetReceipt, AgentBudgetPortError>;
}

/// Failure reported by the durable B1/C0 adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentBudgetPortError {
    /// The verified B1 reducer rejected the command.
    Rejected(BudgetError),
    /// The accepted transition could not be durably committed.
    CommitFailed(crate::SafeText),
}

impl AgentBudgetPortError {
    /// Creates a bounded redaction-safe commit failure.
    #[must_use]
    pub const fn commit_failed(detail: crate::SafeText) -> Self {
        Self::CommitFailed(detail)
    }
}

impl From<BudgetError> for AgentBudgetPortError {
    fn from(value: BudgetError) -> Self {
        Self::Rejected(value)
    }
}

impl fmt::Display for AgentBudgetPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(error) => formatter.write_str(error.kind().code()),
            Self::CommitFailed(detail) => formatter.write_str(detail.as_str()),
        }
    }
}

impl std::error::Error for AgentBudgetPortError {}

/// D0-side failure while enforcing a B1 reservation lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentBudgetError {
    /// The durable budget adapter rejected or could not commit a command.
    Port(AgentBudgetPortError),
    /// The caller attempted a budget operation in the wrong lifecycle phase.
    InvalidPhase,
    /// A variable-use ceiling was empty or tried to reserve attempt/retry dimensions.
    InvalidPlan,
    /// A port returned a receipt for a different operation or reservation.
    ReceiptMismatch,
    /// Final provider usage omitted a dimension whose reservation ceiling is nonzero.
    IncompleteFinalUsage,
    /// Provider usage could not be combined without integer overflow.
    UsageOverflow,
}

impl fmt::Display for AgentBudgetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Port(error) => fmt::Display::fmt(error, formatter),
            Self::InvalidPhase => formatter.write_str("agent budget lifecycle phase is invalid"),
            Self::InvalidPlan => formatter.write_str(
                "agent budget variable-use ceiling is empty or contains attempt charges",
            ),
            Self::ReceiptMismatch => {
                formatter.write_str("budget receipt does not match the requested operation")
            }
            Self::IncompleteFinalUsage => {
                formatter.write_str("final provider usage omits a reserved accounting dimension")
            }
            Self::UsageOverflow => formatter.write_str("provider usage accounting overflowed"),
        }
    }
}

impl std::error::Error for AgentBudgetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Port(error) => Some(error),
            Self::InvalidPhase
            | Self::InvalidPlan
            | Self::ReceiptMismatch
            | Self::IncompleteFinalUsage
            | Self::UsageOverflow => None,
        }
    }
}

impl From<AgentBudgetPortError> for AgentBudgetError {
    fn from(value: AgentBudgetPortError) -> Self {
        Self::Port(value)
    }
}

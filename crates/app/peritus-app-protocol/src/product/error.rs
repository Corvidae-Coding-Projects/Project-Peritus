//! Product-run message construction failures.

use core::fmt;

/// Failure to construct a bounded product-run message.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductRunMessageError {
    /// A required string is empty or whitespace-only.
    Empty,
    /// A string exceeds its compiled protocol bound.
    TooLong,
    /// A conversation exceeds its retained message limit.
    TooManyMessages,
    /// A deliverable contains too many paths or commands, lacks changed paths, or an accepted E0
    /// deliverable lacks a successful command.
    TooManyDeliverableItems,
    /// A deliverable path is absolute, traversing, or targets Git metadata.
    InvalidDeliverablePath,
    /// A settlement and its product snapshot disagree about candidate identity or qualification.
    InvalidSettlement,
}

impl fmt::Display for ProductRunMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "product run text is empty",
            Self::TooLong => "product run text exceeds its protocol bound",
            Self::TooManyMessages => "product run conversation has too many messages",
            Self::TooManyDeliverableItems => {
                "product deliverable path or command collection is invalid"
            }
            Self::InvalidDeliverablePath => "product deliverable contains an unsafe path",
            Self::InvalidSettlement => "product run settlement disagrees with its snapshot",
        })
    }
}

impl std::error::Error for ProductRunMessageError {}

pub(super) fn bounded_text(value: &str, maximum: usize) -> Result<(), ProductRunMessageError> {
    if value.trim().is_empty() {
        Err(ProductRunMessageError::Empty)
    } else {
        optional_bounded_text(value, maximum)
    }
}

pub(super) const fn optional_bounded_text(
    value: &str,
    maximum: usize,
) -> Result<(), ProductRunMessageError> {
    if value.len() > maximum { Err(ProductRunMessageError::TooLong) } else { Ok(()) }
}

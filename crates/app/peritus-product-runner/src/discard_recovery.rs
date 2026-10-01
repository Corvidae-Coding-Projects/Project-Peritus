//! Read-only classification of one explicitly authorized discard transaction.

use std::path::PathBuf;

/// Durable discard progress. Inspection never resumes filesystem effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscardTransactionState {
    /// Intent is durable; destructive restoration has not been admitted.
    Prepared,
    /// Restoration may be partial and requires explicit, fenced retry.
    Restoring,
    /// Every restore effect completed; these directories preserve discarded history.
    Completed(Vec<PathBuf>),
}

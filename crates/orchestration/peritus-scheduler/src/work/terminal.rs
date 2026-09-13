//! Truthful terminal work outcomes.

#![cfg_attr(
    verus_keep_ghost,
    allow(
        missing_docs,
        reason = "pinned Verus generates undocumented public ghost projection methods"
    )
)]

use crate::{DispatchId, WorkId};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Truthful terminal work outcome.
#[derive(Debug, Eq, PartialEq)]
pub enum WorkTerminal {
    /// Work completed with exact inert result digest.
    Succeeded {
        /// Digest of the inert result.
        result_digest: Sha256Digest,
    },
    /// Work failed with exact inert failure digest.
    Failed {
        /// Digest of the inert failure record.
        failure_digest: Sha256Digest,
    },
    /// A canonical prerequisite could not succeed.
    DependencyFailed {
        /// Canonical prerequisite that could not succeed.
        dependency: WorkId,
    },
    /// Work was cancelled before or during execution.
    Cancelled,
    /// Lost ownership left an unknowable external outcome.
    Ambiguous {
        /// Dispatch whose external outcome is unknowable.
        dispatch_id: DispatchId,
    },
    /// Bounded attempts were exhausted.
    Exhausted {
        /// Digest explaining attempt exhaustion.
        cause_digest: Sha256Digest,
    },
    /// An active reservation was explicitly abandoned.
    Abandoned {
        /// Digest explaining explicit abandonment.
        cause_digest: Sha256Digest,
    },
}

} // verus!

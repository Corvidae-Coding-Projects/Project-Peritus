//! Work outcomes emitted by worker-loss classification.

use crate::{DispatchId, WorkId};
use vstd::prelude::*;

/// Deterministic work outcome caused by one worker-loss event.
// Direct verification keeps this Rust declaration checked without synthesizing unused
// enum-field projection methods, whose documentation is lost by the pinned Verus macro.
#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[derive(Debug, Eq, PartialEq)]
pub enum LossOutcome {
    /// Safe ownership loss released and requeued the work.
    Requeued {
        /// Released dispatch identity.
        dispatch_id: DispatchId,
        /// Work returned to the queue.
        work_id: WorkId,
    },
    /// Attempt bound was reached during safe-retry classification.
    Exhausted {
        /// Released dispatch identity.
        dispatch_id: DispatchId,
        /// Work whose attempt bound was reached.
        work_id: WorkId,
    },
    /// External result became ambiguous.
    Ambiguous {
        /// Released dispatch identity.
        dispatch_id: DispatchId,
        /// Work with unknowable external outcome.
        work_id: WorkId,
    },
    /// Loss policy classified work as failed.
    Failed {
        /// Released dispatch identity.
        dispatch_id: DispatchId,
        /// Work classified as failed.
        work_id: WorkId,
    },
    /// Cancellation already dominated the lost ownership.
    Cancelled {
        /// Released dispatch identity.
        dispatch_id: DispatchId,
        /// Work whose cancellation dominated the loss.
        work_id: WorkId,
    },
}

verus! {

impl Clone for LossOutcome {
    fn clone(&self) -> (result: Self)
        ensures result == *self,
    {
        match self {
            Self::Requeued { dispatch_id, work_id } => Self::Requeued {
                dispatch_id: *dispatch_id, work_id: *work_id,
            },
            Self::Exhausted { dispatch_id, work_id } => Self::Exhausted {
                dispatch_id: *dispatch_id, work_id: *work_id,
            },
            Self::Ambiguous { dispatch_id, work_id } => Self::Ambiguous {
                dispatch_id: *dispatch_id, work_id: *work_id,
            },
            Self::Failed { dispatch_id, work_id } => Self::Failed {
                dispatch_id: *dispatch_id, work_id: *work_id,
            },
            Self::Cancelled { dispatch_id, work_id } => Self::Cancelled {
                dispatch_id: *dispatch_id, work_id: *work_id,
            },
        }
    }
}

} // verus!

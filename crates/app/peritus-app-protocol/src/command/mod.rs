//! Exact B3 command submission binding, final results, and bounded idempotency.

mod binding;
mod idempotency;
mod result;

pub use binding::{CommandBinding, CommandSubmissionFrames, ExactB3Frame, RequestDigest};
pub use idempotency::{
    IdempotencyAdmission, IdempotencyEntry, IdempotencyRecordDisposition, IdempotencyWindow,
};
pub use result::{CommandDisposition, CommandResult, CommittedEventRange};

#[cfg(verus_only)]
pub use result::valid_committed_event_range;

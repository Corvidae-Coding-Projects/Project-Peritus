//! Checked typed patches and recoverable filesystem transactions for Peritus.
//!
//! A [`PatchSet`] is inert data. It becomes a [`PatchPlan`] only after exact workspace identity,
//! generation, and revision validation. The transaction adapter accepts plans, never unchecked
//! patch sets, and reports success only after every final file is re-read and verified.

mod codec;
mod content;
mod error;
mod line_endings;
mod operation;
mod path;
mod plan;
mod preimage;
mod set;
mod transaction;
mod verified;

pub(crate) use codec::local_record_codec_limits;

pub use content::FinalFile;
pub use error::{ErrorCode, PatchError, PatchOperationContext, RecoveryClass, RollbackStatus};
pub use line_endings::LineEndingPolicy;
pub use operation::{PatchOperation, PatchOperationKind};
pub use path::{MAX_COMPONENT_BYTES, MAX_COMPONENTS, MAX_PATH_BYTES, WorkspacePath};
pub use plan::{PatchIdentity, PatchPlan};
pub use preimage::{FileMode, Preimage};
pub use set::{MAX_FILE_BYTES, MAX_PATCH_BYTES, MAX_PATCH_OPERATIONS, PatchSet};
pub use transaction::{
    AppliedPatch, RecoveryBinding, RecoveryOutcome, RecoveryState, TransactionFaultPoint,
    TransactionPhase, apply_patch, apply_patch_with_completion,
    apply_patch_with_completion_and_cancellation, cleanup_applied_transaction, recover_transaction,
    recover_transaction_with_completion,
};

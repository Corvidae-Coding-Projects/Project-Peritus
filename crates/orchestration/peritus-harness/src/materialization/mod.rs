//! Pure exact materialization planning and the narrow C1 executor.

mod error;
mod executor;
mod failure;
mod plan_codec;
mod plan_model;
mod planner;
mod receipt;
mod receipt_codec;
mod snapshot;

pub use error::{MaterializationError, MaterializationErrorKind, MaterializationRecovery};
pub use executor::{
    AuthorizationActions, MaterializationAuthorizationPayloads, MaterializationResult,
    execute_plan, materialization_authorization_payloads,
};
pub use failure::{MaterializationFailure, MaterializationFailureCode};
pub use plan_model::{
    MaterializationPlan, MaterializationPlanId, MaterializationReason, PlannedFileOperation,
};
pub use receipt::{MaterializationReceipt, MaterializationReceiptId, ReceiptFile};
pub use snapshot::{ObservedFile, ObservedTarget, WorkspaceSnapshot};

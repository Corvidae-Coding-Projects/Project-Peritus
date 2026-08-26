//! Durable checked harness definitions and exact C1 materialization.

pub mod aggregate;
pub mod domain;
pub mod durability;
pub mod manifest;
pub mod materialization;
pub mod projection;
pub mod replay;
pub mod runtime;
pub mod wire;

pub use aggregate::{
    AggregateError, AggregateErrorKind, AggregateRecovery, DeliveryState, HarnessCommand,
    HarnessCommandKind, HarnessEvent, HarnessEventKind, HarnessState, HarnessTransition,
    PendingMaterialization, ReconciliationDecision, decide,
};
pub use durability::{
    DirectiveClaim, DurabilityError, DurabilityErrorKind, DurabilityRecovery,
    HARNESS_STATE_NAMESPACE, HarnessReplay, commit_harness_settlement, commit_harness_transition,
    harness_aggregate_key, harness_state_key, load_harness_replay,
};
pub use manifest::{
    CheckedLoadedHarness, HarnessManifest, LoadedHarness, ManifestError, ManifestErrorKind,
    load_harness,
};
pub use materialization::{
    AuthorizationActions, MaterializationAuthorizationPayloads, MaterializationError,
    MaterializationErrorKind, MaterializationFailure, MaterializationFailureCode,
    MaterializationPlan, MaterializationPlanId, MaterializationReason, MaterializationReceipt,
    MaterializationReceiptId, MaterializationRecovery, MaterializationResult, ObservedFile,
    ObservedTarget, PlannedFileOperation, ReceiptFile, WorkspaceSnapshot, execute_plan,
    materialization_authorization_payloads,
};
pub use projection::HarnessProjection;
pub use runtime::{
    ArtifactReader, CommittedPlan, GoverningHarnessBinding, GoverningHarnessBindingError,
    HarnessRuntime, MaterializationTiming, PlanCommitEvidence, PlanningOutcome,
    RuntimeAuthorizations, RuntimeError, RuntimeErrorKind, RuntimeOutcome, SettlementIds,
    VerifiedArtifact,
};
pub use wire::{HarnessCommandFrame, HarnessEventFrame, HarnessStateFrame};

//! Runtime type-state evidence, outcomes, and typed failures.

use core::fmt;

use peritus_journal::{CommittedBatch, StoreId};
use peritus_types::{CommandId, EventId};
use peritus_workspace::WorkspaceAuthorizationRequest;

use crate::{
    aggregate::HarnessState,
    materialization::{
        AuthorizationActions, MaterializationFailure, MaterializationPlan, MaterializationReceipt,
    },
};

/// Stable runtime-driver failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeErrorKind {
    /// Caller supplied a non-plan command or inconsistent timing/claim data.
    InvalidInput,
    /// The pure aggregate rejected a proposed plan or settlement.
    Aggregate,
    /// C0 did not durably commit the requested transition.
    Durability,
    /// A C1 outcome occurred but its required durable settlement did not commit.
    Settlement,
}

/// Typed runtime error. No value of this type reports materialization success.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeError {
    kind: RuntimeErrorKind,
    detail: String,
}

impl RuntimeError {
    pub(super) fn new(kind: RuntimeErrorKind, detail: impl Into<String>) -> Self {
        Self { kind, detail: detail.into() }
    }
    /// Returns the stable category.
    #[must_use]
    pub const fn kind(&self) -> RuntimeErrorKind {
        self.kind
    }
    /// Returns bounded diagnostic context.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "harness runtime failed ({:?}): {}", self.kind, self.detail)
    }
}

impl std::error::Error for RuntimeError {}

/// Separate target-owned patch and candidate authorization inputs.
#[derive(Clone, Copy)]
pub struct RuntimeAuthorizations<'a> {
    pub(super) patch: &'a WorkspaceAuthorizationRequest<'a>,
    pub(super) candidate: &'a WorkspaceAuthorizationRequest<'a>,
    pub(super) actions: AuthorizationActions,
}

impl<'a> RuntimeAuthorizations<'a> {
    /// Binds the two complete authorization requests and their expected action identities.
    #[must_use]
    pub const fn new(
        patch: &'a WorkspaceAuthorizationRequest<'a>,
        candidate: &'a WorkspaceAuthorizationRequest<'a>,
        actions: AuthorizationActions,
    ) -> Self {
        Self { patch, candidate, actions }
    }
}

/// Caller-issued identities reserved for the durable success/failure settlement transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettlementIds {
    pub(super) command: CommandId,
    pub(super) event: EventId,
}

impl SettlementIds {
    /// Constructs exact settlement identities.
    #[must_use]
    pub const fn new(command: CommandId, event: EventId) -> Self {
        Self { command, event }
    }
}

/// Exact effect timing retained by a successful receipt or failure diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaterializationTiming {
    pub(super) started_at_millis: u64,
    pub(super) completed_at_millis: u64,
}

impl MaterializationTiming {
    /// Constructs monotonic effect timing.
    ///
    /// # Errors
    /// Rejects a completion time before the start time.
    pub fn new(started_at_millis: u64, completed_at_millis: u64) -> Result<Self, RuntimeError> {
        if completed_at_millis < started_at_millis {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidInput,
                "completion precedes start",
            ));
        }
        Ok(Self { started_at_millis, completed_at_millis })
    }
}

/// Type-state proof that a specific plan event/checkpoint/outbox transaction committed in C0.
#[derive(Debug, Eq, PartialEq)]
pub enum PlanCommitEvidence {
    /// Opaque evidence returned by the atomic planning append.
    Fresh(CommittedBatch),
    /// The plan was recovered from checked events and their exact matching checkpoint.
    Recovered {
        /// Exact C0 store from which replay evidence was loaded.
        store_id: StoreId,
    },
}

/// Type-state proof that a specific plan event/checkpoint/outbox transaction committed in C0.
#[derive(Debug)]
pub struct CommittedPlan {
    pub(super) plan: MaterializationPlan,
    pub(super) state: HarnessState,
    pub(super) evidence: PlanCommitEvidence,
}

impl CommittedPlan {
    /// Returns the exact durable plan.
    #[must_use]
    pub const fn plan(&self) -> &MaterializationPlan {
        &self.plan
    }
    /// Returns the authoritative post-plan checkpoint.
    #[must_use]
    pub const fn state(&self) -> &HarnessState {
        &self.state
    }
    /// Returns C0 evidence for the atomic planning commit or its checked restart replay.
    #[must_use]
    pub const fn evidence(&self) -> &PlanCommitEvidence {
        &self.evidence
    }
}

/// Result of checking idempotency and committing a proposed plan.
#[derive(Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "move-only planning evidence remains directly inspectable at the one-shot runtime boundary"
)]
pub enum PlanningOutcome {
    /// The same revision is already proven at the exact target snapshot.
    AlreadyMaterialized(MaterializationReceipt),
    /// The plan, checkpoint, roots, and stable directive committed before any C1 effect.
    Committed(CommittedPlan),
}

/// Durable terminal result of executing one claimed committed plan.
#[derive(Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "move-only terminal evidence remains directly inspectable at the one-shot runtime boundary"
)]
pub enum RuntimeOutcome {
    /// Complete C1 evidence and its atomic success settlement.
    Completed {
        /// Complete exact C1 materialization receipt.
        receipt: MaterializationReceipt,
        /// Authoritative post-settlement E1 state.
        state: HarnessState,
        /// Evidence that planning committed before C1 execution.
        planning: PlanCommitEvidence,
        /// C0 receipt for the atomic success settlement.
        settlement_batch: CommittedBatch,
    },
    /// Typed non-success evidence and its atomic failure settlement.
    Failed {
        /// Typed retained failure evidence.
        failure: MaterializationFailure,
        /// Authoritative post-settlement E1 state.
        state: HarnessState,
        /// Evidence that planning committed before C1 execution.
        planning: PlanCommitEvidence,
        /// C0 receipt for the atomic failure settlement.
        settlement_batch: CommittedBatch,
    },
}

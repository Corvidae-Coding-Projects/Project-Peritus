//! Accepted semantic events and exact successor-state transitions.

use peritus_types::{CommandId, EventId, HarnessId, Sha256Digest};

use crate::{
    domain::{HarnessRevision, RevisionDigest},
    materialization::{
        MaterializationFailure, MaterializationPlan, MaterializationPlanId, MaterializationReceipt,
        MaterializationReceiptId,
    },
};

use super::{HarnessState, ReconciliationDecision};

/// Closed family-80 semantic event vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant, reason = "closed semantic values remain directly inspectable")]
pub enum HarnessEventKind {
    /// A genesis revision became durable.
    GenesisRegistered {
        /// Complete checked genesis revision.
        revision: HarnessRevision,
    },
    /// A direct successor revision became durable.
    SuccessorRegistered {
        /// Complete checked successor revision.
        revision: HarnessRevision,
    },
    /// A plan and stable outbox directive became durable before C1 was called.
    MaterializationPlanned {
        /// Exact committed plan.
        plan: MaterializationPlan,
    },
    /// Directive delivery was acknowledged without settling materialization.
    DirectiveDeliveryAcknowledged {
        /// Exact pending plan.
        plan_id: MaterializationPlanId,
        /// Durable delivery observation time.
        delivered_at_millis: u64,
    },
    /// Exact C1 success evidence settled pending work.
    MaterializationRecorded {
        /// Complete checked C1 receipt.
        receipt: MaterializationReceipt,
    },
    /// A typed non-success outcome settled or quarantined pending work.
    MaterializationFailureRecorded {
        /// Typed materialization failure evidence.
        failure: MaterializationFailure,
    },
    /// Restart reconciliation classified one exact pending plan.
    PendingMaterializationReconciled {
        /// Exact pending plan.
        plan_id: MaterializationPlanId,
        /// Checked restart decision.
        decision: ReconciliationDecision,
    },
    /// A settled receipt left the bounded hot projection.
    SettledReceiptRetired {
        /// Exact retired receipt identity.
        receipt_id: MaterializationReceiptId,
    },
}

/// One fully bound accepted event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessEvent {
    id: EventId,
    command_id: CommandId,
    harness_id: HarnessId,
    sequence: u64,
    previous_event: Option<EventId>,
    prior_state_digest: Sha256Digest,
    command_digest: Sha256Digest,
    successor_state_digest: Sha256Digest,
    revision_digest: RevisionDigest,
    artifact_roots: Vec<Sha256Digest>,
    kind: HarnessEventKind,
}

impl HarnessEvent {
    #[allow(clippy::too_many_arguments, reason = "event integrity bindings remain explicit")]
    pub(crate) const fn new(
        id: EventId,
        command_id: CommandId,
        harness_id: HarnessId,
        sequence: u64,
        previous_event: Option<EventId>,
        prior_state_digest: Sha256Digest,
        command_digest: Sha256Digest,
        successor_state_digest: Sha256Digest,
        revision_digest: RevisionDigest,
        artifact_roots: Vec<Sha256Digest>,
        kind: HarnessEventKind,
    ) -> Self {
        Self {
            id,
            command_id,
            harness_id,
            sequence,
            previous_event,
            prior_state_digest,
            command_digest,
            successor_state_digest,
            revision_digest,
            artifact_roots,
            kind,
        }
    }

    /// Returns event identity.
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }
    /// Returns producing command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    /// Returns harness lineage.
    #[must_use]
    pub const fn harness_id(&self) -> HarnessId {
        self.harness_id
    }
    /// Returns positive aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Returns exact predecessor event, absent only at genesis.
    #[must_use]
    pub const fn previous_event(&self) -> Option<EventId> {
        self.previous_event
    }
    /// Returns exact prior-state digest.
    #[must_use]
    pub const fn prior_state_digest(&self) -> Sha256Digest {
        self.prior_state_digest
    }
    /// Returns complete producing-command digest.
    #[must_use]
    pub const fn command_digest(&self) -> Sha256Digest {
        self.command_digest
    }
    /// Returns exact successor-state digest.
    #[must_use]
    pub const fn successor_state_digest(&self) -> Sha256Digest {
        self.successor_state_digest
    }
    /// Returns the exact harness revision causally bound to this transition.
    #[must_use]
    pub const fn revision_digest(&self) -> RevisionDigest {
        self.revision_digest
    }
    /// Returns the canonical finalized artifact-root inventory for that revision.
    #[must_use]
    pub fn artifact_roots(&self) -> &[Sha256Digest] {
        &self.artifact_roots
    }
    /// Returns semantic event data.
    #[must_use]
    pub const fn kind(&self) -> &HarnessEventKind {
        &self.kind
    }
}

/// Accepted event paired with its exact complete successor state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessTransition {
    event: HarnessEvent,
    state: HarnessState,
}

impl HarnessTransition {
    pub(crate) const fn new(event: HarnessEvent, state: HarnessState) -> Self {
        Self { event, state }
    }

    /// Returns the semantic event.
    #[must_use]
    pub const fn event(&self) -> &HarnessEvent {
        &self.event
    }
    /// Returns the complete successor state.
    #[must_use]
    pub const fn state(&self) -> &HarnessState {
        &self.state
    }
    /// Consumes the transition.
    #[must_use]
    pub fn into_parts(self) -> (HarnessEvent, HarnessState) {
        (self.event, self.state)
    }
}

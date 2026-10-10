//! Host-owned semantic pins; model-authored entries never become policy or effect receipts.

use super::{
    ObservationId, ObservationKind, WorkingBinding, WorkingError, WorkingLimits, WorkingState,
};
use crate::ContextNodeId;

#[doc = " Unresolved operation disposition. None of these states authorizes (re)dispatch."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingOperationState {
    #[doc = " Proposal recorded before dispatch; effects have not been established."]
    Proposed,
    #[doc = " Host observed an active asynchronous operation handle."]
    Running,
    #[doc = " Outcome is unknown; recovery belongs to the existing effect owner."]
    Unknown,
}
#[doc = " One unresolved operation and the exact observation establishing its current disposition."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingPendingOperation {
    id: ContextNodeId,
    source: ObservationId,
    state: PendingOperationState,
}
impl WorkingPendingOperation {
    #[doc = " Records a host-observed operation disposition; source existence is checked on application."]
    #[must_use]
    pub const fn new(
        id: ContextNodeId,
        source: ObservationId,
        state: PendingOperationState,
    ) -> Self {
        Self { id, source, state }
    }
    #[doc = " Stable host-derived operation identity."]
    #[must_use]
    pub const fn id(self) -> ContextNodeId {
        self.id
    }
    #[doc = " Exact proposal or tool-result observation."]
    #[must_use]
    pub const fn source(self) -> ObservationId {
        self.source
    }
    #[doc = " Unresolved state, never an effect authorization or receipt."]
    #[must_use]
    pub const fn state(self) -> PendingOperationState {
        self.state
    }
}
#[doc = " Current literal instruction references and unresolved protocol obligations."]
#[derive(Debug, Eq, PartialEq)]
pub struct WorkingProtocol {
    requirements: Vec<ObservationId>,
    pending: Vec<WorkingPendingOperation>,
}
impl Clone for WorkingProtocol {
    fn clone(&self) -> Self {
        Self { requirements: self.requirements.clone(), pending: self.pending.clone() }
    }
}
impl WorkingProtocol {
    pub(super) const fn empty() -> Self {
        Self { requirements: Vec::new(), pending: Vec::new() }
    }
    #[doc = " Validates canonical order and allocation bounds for host-owned pins."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects excessive, repeated, or noncanonically ordered references."]
    pub fn new(
        requirements: Vec<ObservationId>,
        pending: Vec<WorkingPendingOperation>,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        if requirements.len() > limits.entries() || pending.len() > limits.entries() {
            return Err(WorkingError::Capacity);
        }
        let mut index = 1;
        while index < requirements.len() {
            if requirements[index - 1] >= requirements[index] {
                return Err(WorkingError::NonCanonicalOrder);
            }
            index += 1;
        }
        index = 1;
        while index < pending.len() {
            if pending[index - 1].id >= pending[index].id {
                return Err(WorkingError::NonCanonicalOrder);
            }
            index += 1;
        }
        Ok(Self { requirements, pending })
    }
    #[doc = " Exact active host-policy/user-instruction handles. Prior versions remain in the archive."]
    #[must_use]
    pub const fn requirements(&self) -> &[ObservationId] {
        self.requirements.as_slice()
    }
    #[doc = " Unresolved operations to pin even when their originating exchange leaves the prompt."]
    #[must_use]
    pub const fn pending(&self) -> &[WorkingPendingOperation] {
        self.pending.as_slice()
    }
}
#[doc = " Host-only replacement of semantic pins, separately typed from model-authored deltas."]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingProtocolUpdate {
    binding: WorkingBinding,
    base_revision: u64,
    protocol: WorkingProtocol,
}
impl WorkingProtocolUpdate {
    #[doc = " Binds host-owned pins to one exact state revision."]
    #[must_use]
    pub const fn new(
        binding: WorkingBinding,
        base_revision: u64,
        protocol: WorkingProtocol,
    ) -> Self {
        Self { binding, base_revision, protocol }
    }
    #[doc = " Exact current task/role/conversation binding."]
    #[must_use]
    pub const fn binding(&self) -> WorkingBinding {
        self.binding
    }
    #[doc = " Exact revision on which the host based the update."]
    #[must_use]
    pub const fn base_revision(&self) -> u64 {
        self.base_revision
    }
    #[doc = " Complete bounded replacement pins."]
    #[must_use]
    pub const fn protocol(&self) -> &WorkingProtocol {
        &self.protocol
    }
}
#[doc = " Validates and replaces host pins without accepting model authority or executing effects."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = " Rejects scope/revision drift, missing sources, forged instruction origins, and capacity."]
pub fn apply_working_protocol(
    state: &WorkingState,
    update: &WorkingProtocolUpdate,
) -> Result<WorkingState, WorkingError> {
    state.check_binding(update.binding)?;
    if state.revision() != update.base_revision {
        return Err(WorkingError::RevisionMismatch);
    }
    validate_protocol(state, update.protocol())?;
    let revision = super::state_revision::next_revision(state.revision())?;
    let next = state.with_protocol(update.protocol().clone(), revision);
    Ok(next)
}
pub(super) fn validate_protocol(
    state: &WorkingState,
    protocol: &WorkingProtocol,
) -> Result<(), WorkingError> {
    if protocol.requirements.len() > state.limits.entries()
        || protocol.pending.len() > state.limits.entries()
    {
        return Err(WorkingError::Capacity);
    }
    let mut index = 0;
    while index < protocol.requirements.len() {
        let source = state.observation(state.binding(), protocol.requirements[index])?;
        if !matches!(source.kind(), ObservationKind::HostPolicy | ObservationKind::UserInstruction)
        {
            return Err(WorkingError::ConflictingEvidence);
        }
        index += 1;
    }
    index = 0;
    while index < protocol.pending.len() {
        let source = state.observation(state.binding(), protocol.pending[index].source)?;
        if !matches!(source.kind(), ObservationKind::AgentMessage | ObservationKind::ToolOutput) {
            return Err(WorkingError::ConflictingEvidence);
        }
        index += 1;
    }
    Ok(())
}

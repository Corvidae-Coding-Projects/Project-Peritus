//! Host-owned semantic pins; model-authored entries never become policy or effect receipts.

use super::{ObservationId, ObservationKind, WorkingBinding, WorkingError, WorkingLimits, WorkingState};
use crate::ContextNodeId;
use vstd::prelude::*;

verus! {
/// Unresolved operation disposition. None of these states authorizes (re)dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingOperationState {
    /// Proposal recorded before dispatch; effects have not been established.
    Proposed,
    /// Host observed an active asynchronous operation handle.
    Running,
    /// Outcome is unknown; recovery belongs to the existing effect owner.
    Unknown,
}

/// One unresolved operation and the exact observation establishing its current disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingPendingOperation {
    id: ContextNodeId,
    source: ObservationId,
    state: PendingOperationState,
}
impl WorkingPendingOperation {
    /// Records a host-observed operation disposition; source existence is checked on application.
    #[must_use]
    pub const fn new(id: ContextNodeId, source: ObservationId, state: PendingOperationState) -> Self { Self { id, source, state } }
    /// Stable host-derived operation identity.
    #[must_use]
    pub const fn id(self) -> ContextNodeId { self.id }
    /// Exact proposal or tool-result observation.
    #[must_use]
    pub const fn source(self) -> ObservationId { self.source }
    /// Unresolved state, never an effect authorization or receipt.
    #[must_use]
    pub const fn state(self) -> PendingOperationState { self.state }
}

/// Current literal instruction references and unresolved protocol obligations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingProtocol {
    requirements: Vec<ObservationId>,
    pending: Vec<WorkingPendingOperation>,
}
impl WorkingProtocol {
    pub(super) const fn empty() -> Self { Self { requirements: Vec::new(), pending: Vec::new() } }
    /// Validates canonical order and allocation bounds for host-owned pins.
    ///
    /// # Errors
    /// Rejects excessive, repeated, or noncanonically ordered references.
    pub fn new(requirements: Vec<ObservationId>, pending: Vec<WorkingPendingOperation>, limits: WorkingLimits) -> Result<Self, WorkingError> {
        if requirements.len() > limits.entries() || pending.len() > limits.entries() { return Err(WorkingError::Capacity); }
        let mut index = 1;
        while index < requirements.len()
            invariant index >= 1,
            decreases requirements.len() - index,
        {
            if requirements[index - 1] >= requirements[index] { return Err(WorkingError::NonCanonicalOrder); }
            index += 1;
        }
        index = 1;
        while index < pending.len()
            invariant index >= 1,
            decreases pending.len() - index,
        {
            if pending[index - 1].id >= pending[index].id { return Err(WorkingError::NonCanonicalOrder); }
            index += 1;
        }
        Ok(Self { requirements, pending })
    }
    /// Exact active host-policy/user-instruction handles. Prior versions remain in the archive.
    #[must_use]
    pub const fn requirements(&self) -> &[ObservationId] { self.requirements.as_slice() }
    /// Unresolved operations to pin even when their originating exchange leaves the prompt.
    #[must_use]
    pub const fn pending(&self) -> &[WorkingPendingOperation] { self.pending.as_slice() }
}

/// Host-only replacement of semantic pins, separately typed from model-authored deltas.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingProtocolUpdate {
    binding: WorkingBinding,
    base_revision: u64,
    protocol: WorkingProtocol,
}
impl WorkingProtocolUpdate {
    /// Binds host-owned pins to one exact state revision.
    #[must_use]
    pub const fn new(binding: WorkingBinding, base_revision: u64, protocol: WorkingProtocol) -> Self { Self { binding, base_revision, protocol } }
    /// Exact current task/role/conversation binding.
    #[must_use]
    pub const fn binding(&self) -> WorkingBinding { self.binding }
    /// Exact revision on which the host based the update.
    #[must_use]
    pub const fn base_revision(&self) -> u64 { self.base_revision }
    /// Complete bounded replacement pins.
    #[must_use]
    pub const fn protocol(&self) -> &WorkingProtocol { &self.protocol }
}

/// Validates and replaces host pins without accepting model authority or executing effects.
///
/// # Errors
/// Rejects scope/revision drift, missing sources, forged instruction origins, and capacity.
pub fn apply_working_protocol(state: &WorkingState, update: &WorkingProtocolUpdate) -> Result<WorkingState, WorkingError> {
    state.check_binding(update.binding)?;
    if state.revision != update.base_revision { return Err(WorkingError::RevisionMismatch); }
    validate_protocol(state, &update.protocol)?;
    let revision = super::state::next_revision(state.revision)?;
    let mut next = state.clone();
    next.protocol = update.protocol.clone();
    next.revision = revision;
    Ok(next)
}

pub(super) fn validate_protocol(state: &WorkingState, protocol: &WorkingProtocol) -> Result<(), WorkingError> {
    if protocol.requirements.len() > state.limits.entries() || protocol.pending.len() > state.limits.entries() { return Err(WorkingError::Capacity); }
    let mut index = 0;
    while index < protocol.requirements.len()
        invariant index <= protocol.requirements.len(),
        decreases protocol.requirements.len() - index,
    {
        let source = state.observation(state.binding(), protocol.requirements[index])?;
        if !matches!(source.kind(), ObservationKind::HostPolicy | ObservationKind::UserInstruction) { return Err(WorkingError::ConflictingEvidence); }
        index += 1;
    }
    index = 0;
    while index < protocol.pending.len()
        invariant index <= protocol.pending.len(),
        decreases protocol.pending.len() - index,
    {
        let source = state.observation(state.binding(), protocol.pending[index].source)?;
        if !matches!(source.kind(), ObservationKind::AgentMessage | ObservationKind::ToolOutput) { return Err(WorkingError::ConflictingEvidence); }
        index += 1;
    }
    Ok(())
}
}

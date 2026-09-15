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
#[derive(Debug, Eq, PartialEq)]
pub struct WorkingProtocol {
    requirements: Vec<ObservationId>,
    pending: Vec<WorkingPendingOperation>,
}

impl Clone for WorkingProtocol {
    fn clone(&self) -> (result: Self)
        ensures
            result.spec_requirements() == self.spec_requirements(),
            result.spec_pending() == self.spec_pending(),
    {
        Self {
            requirements: self.requirements.clone(),
            pending: self.pending.clone(),
        }
    }
}
impl WorkingProtocol {
    /// Logical active requirement sources.
    pub closed spec fn spec_requirements(&self) -> Seq<ObservationId> { self.requirements@ }
    /// Logical unresolved operation sequence.
    pub closed spec fn spec_pending(&self) -> Seq<WorkingPendingOperation> { self.pending@ }

    pub(super) const fn empty() -> (result: Self)
        ensures
            result.spec_requirements().len() == 0,
            result.spec_pending().len() == 0,
    { Self { requirements: Vec::new(), pending: Vec::new() } }
    /// Validates canonical order and allocation bounds for host-owned pins.
    ///
    /// # Errors
    /// Rejects excessive, repeated, or noncanonically ordered references.
    pub fn new(requirements: Vec<ObservationId>, pending: Vec<WorkingPendingOperation>, limits: WorkingLimits) -> (result: Result<Self, WorkingError>)
        ensures match result {
            Ok(protocol) => {
                &&& protocol.spec_requirements() == requirements@
                &&& protocol.spec_pending() == pending@
            }
            Err(_) => true,
        },
    {
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
    pub const fn requirements(&self) -> (result: &[ObservationId])
        ensures result@ == self.spec_requirements(),
    { self.requirements.as_slice() }
    /// Unresolved operations to pin even when their originating exchange leaves the prompt.
    #[must_use]
    pub const fn pending(&self) -> (result: &[WorkingPendingOperation])
        ensures result@ == self.spec_pending(),
    { self.pending.as_slice() }
}

/// Host-only replacement of semantic pins, separately typed from model-authored deltas.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingProtocolUpdate {
    binding: WorkingBinding,
    base_revision: u64,
    protocol: WorkingProtocol,
}
impl WorkingProtocolUpdate {
    /// Logical target binding.
    pub closed spec fn spec_binding(&self) -> WorkingBinding { self.binding }
    /// Logical base revision.
    pub closed spec fn spec_base_revision(&self) -> u64 { self.base_revision }
    /// Logical complete protocol replacement.
    pub closed spec fn spec_protocol(&self) -> WorkingProtocol { self.protocol }

    /// Binds host-owned pins to one exact state revision.
    #[must_use]
    pub const fn new(binding: WorkingBinding, base_revision: u64, protocol: WorkingProtocol) -> (result: Self)
        ensures
            result.spec_binding() == binding,
            result.spec_base_revision() == base_revision,
            result.spec_protocol() == protocol,
    { Self { binding, base_revision, protocol } }
    /// Exact current task/role/conversation binding.
    #[must_use]
    pub const fn binding(&self) -> (result: WorkingBinding)
        ensures result == self.spec_binding(),
    { self.binding }
    /// Exact revision on which the host based the update.
    #[must_use]
    pub const fn base_revision(&self) -> (result: u64)
        ensures result == self.spec_base_revision(),
    { self.base_revision }
    /// Complete bounded replacement pins.
    #[must_use]
    pub const fn protocol(&self) -> (result: &WorkingProtocol)
        ensures *result == self.spec_protocol(),
    { &self.protocol }
}

/// Validates and replaces host pins without accepting model authority or executing effects.
///
/// # Errors
/// Rejects scope/revision drift, missing sources, forged instruction origins, and capacity.
pub fn apply_working_protocol(
    state: &WorkingState,
    update: &WorkingProtocolUpdate,
) -> (result: Result<WorkingState, WorkingError>)
    ensures match result {
        Ok(next) => {
            &&& next.spec_environment().spec_binding()
                == state.spec_environment().spec_binding()
            &&& next.spec_environment().spec_candidate()
                == state.spec_environment().spec_candidate()
            &&& next.spec_environment().spec_files()
                == state.spec_environment().spec_files()
            &&& next.spec_revision() as int == state.spec_revision() as int + 1
            &&& next.spec_observations() == state.spec_observations()
            &&& super::WorkingEntry::sequence_clone_equivalent(
                state.spec_entries(), next.spec_entries(),
            )
            &&& next.spec_limits() == state.spec_limits()
            &&& next.spec_protocol().spec_requirements()
                == update.spec_protocol().spec_requirements()
            &&& next.spec_protocol().spec_pending()
                == update.spec_protocol().spec_pending()
        }
        Err(error) => error.spec_is_protocol_error(),
    },
{
    state.check_binding(update.binding)?;
    if state.revision() != update.base_revision { return Err(WorkingError::RevisionMismatch); }
    validate_protocol(state, update.protocol())?;
    let revision = super::state_revision::next_revision(state.revision())?;
    let next = state.with_protocol(update.protocol().clone(), revision);
    proof {
        assert(next.spec_revision() as int == state.spec_revision() as int + 1);
        assert(next.spec_observations() == state.spec_observations());
        assert(next.spec_protocol().spec_requirements()
            == update.spec_protocol().spec_requirements());
        assert(next.spec_protocol().spec_pending()
            == update.spec_protocol().spec_pending());
    }
    Ok(next)
}

pub(super) fn validate_protocol(
    state: &WorkingState,
    protocol: &WorkingProtocol,
) -> (result: Result<(), WorkingError>)
    ensures result.is_err() ==> result.unwrap_err().spec_is_protocol_error(),
{
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

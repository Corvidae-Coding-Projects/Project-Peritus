//! Ordered, replay-aware projections of C4 tool work.

use crate::{
    AgentErrorCode, AgentOperation, AgentRecovery, AgentRejection, ModelCallId, ToolOrdinal,
};
use peritus_policy::AuthorityInstant;
use peritus_types::{ActionId, CapabilityName, EvidenceId, RevisionTuple, Sha256Digest};
use std::collections::BTreeSet;

/// Checked semantic tool version.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ToolVersion {
    major: u16,
    minor: u16,
}

impl ToolVersion {
    /// Creates a semantic version with a positive major component.
    ///
    /// # Errors
    ///
    /// Returns `InvalidTool` when `major` is zero.
    pub const fn new(major: u16, minor: u16) -> Result<Self, AgentRejection> {
        if major == 0 {
            Err(tool_error(AgentErrorCode::InvalidTool, "tool major version must be positive"))
        } else {
            Ok(Self { major, minor })
        }
    }
    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }
    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }
}

/// Side-effect projection used for D0 serialization; C4 remains authoritative.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ToolSideEffect {
    None,
    Workspace,
    Process,
    External,
}

/// Replay classification copied from C4 preparation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ToolIdempotency {
    Idempotent,
    ReplayTerminalOnly,
    NonIdempotent,
}

/// Immutable checked tool proposal supplied after C4 preparation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolProposal {
    ordinal: ToolOrdinal,
    model_call_id: ModelCallId,
    action_id: ActionId,
    capability: CapabilityName,
    version: ToolVersion,
    argument_digest: Sha256Digest,
    prepared_digest: Sha256Digest,
    replay_identity: Sha256Digest,
    revision: RevisionTuple,
    deadline: AuthorityInstant,
    side_effect: ToolSideEffect,
    idempotency: ToolIdempotency,
}

impl ToolProposal {
    #[allow(clippy::too_many_arguments, reason = "the C4-prepared identity must remain exact")]
    #[must_use]
    pub const fn new(
        ordinal: ToolOrdinal,
        model_call_id: ModelCallId,
        action_id: ActionId,
        capability: CapabilityName,
        version: ToolVersion,
        argument_digest: Sha256Digest,
        prepared_digest: Sha256Digest,
        replay_identity: Sha256Digest,
        revision: RevisionTuple,
        deadline: AuthorityInstant,
        side_effect: ToolSideEffect,
        idempotency: ToolIdempotency,
    ) -> Self {
        Self {
            ordinal,
            model_call_id,
            action_id,
            capability,
            version,
            argument_digest,
            prepared_digest,
            replay_identity,
            revision,
            deadline,
            side_effect,
            idempotency,
        }
    }

    #[must_use]
    pub const fn ordinal(&self) -> ToolOrdinal {
        self.ordinal
    }
    #[must_use]
    pub const fn model_call_id(&self) -> ModelCallId {
        self.model_call_id
    }
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    #[must_use]
    pub const fn capability(&self) -> &CapabilityName {
        &self.capability
    }
    #[must_use]
    pub const fn version(&self) -> ToolVersion {
        self.version
    }
    #[must_use]
    pub const fn argument_digest(&self) -> Sha256Digest {
        self.argument_digest
    }
    #[must_use]
    pub const fn prepared_digest(&self) -> Sha256Digest {
        self.prepared_digest
    }
    #[must_use]
    pub const fn replay_identity(&self) -> Sha256Digest {
        self.replay_identity
    }
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    pub const fn deadline(&self) -> AuthorityInstant {
        self.deadline
    }
    #[must_use]
    pub const fn side_effect(&self) -> ToolSideEffect {
        self.side_effect
    }
    #[must_use]
    pub const fn idempotency(&self) -> ToolIdempotency {
        self.idempotency
    }
}

/// Tool lifecycle retained by the aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ToolSlotPhase {
    Proposed,
    AwaitingAuthorization,
    Authorized,
    Dispatched,
    Active,
    Terminal,
}

/// Terminal C4 result projection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ToolResultStatus {
    Succeeded,
    Failed,
    Denied,
    Cancelled,
    Indeterminate,
}

/// Bounded terminal tool result supplied by C4.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolResultRecord {
    status: ToolResultStatus,
    result_digest: Sha256Digest,
    model_visible_bytes: u64,
    evidence: Vec<EvidenceId>,
}

impl ToolResultRecord {
    pub const MAX_EVIDENCE: usize = 256;

    /// Creates a bounded canonically ordered terminal result.
    ///
    /// # Errors
    ///
    /// Returns a typed rejection when evidence is excessive, duplicated, or unordered.
    pub fn new(
        status: ToolResultStatus,
        result_digest: Sha256Digest,
        model_visible_bytes: u64,
        evidence: Vec<EvidenceId>,
    ) -> Result<Self, AgentRejection> {
        if evidence.len() > Self::MAX_EVIDENCE || evidence.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(tool_error(
                AgentErrorCode::NonCanonicalOrder,
                "tool evidence exceeds its bound or is not strictly ordered",
            ));
        }
        Ok(Self { status, result_digest, model_visible_bytes, evidence })
    }

    #[must_use]
    pub const fn status(&self) -> ToolResultStatus {
        self.status
    }
    #[must_use]
    pub const fn result_digest(&self) -> Sha256Digest {
        self.result_digest
    }
    #[must_use]
    pub const fn model_visible_bytes(&self) -> u64 {
        self.model_visible_bytes
    }
    #[must_use]
    pub fn evidence(&self) -> &[EvidenceId] {
        &self.evidence
    }
}

/// One ordered tool slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolSlot {
    proposal: ToolProposal,
    phase: ToolSlotPhase,
    authority_digest: Option<Sha256Digest>,
    next_progress_sequence: u32,
    last_progress_digest: Option<Sha256Digest>,
    result: Option<ToolResultRecord>,
}

impl ToolSlot {
    const fn proposed(proposal: ToolProposal) -> Self {
        Self {
            proposal,
            phase: ToolSlotPhase::Proposed,
            authority_digest: None,
            next_progress_sequence: 1,
            last_progress_digest: None,
            result: None,
        }
    }
    #[must_use]
    pub const fn proposal(&self) -> &ToolProposal {
        &self.proposal
    }
    #[must_use]
    pub const fn phase(&self) -> ToolSlotPhase {
        self.phase
    }
    #[must_use]
    pub const fn authority_digest(&self) -> Option<Sha256Digest> {
        self.authority_digest
    }
    #[must_use]
    pub const fn next_progress_sequence(&self) -> u32 {
        self.next_progress_sequence
    }
    #[must_use]
    pub const fn last_progress_digest(&self) -> Option<Sha256Digest> {
        self.last_progress_digest
    }
    #[must_use]
    pub const fn result(&self) -> Option<&ToolResultRecord> {
        self.result.as_ref()
    }
}

/// Canonically ordered current batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolBatch {
    slots: Vec<ToolSlot>,
}

impl ToolBatch {
    pub(super) fn new(
        proposals: Vec<ToolProposal>,
        revision: RevisionTuple,
        max: u16,
    ) -> Result<Self, AgentRejection> {
        if proposals.is_empty() || proposals.len() > usize::from(max) {
            return Err(tool_error(
                AgentErrorCode::InvalidLimit,
                "tool batch is empty or exceeds the turn limit",
            ));
        }
        let mut actions = BTreeSet::new();
        let mut calls = BTreeSet::new();
        let mut mutation_count = 0_u16;
        for (index, proposal) in proposals.iter().enumerate() {
            let expected = u16::try_from(index).map_err(|_| {
                tool_error(AgentErrorCode::InvalidLimit, "tool ordinal exceeds representation")
            })?;
            if proposal.ordinal.get() != expected
                || proposal.revision != revision
                || !actions.insert(proposal.action_id)
                || !calls.insert(proposal.model_call_id)
            {
                return Err(tool_error(
                    AgentErrorCode::NonCanonicalOrder,
                    "tool proposals are unordered, duplicated, or stale",
                ));
            }
            if proposal.side_effect == ToolSideEffect::Workspace {
                mutation_count += 1;
            }
        }
        if mutation_count > 0 && proposals.len() > 1 {
            return Err(tool_error(
                AgentErrorCode::InvalidTool,
                "workspace mutations must be serialized",
            ));
        }
        Ok(Self { slots: proposals.into_iter().map(ToolSlot::proposed).collect() })
    }

    #[must_use]
    pub fn slots(&self) -> &[ToolSlot] {
        &self.slots
    }
    #[must_use]
    pub fn all_terminal(&self) -> bool {
        self.slots.iter().all(|slot| slot.phase == ToolSlotPhase::Terminal)
    }
    #[must_use]
    pub fn has_indeterminate(&self) -> bool {
        self.slots.iter().any(|slot| {
            slot.result
                .as_ref()
                .is_some_and(|result| result.status == ToolResultStatus::Indeterminate)
        })
    }
    pub(super) fn slot_mut(
        &mut self,
        ordinal: ToolOrdinal,
    ) -> Result<&mut ToolSlot, AgentRejection> {
        self.slots
            .get_mut(usize::from(ordinal.get()))
            .ok_or_else(|| tool_error(AgentErrorCode::InvalidTool, "unknown tool ordinal"))
    }

    pub(super) fn slot_mut_for_read(
        &self,
        ordinal: ToolOrdinal,
    ) -> Result<ToolSlotPhase, AgentRejection> {
        self.slots
            .get(usize::from(ordinal.get()))
            .map(ToolSlot::phase)
            .ok_or_else(|| tool_error(AgentErrorCode::InvalidTool, "unknown tool ordinal"))
    }
}

pub const fn tool_error(code: AgentErrorCode, detail: &'static str) -> AgentRejection {
    AgentRejection::new(code, AgentOperation::ValidateTools, AgentRecovery::CorrectRequest, detail)
}

pub fn set_awaiting(batch: &mut ToolBatch) {
    for slot in &mut batch.slots {
        slot.phase = ToolSlotPhase::AwaitingAuthorization;
    }
}
pub const fn authorize(slot: &mut ToolSlot, digest: Sha256Digest) {
    slot.phase = ToolSlotPhase::Authorized;
    slot.authority_digest = Some(digest);
}
pub fn terminal(slot: &mut ToolSlot, result: ToolResultRecord) {
    slot.phase = ToolSlotPhase::Terminal;
    slot.result = Some(result);
}
pub const fn dispatch(slot: &mut ToolSlot) {
    slot.phase = ToolSlotPhase::Dispatched;
}
pub const fn activate(slot: &mut ToolSlot) {
    slot.phase = ToolSlotPhase::Active;
}
pub const fn progress(slot: &mut ToolSlot, digest: Sha256Digest) {
    slot.next_progress_sequence += 1;
    slot.last_progress_digest = Some(digest);
}
